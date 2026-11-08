//! Prefork zygote for HTTP workers (Linux).
//!
//! Separately `exec`'d workers each own a private OPcache segment, so N workers
//! keep N copies of every compiled script and JIT trace competing for the same
//! last-level cache and resident memory. Instead, one zygote process starts the
//! Zend engine (and its OPcache/JIT shared memory) once and forks workers when
//! the cluster master asks for them, exactly like PHP-FPM children share one
//! segment. The fork happens before Composer, the PAM runtime or the
//! application execute any PHP code, so every worker still boots from pristine
//! PHP state (random seeds, connections, globals).
//!
//! The zygote sleeps in `poll(2)` without a timeout and is woken only by a
//! master command or a `SIGCHLD` delivered through a `signalfd`.

use std::io;
use std::process::{Child, ExitStatus};

#[cfg(target_os = "linux")]
pub use linux::{Zygote, ZygoteOutcome, serve};

/// A worker process started either with `exec` or by a zygote fork.
pub enum WorkerProcess {
    Spawned(Child),
    #[cfg(target_os = "linux")]
    Forked {
        pid: u32,
        zygote: std::rc::Rc<std::cell::RefCell<Zygote>>,
        status: Option<ExitStatus>,
    },
}

impl WorkerProcess {
    pub fn id(&self) -> u32 {
        match self {
            Self::Spawned(child) => child.id(),
            #[cfg(target_os = "linux")]
            Self::Forked { pid, .. } => *pid,
        }
    }

    #[cfg(target_os = "linux")]
    pub fn zygote(&self) -> Option<std::rc::Rc<std::cell::RefCell<Zygote>>> {
        match self {
            Self::Spawned(_) => None,
            Self::Forked { zygote, .. } => Some(zygote.clone()),
        }
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        match self {
            Self::Spawned(child) => child.try_wait(),
            #[cfg(target_os = "linux")]
            Self::Forked {
                pid,
                zygote,
                status,
            } => {
                use std::os::unix::process::ExitStatusExt;
                if status.is_none() {
                    let mut zygote = zygote.borrow_mut();
                    if let Some(exit) = zygote.take_exit(*pid) {
                        *status = Some(exit);
                    } else if !zygote.is_running() {
                        // PR_SET_PDEATHSIG already killed workers of a dead zygote.
                        *status = Some(ExitStatus::from_raw(libc::SIGKILL));
                    }
                }
                Ok(*status)
            }
        }
    }

    pub fn signal(&mut self, signal: i32) {
        if matches!(self.try_wait(), Ok(Some(_))) {
            return;
        }
        match self {
            Self::Spawned(child) => {
                let Ok(pid) = i32::try_from(child.id()) else {
                    return;
                };
                // SAFETY: this master is the parent and `try_wait` has not
                // reaped the child, so the PID cannot have been recycled.
                unsafe { libc::kill(pid, signal) };
            }
            // Only the zygote, the parent of a forked worker, knows whether the
            // PID is still unreaped; signalling it from here could hit a
            // recycled PID after the zygote reaped the worker.
            #[cfg(target_os = "linux")]
            Self::Forked { pid, zygote, .. } => zygote.borrow_mut().signal_worker(*pid, signal),
        }
    }

