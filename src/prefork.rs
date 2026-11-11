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
//! With `PAM_PREFORK_BOOT=app` (the default for `pam:octane`) the zygote goes
//! further: it boots the application once (Composer, the PAM runtime, the
//! script up to the point where it registered its HTTP handler) and forks
//! workers from that warm state, so a worker is ready in milliseconds and
//! every page the boot touched is shared copy-on-write. Process-bound state is
//! made safe generically before and after the fork (see `fork_hygiene`), and a
//! zygote that cannot fork its booted state safely re-executes itself in the
//! pre-boot mode with one log line saying why.
//!
//! The zygote sleeps in `poll(2)` without a timeout and is woken only by a
//! master command or a `SIGCHLD` delivered through a `signalfd`.

use std::io;
use std::process::{Child, ExitStatus};

#[cfg(target_os = "linux")]
pub use linux::{
    BOOT_MODE_ENV, Channel, Zygote, ZygoteOutcome, ensure_single_threaded, fork_hygiene, serve,
};

/// What a zygote has initialized before it forks workers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootMode {
    /// Only the Zend engine and OPcache: workers boot the application.
    Engine,
    /// The whole application: workers start serving right after the fork.
    Application,
}

impl BootMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "engine" | "pre" | "preboot" | "pre-boot" | "0" | "off" | "false" => Some(Self::Engine),
            "app" | "application" | "post" | "postboot" | "post-boot" | "1" | "on" | "true" => {
                Some(Self::Application)
            }
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Engine => "engine",
            Self::Application => "app",
        }
    }
}

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

    const COMMAND_FD_ENV: &str = "PAM_ZYGOTE_COMMAND_FD";
    const EVENT_FD_ENV: &str = "PAM_ZYGOTE_EVENT_FD";
    /// `engine` or `app`; set by the master on the zygote command.
    pub const BOOT_MODE_ENV: &str = "PAM_PREFORK_BOOT";
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

    pub enum ZygoteOutcome<T> {
        /// The master closed the command channel; the zygote process should exit.
        Exit,
        /// This process is a freshly forked worker that must continue booting.
        Worker(T),
    }

    /// The zygote's private pipes to the master.
    pub struct Channel {
        commands: File,
        events: File,
        /// The environment the master started this zygote with, before the
        /// application could add to it (Laravel puts `.env` with `putenv`).
        environment: Vec<(std::ffi::OsString, std::ffi::OsString)>,
    }

    impl Channel {
        /// Takes ownership of the channel the master handed over. The
        /// descriptors become close-on-exec, so nothing the application starts
        /// while the zygote boots it can inherit them.
        pub fn claim() -> Result<Self, String> {
            let command_fd = inherited_fd(COMMAND_FD_ENV)?;
            let event_fd = inherited_fd(EVENT_FD_ENV)?;
            // SAFETY: both descriptors were handed over by the master for this process.
            let (commands, events) =
                unsafe { (File::from_raw_fd(command_fd), File::from_raw_fd(event_fd)) };
            // Workers must not see the zygote's private channel.
            // SAFETY: command dispatch happens before PHP code or threads run.
            unsafe {
                std::env::remove_var(COMMAND_FD_ENV);
                std::env::remove_var(EVENT_FD_ENV);
            }
            for fd in [command_fd, event_fd] {
                set_cloexec(fd, true)?;
            }
            Ok(Self {
                commands,
                events,
                environment: std::env::vars_os().collect(),
            })
        }

        pub fn descriptors(&self) -> [RawFd; 2] {
            [self.commands.as_raw_fd(), self.events.as_raw_fd()]
        }

        /// Replaces this process with a pre-boot (`engine`) zygote that keeps
        /// the same PID and channel, so the master never notices the switch.
        /// Every other descriptor the boot opened is closed first: the new
        /// image has no PHP state that could use them.
        pub fn reexec_engine_mode(self, reason: &str) -> String {
            eprintln!(
                "pam: post-boot zygote disabled ({reason}); falling back to pre-boot forking"
            );
            let keep = self.descriptors();
            let executable = match std::env::current_exe() {
                Ok(path) => path,
                Err(error) => return format!("cannot locate the PAM executable: {error}"),
            };
            for fd in open_descriptors() {
                if fd > 2 && !keep.contains(&fd) {
                    // SAFETY: the process is single-threaded and about to exec;
                    // nothing uses these descriptors any more.
                    unsafe { libc::close(fd) };
                }
            }
            for fd in keep {
                if let Err(error) = set_cloexec(fd, false) {
                    return error;
                }
            }
            let error = Command::new(executable)
                .args(std::env::args_os().skip(1))
                .env_clear()
                .envs(self.environment)
                .env(COMMAND_FD_ENV, keep[0].to_string())
                .env(EVENT_FD_ENV, keep[1].to_string())
                .env(BOOT_MODE_ENV, super::BootMode::Engine.as_str())
                .exec();
            format!("cannot re-execute the zygote in pre-boot mode: {error}")
        }
    }

    fn set_cloexec(fd: RawFd, enabled: bool) -> Result<(), String> {
        // SAFETY: fcntl on a descriptor this process owns.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        let flags = if enabled {
            flags | libc::FD_CLOEXEC
        } else {
            flags & !libc::FD_CLOEXEC
        };
        // SAFETY: same descriptor, flags derived from F_GETFD.
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFD, flags) } != 0 {
            return Err(format!(
                "cannot configure the zygote channel: {}",
                io::Error::last_os_error()
            ));
        }
        Ok(())
    }

    pub(super) fn open_descriptors() -> Vec<RawFd> {
        let Ok(entries) = std::fs::read_dir("/proc/self/fd") else {
            return Vec::new();
        };
        let mut descriptors = entries
            .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse::<RawFd>().ok())
            .collect::<Vec<_>>();
        descriptors.sort_unstable();
        descriptors
    }

    /// Runs the zygote loop on the current thread, which must be the only one.
    /// `after_fork` runs in each forked worker once the per-worker environment
    /// is applied and the zygote's private descriptors are closed.
    pub fn serve<T>(
        channel: Channel,
        mut state: T,
        after_fork: impl Fn(&mut T) -> Result<(), String>,
    ) -> Result<ZygoteOutcome<T>, String> {
        let Channel {
            mut commands,
            mut events,
            ..
        } = channel;

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
                    prefer_worker_for_oom_kill();
                    for (name, value) in env {
                        // SAFETY: the forked worker is single-threaded until PHP
                        // finishes booting and the HTTP server starts.
                        unsafe { std::env::set_var(name, value) };
                    }
                    if let Err(error) = after_fork(&mut state) {
                        eprintln!("pam: worker initialization after fork failed: {error}");
                        // SAFETY: leave without running the zygote's PHP
                        // shutdown functions or Rust destructors.
                        unsafe { libc::_exit(70) };
                    }
                    return Ok(ZygoteOutcome::Worker(state));
                }
                live.insert(pid);
                emit(&mut events, &ZygoteEvent::Spawned { pid: pid as u32 })?;
            }
        }
    }

    /// Generic handling of descriptors that must not cross a fork boundary
    /// when workers are forked from a booted application.
    pub mod fork_hygiene {
        use std::io;
        use std::os::fd::RawFd;

        /// A socket the booted application left open in the zygote.
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct DisconnectedSocket {
            pub fd: RawFd,
            pub description: String,
        }

        /// Disconnects every socket (connected or listening) the application
        /// left open after its before-fork hooks, except `keep` and stdio.
        ///
        /// Each one is replaced in place by a fresh, unconnected socket of the
        /// same family and type: the descriptor number stays valid for the
        /// PHP object that owns it, the real connection is closed exactly once
        /// (here, before any worker exists), and a worker that uses it gets a
        /// "not connected" error that client libraries treat as a lost
        /// connection and reconnect from, instead of silently sharing one
        /// connection between processes.
        pub fn disconnect_inherited_sockets(keep: &[RawFd]) -> Vec<DisconnectedSocket> {
            let candidates = super::open_descriptors()
                .into_iter()
                .filter(|fd| *fd > 2 && !keep.contains(fd))
                .collect::<Vec<_>>();
            disconnect_sockets(&candidates)
        }

        pub(crate) fn disconnect_sockets(descriptors: &[RawFd]) -> Vec<DisconnectedSocket> {
            let mut disconnected = Vec::new();
            for &fd in descriptors {
                if !is_socket(fd) {
                    continue;
                }
                let description = describe_socket(fd);
                match replace_with_unconnected(fd) {
                    Ok(()) => disconnected.push(DisconnectedSocket { fd, description }),
                    Err(error) => {
                        eprintln!(
                            "pam: cannot disconnect inherited socket {fd} ({description}): {error}"
                        );
                    }
                }
            }
            disconnected
        }

        /// Gives this (forked) process its own open file description for every
        /// regular file it inherited, so workers never share a file offset or
        /// an `flock` with each other or the zygote. Returns how many files
        /// were reopened.
        pub fn reopen_regular_files() -> usize {
            let candidates = super::open_descriptors()
                .into_iter()
                .filter(|fd| *fd > 2)
                .collect::<Vec<_>>();
            reopen_files(&candidates)
        }

        pub(crate) fn reopen_files(descriptors: &[RawFd]) -> usize {
            let mut reopened = 0;
            for &fd in descriptors {
                if reopen_regular_file(fd).unwrap_or(false) {
                    reopened += 1;
                }
            }
            reopened
        }

        fn stat(fd: RawFd) -> Option<libc::stat> {
            // SAFETY: zeroed stat is a valid out-parameter for fstat.
            let mut status: libc::stat = unsafe { std::mem::zeroed() };
            // SAFETY: fstat writes into the valid buffer above.
            (unsafe { libc::fstat(fd, &mut status) } == 0).then_some(status)
        }

        fn is_socket(fd: RawFd) -> bool {
            stat(fd).is_some_and(|status| status.st_mode & libc::S_IFMT == libc::S_IFSOCK)
        }

        fn socket_option(fd: RawFd, option: libc::c_int) -> Option<libc::c_int> {
            let mut value: libc::c_int = 0;
            let mut length = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
            // SAFETY: value/length describe a valid c_int buffer.
            let status = unsafe {
                libc::getsockopt(
                    fd,
                    libc::SOL_SOCKET,
                    option,
                    (&mut value as *mut libc::c_int).cast(),
                    &mut length,
                )
            };
            (status == 0).then_some(value)
        }

        fn address(fd: RawFd, peer: bool) -> Option<String> {
            // SAFETY: zeroed sockaddr_storage is a valid out-parameter.
            let mut storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
            let mut length = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
            let pointer = (&mut storage as *mut libc::sockaddr_storage).cast::<libc::sockaddr>();
            // SAFETY: storage/length describe a valid sockaddr_storage buffer.
            let status = unsafe {
                if peer {
                    libc::getpeername(fd, pointer, &mut length)
                } else {
                    libc::getsockname(fd, pointer, &mut length)
                }
            };
            if status != 0 {
                return None;
            }
            match storage.ss_family as libc::c_int {
                libc::AF_INET => {
                    // SAFETY: the kernel filled a sockaddr_in for AF_INET.
                    let address = unsafe { *(pointer as *const libc::sockaddr_in) };
                    let ip = std::net::Ipv4Addr::from(u32::from_be(address.sin_addr.s_addr));
                    Some(format!("{ip}:{}", u16::from_be(address.sin_port)))
                }
                libc::AF_INET6 => {
                    // SAFETY: the kernel filled a sockaddr_in6 for AF_INET6.
                    let address = unsafe { *(pointer as *const libc::sockaddr_in6) };
                    let ip = std::net::Ipv6Addr::from(address.sin6_addr.s6_addr);
                    Some(format!("[{ip}]:{}", u16::from_be(address.sin6_port)))
                }
                libc::AF_UNIX => {
                    // SAFETY: the kernel filled a sockaddr_un for AF_UNIX.
                    let address = unsafe { &*(pointer as *const libc::sockaddr_un) };
                    let path = address
                        .sun_path
                        .iter()
                        .take_while(|byte| **byte != 0)
                        .map(|byte| byte.to_ne_bytes()[0])
                        .collect::<Vec<_>>();
                    Some(format!("unix:{}", String::from_utf8_lossy(&path)))
                }
                family => Some(format!("family {family}")),
            }
        }

        pub(crate) fn describe_socket(fd: RawFd) -> String {
            if socket_option(fd, libc::SO_ACCEPTCONN) == Some(1) {
                return format!(
                    "listener {}",
                    address(fd, false).unwrap_or_else(|| "unknown".to_owned())
                );
            }
            match address(fd, true) {
                Some(peer) => format!("connection to {peer}"),
                None => format!(
                    "unconnected socket {}",
                    address(fd, false).unwrap_or_else(|| "unknown".to_owned())
                ),
            }
        }

        fn replace_with_unconnected(fd: RawFd) -> io::Result<()> {
            let kind = socket_option(fd, libc::SO_TYPE).ok_or_else(io::Error::last_os_error)?;
            let family = socket_option(fd, libc::SO_DOMAIN).ok_or_else(io::Error::last_os_error)?;
            // SAFETY: fcntl on a descriptor this process owns.
            let descriptor_flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
            // SAFETY: plain socket creation.
            let replacement = unsafe { libc::socket(family, kind | libc::SOCK_CLOEXEC, 0) };
            if replacement < 0 {
                return Err(io::Error::last_os_error());
            }
            let flags = if descriptor_flags > 0 && descriptor_flags & libc::FD_CLOEXEC != 0 {
                libc::O_CLOEXEC
            } else {
                0
            };
            // SAFETY: both descriptors are owned by this process; dup3
            // atomically closes the old socket and reuses its number.
            let status = unsafe { libc::dup3(replacement, fd, flags) };
            let error = io::Error::last_os_error();
            // SAFETY: the temporary descriptor is no longer needed.
            unsafe { libc::close(replacement) };
            if status < 0 { Err(error) } else { Ok(()) }
        }

        fn reopen_regular_file(fd: RawFd) -> io::Result<bool> {
            let Some(status) = stat(fd) else {
                return Ok(false);
            };
            if status.st_mode & libc::S_IFMT != libc::S_IFREG {
                return Ok(false);
            }
            // SAFETY: fcntl on a descriptor this process owns.
            let status_flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            // SAFETY: same descriptor.
            let descriptor_flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
            if status_flags < 0 || descriptor_flags < 0 || status_flags & libc::O_PATH != 0 {
                return Ok(false);
            }
            // SAFETY: lseek on an owned regular file.
            let offset = unsafe { libc::lseek(fd, 0, libc::SEEK_CUR) };
            let keep = libc::O_ACCMODE
                | libc::O_APPEND
                | libc::O_NONBLOCK
                | libc::O_SYNC
                | libc::O_DSYNC
                | libc::O_DIRECT
                | libc::O_NOATIME
                | libc::O_LARGEFILE;
            let path = std::ffi::CString::new(format!("/proc/self/fd/{fd}"))
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
            // SAFETY: the path is NUL-terminated; /proc/self/fd/N reopens the
            // same inode (even an unlinked one) with a new file description.
            let reopened =
                unsafe { libc::open(path.as_ptr(), (status_flags & keep) | libc::O_CLOEXEC) };
            if reopened < 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: lseek on the new descriptor.
            if offset >= 0 && unsafe { libc::lseek(reopened, offset, libc::SEEK_SET) } < 0 {
                let error = io::Error::last_os_error();
                // SAFETY: closing the temporary descriptor.
                unsafe { libc::close(reopened) };
                return Err(error);
            }
            let flags = if descriptor_flags & libc::FD_CLOEXEC != 0 {
                libc::O_CLOEXEC
            } else {
                0
            };
            // SAFETY: dup3 atomically swaps the description behind `fd`.
            let result = unsafe { libc::dup3(reopened, fd, flags) };
            let error = io::Error::last_os_error();
            // SAFETY: closing the temporary descriptor.
            unsafe { libc::close(reopened) };
            if result < 0 { Err(error) } else { Ok(true) }
        }
    }

    /// How much more an OOM kill should prefer a forked worker over its
    /// zygote, in `oom_score_adj` units (thousandths of the memory limit).
    const WORKER_OOM_SCORE_BONUS: i32 = 300;

    /// The zygote is the parent of every worker of its generation: when the
    /// OOM killer picks it (a post-boot zygote holds the booted application,
    /// so it is the largest idle process), every worker dies with it and the
    /// port has no listener until replacements boot. Raising the worker's own
    /// `oom_score_adj` needs no privilege and makes the kernel kill the worker
    /// that is actually growing instead; the master replaces it from the
    /// zygote in milliseconds.
    fn prefer_worker_for_oom_kill() {
        let path = "/proc/self/oom_score_adj";
        let Some(current) = std::fs::read_to_string(path)
            .ok()
            .and_then(|value| value.trim().parse::<i32>().ok())
        else {
            return;
        };
        let _ = std::fs::write(path, worker_oom_score_adj(current).to_string());
    }

    pub(super) fn worker_oom_score_adj(zygote: i32) -> i32 {
        zygote
            .saturating_add(WORKER_OOM_SCORE_BONUS)
            .clamp(-1000, 1000)
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
    pub fn ensure_single_threaded() -> Result<(), String> {
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
        /// Until the first fork, the zygote may still be booting the application.
        boot_timeout: Option<Duration>,
    }

    impl Zygote {
        /// Starts `command` (a `pam __zygote ...` invocation) with its channel.
        /// `boot_timeout` bounds the first fork, which waits for a post-boot
        /// zygote to finish booting the application.
        pub fn start(mut command: Command, boot_timeout: Duration) -> Result<Self, String> {
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
                boot_timeout: Some(boot_timeout.max(SPAWN_TIMEOUT)),
            })
        }

        /// Asks the zygote for one worker with `env` applied after the fork.
        pub fn fork_worker(&mut self, env: Vec<(String, String)>) -> Result<u32, String> {
            self.send(&ZygoteCommand::Spawn { env })?;
            let deadline = Instant::now() + self.boot_timeout.unwrap_or(SPAWN_TIMEOUT);
            loop {
                self.pump();
                if let Some(result) = self.spawned.pop_front() {
                    if result.is_ok() {
                        self.boot_timeout = None;
                    }
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
    fn forked_workers_are_preferred_over_the_zygote_by_the_oom_killer() {
        use super::linux::worker_oom_score_adj;
        assert_eq!(worker_oom_score_adj(0), 300);
        assert_eq!(worker_oom_score_adj(-500), -200);
        assert_eq!(worker_oom_score_adj(900), 1000);
    }

    #[test]
    fn parses_boot_modes() {
        use super::BootMode;
        assert_eq!(BootMode::parse("app"), Some(BootMode::Application));
        assert_eq!(BootMode::parse(" Post-Boot "), Some(BootMode::Application));
        assert_eq!(BootMode::parse("engine"), Some(BootMode::Engine));
        assert_eq!(BootMode::parse("0"), Some(BootMode::Engine));
        assert_eq!(BootMode::parse("sometimes"), None);
        assert_eq!(BootMode::Application.as_str(), "app");
    }

    #[test]
    fn disconnects_connections_and_listeners_in_place() {
        use std::io::{Read, Write};
        use std::os::fd::AsRawFd;

        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        let file = std::fs::File::open("/proc/self/status").unwrap();

        let disconnected = super::fork_hygiene::disconnect_sockets(&[
            listener.as_raw_fd(),
            client.as_raw_fd(),
            file.as_raw_fd(),
        ]);
        let descriptions = disconnected
            .iter()
            .map(|socket| socket.description.as_str())
            .collect::<Vec<_>>();
        assert_eq!(disconnected.len(), 2, "{descriptions:?}");
        assert!(
            descriptions[0].starts_with("listener 127.0.0.1:"),
            "{descriptions:?}"
        );
        assert!(
            descriptions[1].starts_with("connection to 127.0.0.1:"),
            "{descriptions:?}"
        );

        // The peer sees the connection closed, the descriptor number stays
        // valid but unconnected, and the listener no longer accepts.
        server
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut buffer = Vec::new();
        assert_eq!(server.read_to_end(&mut buffer).unwrap(), 0);
        assert!(
            super::fork_hygiene::describe_socket(client.as_raw_fd()).starts_with("unconnected")
        );
        assert!((&client).write_all(b"x").is_err());
        listener.set_nonblocking(true).unwrap();
        assert!(listener.accept().is_err());
    }

    #[test]
    fn reopened_files_get_a_private_offset() {
        use std::io::{Read, Seek, SeekFrom, Write};
        use std::os::fd::{AsRawFd, FromRawFd};

        let path = std::env::temp_dir().join(format!("pam-reopen-{}", std::process::id()));
        std::fs::File::create(&path)
            .unwrap()
            .write_all(b"0123456789")
            .unwrap();
        let mut original = std::fs::File::open(&path).unwrap();
        original.seek(SeekFrom::Start(3)).unwrap();
        // A dup shares the open file description, exactly like a fork does.
        // SAFETY: dup returns a new descriptor owned by the File below.
        let mut shared = unsafe { std::fs::File::from_raw_fd(libc::dup(original.as_raw_fd())) };
        let mut fds = [0; 2];
        // SAFETY: fds has room for the two descriptors pipe returns.
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        // SAFETY: pipe returned two new descriptors owned by these Files.
        let (pipe_read, _pipe_write) = unsafe {
            (
                std::fs::File::from_raw_fd(fds[0]),
                std::fs::File::from_raw_fd(fds[1]),
            )
        };

        let reopened =
            super::fork_hygiene::reopen_files(&[shared.as_raw_fd(), pipe_read.as_raw_fd()]);
        assert_eq!(reopened, 1, "only the regular file is reopened");
        let mut byte = [0_u8; 1];
        shared.read_exact(&mut byte).unwrap();
        assert_eq!(&byte, b"3", "the reopened file keeps its position");
        original.read_exact(&mut byte).unwrap();
        assert_eq!(&byte, b"3", "and no longer moves the original's offset");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn signal_commands_round_trip() {
        let line = serde_json::to_string(&super::linux::test_signal_command(42, 15)).unwrap();
        assert_eq!(line, r#"{"command":"signal","pid":42,"signal":15}"#);
    }
}