    pub fn kill(&mut self) {
        match self {
            Self::Spawned(child) => {
                let _ = child.kill();
            }
            #[cfg(target_os = "linux")]
            Self::Forked { .. } => self.signal(libc::SIGKILL),
        }
    }

    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        match self {
            Self::Spawned(child) => child.wait(),
            #[cfg(target_os = "linux")]
            Self::Forked { .. } => loop {
                if let Some(status) = self.try_wait()? {
                    return Ok(status);
                }
                if let Self::Forked { zygote, .. } = self {
                    zygote
                        .borrow()
                        .wait_for_events(std::time::Duration::from_millis(100));
                }
            },
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::collections::{HashMap, VecDeque};
    use std::fs::File;
    use std::io::{self, Read, Write};
    use std::os::fd::{AsRawFd, FromRawFd, RawFd};
    use std::os::unix::process::{CommandExt, ExitStatusExt};
    use std::process::{Child, Command, ExitStatus};
    use std::time::{Duration, Instant};

    use serde::{Deserialize, Serialize};

    use crate::php::PhpEngine;

    const COMMAND_FD_ENV: &str = "PAM_ZYGOTE_COMMAND_FD";
    const EVENT_FD_ENV: &str = "PAM_ZYGOTE_EVENT_FD";
    const SPAWN_TIMEOUT: Duration = Duration::from_secs(10);
    const SIGINFO_BYTES: usize = 128;

    #[derive(Debug, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase", tag = "command")]
    enum ZygoteCommand {
        Spawn {
            env: Vec<(String, String)>,
        },
        /// Signals a worker only while the zygote still holds it unreaped.
        Signal {
            pid: u32,
            signal: i32,
        },
    }

    #[derive(Debug, Deserialize, Serialize)]
    #[serde(rename_all = "camelCase", tag = "event")]
    enum ZygoteEvent {
        Spawned { pid: u32 },
        Failed { message: String },
        Exited { pid: u32, status: i32 },
    }

    pub enum ZygoteOutcome {
        /// The master closed the command channel; the zygote process should exit.
        Exit,
        /// This process is a freshly forked worker that must continue booting.
        Worker(PhpEngine),
    }

    /// Runs the zygote loop on the current thread, which must be the only one.
    pub fn serve(mut engine: PhpEngine) -> Result<ZygoteOutcome, String> {
        let command_fd = inherited_fd(COMMAND_FD_ENV)?;
        let event_fd = inherited_fd(EVENT_FD_ENV)?;
        // SAFETY: both descriptors were handed over by the master for this process.
        let mut commands = unsafe { File::from_raw_fd(command_fd) };
        let mut events = unsafe { File::from_raw_fd(event_fd) };
        // Workers must not see the zygote's private channel.
        // SAFETY: command dispatch happens before PHP code or threads run.
        unsafe {
            std::env::remove_var(COMMAND_FD_ENV);
            std::env::remove_var(EVENT_FD_ENV);
        }

        if let Err(error) = crate::balance::install_shared_table() {
            eprintln!("pam: connection balancing disabled: {error}");
        }
        // SAFETY: getpid has no preconditions.
        let zygote_pid = unsafe { libc::getpid() };
        // SAFETY: zeroed sigset_t values are initialized by sigemptyset/sigprocmask.
        let mut child_signal: libc::sigset_t = unsafe { std::mem::zeroed() };
        let mut previous_mask: libc::sigset_t = unsafe { std::mem::zeroed() };
        // SAFETY: the sets are valid, and this single-threaded process owns its mask.
        let signal_fd = unsafe {
            libc::sigemptyset(&mut child_signal);
            libc::sigaddset(&mut child_signal, libc::SIGCHLD);
            if libc::sigprocmask(libc::SIG_BLOCK, &child_signal, &mut previous_mask) != 0 {
                return Err(format!(
                    "cannot block SIGCHLD: {}",
                    io::Error::last_os_error()
                ));
            }
            libc::signalfd(-1, &child_signal, libc::SFD_CLOEXEC | libc::SFD_NONBLOCK)
        };
        if signal_fd < 0 {
            return Err(format!(
                "cannot create the zygote signalfd: {}",
                io::Error::last_os_error()
            ));
        }
        // SAFETY: signalfd returned a new descriptor owned by this File.
        let signals = unsafe { File::from_raw_fd(signal_fd) };

        ensure_single_threaded()?;
        let mut live = std::collections::HashSet::<libc::pid_t>::new();
        let mut pending = Vec::new();
        loop {
            let mut descriptors = [
                libc::pollfd {
                    fd: commands.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: signals.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            // SAFETY: the array outlives the call and its length is passed exactly.
            let ready = unsafe { libc::poll(descriptors.as_mut_ptr(), 2, -1) };
            if ready < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(format!("zygote poll failed: {error}"));
            }

            if descriptors[1].revents != 0 {
                drain(&signals);
                reap_children(&mut events, &mut live)?;
            }
            if descriptors[0].revents == 0 {
                continue;
            }

            let mut chunk = [0_u8; 4096];
            let read = match commands.read(&mut chunk) {
                Ok(0) => return Ok(ZygoteOutcome::Exit),
                Ok(read) => read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(format!("cannot read zygote command: {error}")),
            };
            pending.extend_from_slice(&chunk[..read]);
            while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
                let line = pending.drain(..=end).collect::<Vec<_>>();
                let env = match serde_json::from_slice::<ZygoteCommand>(&line[..line.len() - 1]) {
                    Ok(ZygoteCommand::Spawn { env }) => env,
                    Ok(ZygoteCommand::Signal { pid, signal }) => {
                        // Exit notifications are processed on this same thread,
                        // so a PID in `live` is a child that has not been
                        // reaped yet and cannot have been reused.
                        if let Ok(pid) = libc::pid_t::try_from(pid)
                            && live.contains(&pid)
                        {
                            // SAFETY: `pid` is an unreaped child of this process.
                            unsafe { libc::kill(pid, signal) };
                        }
                        continue;
                    }
                    Err(error) => {
                        emit(
                            &mut events,
                            &ZygoteEvent::Failed {
                                message: format!("invalid zygote command: {error}"),
                            },
                        )?;
                        continue;
                    }
                };
                if let Err(message) = ensure_single_threaded() {
                    emit(&mut events, &ZygoteEvent::Failed { message })?;
                    continue;
                }
                // SAFETY: the zygote was just verified to be single-threaded, so
                // the child inherits a consistent address space; it only touches
                // its own state below.
                let pid = unsafe { libc::fork() };
                if pid < 0 {
                    emit(
                        &mut events,
                        &ZygoteEvent::Failed {
                            message: format!("fork failed: {}", io::Error::last_os_error()),
                        },
                    )?;
                    continue;
                }
                if pid == 0 {
                    // SAFETY: restores the inherited mask and ties the worker's
                    // lifetime to the zygote, which is itself tied to the master.
                    unsafe {
                        libc::sigprocmask(libc::SIG_SETMASK, &previous_mask, std::ptr::null_mut());
                        if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0
                            || libc::getppid() != zygote_pid
                        {
                            libc::_exit(70);
                        }
                    }
                    drop(signals);
                    drop(commands);
                    drop(events);
                    for (name, value) in env {
                        // SAFETY: the forked worker is single-threaded until PHP
                        // finishes booting and the HTTP server starts.
                        unsafe { std::env::set_var(name, value) };
                    }
                    engine.adopt_after_fork();
                    return Ok(ZygoteOutcome::Worker(engine));
                }
                live.insert(pid);
                emit(&mut events, &ZygoteEvent::Spawned { pid: pid as u32 })?;
            }
        }
    }

    fn inherited_fd(name: &str) -> Result<RawFd, String> {
        std::env::var(name)
            .ok()
            .and_then(|value| value.parse::<RawFd>().ok())
            .filter(|fd| *fd > 2)
            .ok_or_else(|| format!("the zygote channel {name} is missing"))
    }

    fn drain(file: &File) {
        let mut buffer = [0_u8; SIGINFO_BYTES * 8];
        loop {
            // SAFETY: the buffer is valid for its full length.
            let read =
                unsafe { libc::read(file.as_raw_fd(), buffer.as_mut_ptr().cast(), buffer.len()) };
            if read <= 0 {
                return;
            }
        }
    }

    /// `fork(2)` only duplicates the calling thread: a lock held by any other
    /// thread (allocator, stdio, PHP/TSRM) would stay locked forever in the
    /// worker. The zygote therefore refuses to fork unless the kernel reports
    /// exactly one thread for this process.
    pub(super) fn ensure_single_threaded() -> Result<(), String> {
        let status = std::fs::read_to_string("/proc/self/status")
            .map_err(|error| format!("cannot verify the zygote thread count: {error}"))?;
        match thread_count(&status) {
            Some(1) => Ok(()),
            Some(count) => Err(format!(
                "the PHP zygote has {count} threads; it must be single-threaded to fork workers"
            )),
            None => Err("cannot verify the zygote thread count".to_owned()),
        }
    }

    pub(super) fn thread_count(status: &str) -> Option<usize> {
        status
            .lines()
            .find_map(|line| line.strip_prefix("Threads:"))
            .and_then(|value| value.trim().parse().ok())
    }

    fn reap_children(
        events: &mut File,
        live: &mut std::collections::HashSet<libc::pid_t>,
    ) -> Result<(), String> {
        loop {
            let mut status = 0;
            // SAFETY: status points to a valid integer.
            let pid = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
            if pid <= 0 {
                return Ok(());
            }
            live.remove(&pid);
            emit(
                events,
                &ZygoteEvent::Exited {
                    pid: pid as u32,
                    status,
                },
            )?;
        }
    }

    fn emit(events: &mut File, event: &ZygoteEvent) -> Result<(), String> {
        let mut line = serde_json::to_vec(event).map_err(|error| error.to_string())?;
        line.push(b'\n');
        events
            .write_all(&line)
            .map_err(|error| format!("cannot report to the PAM master: {error}"))
    }

    #[cfg(test)]
    pub(super) fn test_signal_command(pid: u32, signal: i32) -> impl Serialize {
        ZygoteCommand::Signal { pid, signal }
    }

    /// Master-side handle of one zygote process.
    pub struct Zygote {
        child: Child,
        commands: File,
        events: File,
        pending: Vec<u8>,
        spawned: VecDeque<Result<u32, String>>,
        exits: HashMap<u32, i32>,
        disconnected: bool,
    }

    impl Zygote {
        /// Starts `command` (a `pam __zygote ...` invocation) with its channel.
        pub fn start(mut command: Command) -> Result<Self, String> {
            let (command_read, command_write) = pipe()?;
            let (event_read, event_write) = pipe()?;
            let command_fd = command_read.as_raw_fd();
            let event_fd = event_write.as_raw_fd();
            command
                .env(COMMAND_FD_ENV, command_fd.to_string())
                .env(EVENT_FD_ENV, event_fd.to_string());
            // SAFETY: only async-signal-safe fcntl calls run between fork and exec.
            unsafe {
                command.pre_exec(move || {
                    for fd in [command_fd, event_fd] {
                        if libc::fcntl(fd, libc::F_SETFD, 0) != 0 {
                            return Err(io::Error::last_os_error());
                        }
                    }
                    Ok(())
                });
            }
            let child = command
                .spawn()
                .map_err(|error| format!("cannot start the PHP zygote: {error}"))?;
            drop(command_read);
            drop(event_write);
            // SAFETY: the descriptor is owned by `event_read`.
            let flags = unsafe { libc::fcntl(event_read.as_raw_fd(), libc::F_GETFL) };
            // SAFETY: same descriptor, flags obtained above.
            if flags < 0
                || unsafe {
                    libc::fcntl(
                        event_read.as_raw_fd(),
                        libc::F_SETFL,
                        flags | libc::O_NONBLOCK,
                    )
                } != 0
            {
                return Err(format!(
                    "cannot configure the zygote channel: {}",
                    io::Error::last_os_error()
                ));
            }
            Ok(Self {
                child,
                commands: command_write,
                events: event_read,
                pending: Vec::new(),
                spawned: VecDeque::new(),
                exits: HashMap::new(),
                disconnected: false,
            })
        }

        /// Asks the zygote for one worker with `env` applied after the fork.
        pub fn fork_worker(&mut self, env: Vec<(String, String)>) -> Result<u32, String> {
            self.send(&ZygoteCommand::Spawn { env })?;
            let deadline = Instant::now() + SPAWN_TIMEOUT;
            loop {
                self.pump();
                if let Some(result) = self.spawned.pop_front() {
                    return result;
                }
                if self.disconnected {
                    return Err(self.exit_description());
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err("the PHP zygote did not fork a worker in time".to_owned());
                }
                self.wait_for_events(remaining);
            }
        }

        fn send(&mut self, command: &ZygoteCommand) -> Result<(), String> {
            let mut line = serde_json::to_vec(command)
                .map_err(|error| format!("cannot encode zygote command: {error}"))?;
            line.push(b'\n');
            self.commands
                .write_all(&line)
                .map_err(|error| format!("the PHP zygote is unavailable: {error}"))
        }

        /// Asks the zygote to deliver `signal` to a forked worker. The zygote
        /// drops the request when it has already reaped `pid`.
        pub(super) fn signal_worker(&mut self, pid: u32, signal: i32) {
            self.pump();
            if self.exits.contains_key(&pid) || self.disconnected {
                return;
            }
            if let Err(error) = self.send(&ZygoteCommand::Signal { pid, signal }) {
                eprintln!("pam: cannot signal worker {pid}: {error}");
            }
        }

        /// Returns the exit status of a worker the zygote has reaped.
        pub(super) fn take_exit(&mut self, pid: u32) -> Option<ExitStatus> {
            self.pump();
            self.exits.remove(&pid).map(ExitStatus::from_raw)
        }

        pub fn is_running(&mut self) -> bool {
            self.pump();
            !self.disconnected && matches!(self.child.try_wait(), Ok(None))
        }

        pub(super) fn wait_for_events(&self, timeout: Duration) {
            let mut descriptor = libc::pollfd {
                fd: self.events.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let millis = timeout.as_millis().min(i32::MAX as u128) as i32;
            // SAFETY: one valid pollfd is passed.
            unsafe { libc::poll(&mut descriptor, 1, millis.max(1)) };
        }

        fn pump(&mut self) {
            let mut chunk = [0_u8; 4096];
            loop {
                match self.events.read(&mut chunk) {
                    Ok(0) => {
                        self.disconnected = true;
                        break;
                    }
                    Ok(read) => self.pending.extend_from_slice(&chunk[..read]),
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(_) => {
                        self.disconnected = true;
                        break;
                    }
                }
            }
            while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
                let line = self.pending.drain(..=end).collect::<Vec<_>>();
                match serde_json::from_slice::<ZygoteEvent>(&line[..line.len() - 1]) {
                    Ok(ZygoteEvent::Spawned { pid }) => self.spawned.push_back(Ok(pid)),
                    Ok(ZygoteEvent::Failed { message }) => self.spawned.push_back(Err(message)),
                    Ok(ZygoteEvent::Exited { pid, status }) => {
                        self.exits.insert(pid, status);
                    }
                    Err(error) => eprintln!("pam: invalid zygote event: {error}"),
                }
            }
        }

        fn exit_description(&mut self) -> String {
            match self.child.try_wait() {
                Ok(Some(status)) => format!("the PHP zygote exited with {status}"),
                _ => "the PHP zygote closed its channel".to_owned(),
            }
        }
    }

    impl Drop for Zygote {
        fn drop(&mut self) {
            // Workers are already stopped when the last handle goes away. Closing
            // the channel lets the zygote exit; SIGKILL covers a wedged process.
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn pipe() -> Result<(File, File), String> {
        let mut fds = [0; 2];
        // SAFETY: fds has room for the two descriptors pipe2 returns.
        if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
            return Err(format!(
                "cannot create the zygote channel: {}",
                io::Error::last_os_error()
            ));
        }
        // SAFETY: pipe2 returned two new descriptors owned by these Files.
        Ok(unsafe { (File::from_raw_fd(fds[0]), File::from_raw_fd(fds[1])) })
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::linux::{ensure_single_threaded, thread_count};

    #[test]
    fn parses_the_thread_count() {
        assert_eq!(
            thread_count("Name:\tpam\nThreads:\t1\nVmRSS: 1 kB\n"),
            Some(1)
        );
        assert_eq!(thread_count("Threads:   12\n"), Some(12));
        assert_eq!(thread_count("Name: pam\n"), None);
    }

    #[test]
    fn refuses_to_fork_from_a_multi_threaded_process() {
        // The test harness runs tests on worker threads, so this process is
        // never single-threaded here.
        let (sender, receiver) = std::sync::mpsc::channel::<()>();
        let helper = std::thread::spawn(move || receiver.recv());
        let error = ensure_single_threaded().unwrap_err();
        assert!(error.contains("must be single-threaded"), "{error}");
        drop(sender);
        let _ = helper.join();
    }

    #[test]
    fn signal_commands_round_trip() {
        let line = serde_json::to_string(&super::linux::test_signal_command(42, 15)).unwrap();
        assert_eq!(line, r#"{"command":"signal","pid":42,"signal":15}"#);
    }
}
