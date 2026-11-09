//! Least-connections balancing between sibling workers of one prefork zygote.
//!
//! `SO_REUSEPORT` spreads new connections by hash, so with keep-alive clients a
//! few workers hold noticeably more connections than others and their queues
//! set the tail latency. Siblings share a small table of assigned connection
//! counts: an anonymous shared mapping the zygote creates before it forks.
//! When a worker accepts a connection while holding at least
//! [`HANDOFF_THRESHOLD`] more than the least loaded sibling, it passes the
//! socket to that sibling with `SCM_RIGHTS`. This costs one message per moved
//! connection and nothing per request.
//!
//! Hand-off sockets live in the master's private (0700) runtime directory, and
//! a received descriptor is only served after it is verified to be a connected
//! (not listening) TCP stream socket.
//!
//! Accounting invariant: the sender increments the receiver's `connections`
//! before the descriptor is in flight, so every message the receiver consumes
//! is matched by exactly one decrement — when the connection ends, or at once
//! when the descriptor is rejected. A draining receiver unlinks its socket so
//! late sends fail on the sender (which then serves the connection itself) and
//! keeps serving whatever was already queued instead of closing it. Counters
//! saturate at zero.

use std::io;
use std::mem::size_of;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixDatagram;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;

const SLOTS: usize = 512;
const HANDOFF_THRESHOLD: u32 = 2;

/// One slot per worker process. A slot stays owned until its process exits,
/// so connections a draining worker still serves never mix with the counts of
/// its replacement.
#[repr(C)]
struct Slot {
    pid: AtomicU32,
    accepting: AtomicU32,
    connections: AtomicU32,
}

static TABLE: AtomicPtr<Slot> = AtomicPtr::new(std::ptr::null_mut());

/// Creates the table shared by every worker the calling zygote will fork.
pub fn install_shared_table() -> io::Result<()> {
    // SAFETY: an anonymous shared mapping with no address hint; the result is
    // checked before use and stays mapped for the life of the process tree.
    let mapping = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            SLOTS * size_of::<Slot>(),
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED | libc::MAP_ANONYMOUS,
            -1,
            0,
        )
    };
    if mapping == libc::MAP_FAILED {
        return Err(io::Error::last_os_error());
    }
    TABLE.store(mapping.cast(), Ordering::Release);
    Ok(())
}

fn shared_table() -> Option<&'static [Slot]> {
    let table = TABLE.load(Ordering::Acquire);
    // SAFETY: a non-null pointer always refers to the zero-initialized mapping
    // of SLOTS slots created above (inherited by forked workers); zeroed atomics
    // are valid values.
    (!table.is_null()).then(|| unsafe { std::slice::from_raw_parts(table, SLOTS) })
}

pub struct Balancer {
    slot: usize,
    slots: &'static [Slot],
    directory: PathBuf,
    generation: u64,
    sender: UnixDatagram,
}

impl Balancer {
    /// Returns the balancer and this worker's hand-off socket when the worker
    /// was forked by a zygote. `PAM_BALANCE=0` disables balancing.
    pub fn from_environment() -> Option<(Arc<Self>, UnixDatagram)> {
        if std::env::var_os("PAM_BALANCE").is_some_and(|value| value == "0") {
            return None;
        }
        let slots = shared_table()?;
        std::env::var_os("PAM_WORKER_ID")?;
        let generation = std::env::var("PAM_WORKER_GENERATION")
            .ok()?
            .parse::<u64>()
            .ok()?;
        let directory = std::env::var_os(crate::worker_state::WORKER_STATE_PATH_ENV)
            .map(PathBuf::from)?
            .parent()?
            .to_path_buf();
        let slot = claim_slot(slots)?;
        let channel = (|| {
            let path = socket_path(&directory, generation, slot);
            let _ = std::fs::remove_file(&path);
            let receiver = UnixDatagram::bind(&path)?;
            receiver.set_nonblocking(true)?;
            let sender = UnixDatagram::unbound()?;
            sender.set_nonblocking(true)?;
            io::Result::Ok((receiver, sender))
        })();
        let (receiver, sender) = match channel {
            Ok(channel) => channel,
            Err(error) => {
                eprintln!("pam: connection balancing disabled: {error}");
                slots[slot].pid.store(0, Ordering::Release);
                return None;
            }
        };
        slots[slot].accepting.store(1, Ordering::Release);
        Some((
            Arc::new(Self {
                slot,
                slots,
                directory,
                generation,
                sender,
            }),
            receiver,
        ))
    }

    /// Decides where an accepted connection is served. Returns the stream when
    /// it stays here, or `None` after it was handed to a less loaded sibling.
    pub fn place(&self, stream: TcpStream) -> Option<(TcpStream, ConnectionGuard)> {
        let own = self.slots[self.slot].connections.load(Ordering::Relaxed);
        // A draining worker (recycle or reload) still accepts from its
        // SO_REUSEPORT queue during the quiesce window. Serving those here
        // would cut them at the graceful shutdown that follows, so they go to
        // a sibling that keeps serving.
        let draining = self.slots[self.slot].accepting.load(Ordering::Acquire) == 0;
        if let Some((sibling, pid, load)) = self.least_loaded_sibling()
            && (draining || own >= load.saturating_add(HANDOFF_THRESHOLD))
        {
            if process_alive(pid) {
                // Count the connection before it is in flight so concurrent
                // accepts in other workers do not pick the same sibling.
                self.slots[sibling]
                    .connections
                    .fetch_add(1, Ordering::Relaxed);
                if self.hand_off(sibling, &stream).is_ok() {
                    return None;
                }
                release(&self.slots[sibling]);
            } else {
                self.slots[sibling].accepting.store(0, Ordering::Release);
            }
        }
        Some((stream, self.adopt()))
    }

    /// Accounts for a connection this worker serves (accepted or received).
    pub fn adopt(&self) -> ConnectionGuard {
        self.slots[self.slot]
            .connections
            .fetch_add(1, Ordering::Relaxed);
        self.received()
    }

    /// Guard for a received connection, which the sender already counted.
    pub fn received(&self) -> ConnectionGuard {
        ConnectionGuard {
            slot: Some(&self.slots[self.slot]),
        }
    }

    /// Undoes the sender's count for a handed-off descriptor that was consumed
    /// but will not be served.
    pub fn discard_received(&self) {
        release(&self.slots[self.slot]);
    }

    /// Stops siblings from handing new connections to this draining worker.
    pub fn withdraw(&self) {
        self.slots[self.slot].accepting.store(0, Ordering::Release);
    }

    /// Withdraws and removes the hand-off socket's name, so a sibling that
    /// still picked this worker gets an error from `sendmsg` and keeps the
    /// connection instead of queueing it on a socket that is about to close.
    pub fn close_inbox(&self) {
        self.withdraw();
        let _ = std::fs::remove_file(socket_path(&self.directory, self.generation, self.slot));
    }

    fn least_loaded_sibling(&self) -> Option<(usize, u32, u32)> {
        let mut best: Option<(usize, u32, u32)> = None;
        for (index, slot) in self.slots.iter().enumerate().skip(1) {
            if index == self.slot || slot.accepting.load(Ordering::Acquire) == 0 {
                continue;
            }
            let pid = slot.pid.load(Ordering::Acquire);
            if pid == 0 {
                continue;
            }
            let load = slot.connections.load(Ordering::Relaxed);
            if best.is_none_or(|(_, _, current)| load < current) {
                best = Some((index, pid, load));
            }
        }
        best
    }

    fn hand_off(&self, sibling: usize, stream: &TcpStream) -> io::Result<()> {
        let path = socket_path(&self.directory, self.generation, sibling);
        send_descriptor(self.sender.as_raw_fd(), &path, stream.as_raw_fd())
    }
}

impl Drop for Balancer {
    fn drop(&mut self) {
        self.withdraw();
        let _ = std::fs::remove_file(socket_path(&self.directory, self.generation, self.slot));
        // Draining connections still reference this slot, so it is reclaimed
        // only after this process exits (see `claim_slot`).
    }
}

/// Claims a slot that is free or whose owner process no longer exists.
fn claim_slot(slots: &'static [Slot]) -> Option<usize> {
    let pid = std::process::id();
    for (index, slot) in slots.iter().enumerate().skip(1) {
        let owner = slot.pid.load(Ordering::Acquire);
        if owner != 0 && process_alive(owner) {
            continue;
        }
        if slot
            .pid
            .compare_exchange(owner, pid, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            slot.accepting.store(0, Ordering::Release);
            slot.connections.store(0, Ordering::Release);
            return Some(index);
        }
    }
    None
}

/// Decrements the owning worker's connection count when the connection ends.
pub struct ConnectionGuard {
    slot: Option<&'static Slot>,
}

impl ConnectionGuard {
    pub fn untracked() -> Self {
        Self { slot: None }
    }
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        if let Some(slot) = self.slot {
            release(slot);
        }
    }
}

/// Decrements a connection count without wrapping below zero, so a stray
/// decrement can never make a slot look like it holds ~4 billion connections.
fn release(slot: &Slot) {
    let _ = slot
        .connections
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
            count.checked_sub(1)
        });
}

/// A TCP stream that releases its balancing slot when dropped.
pub struct TrackedStream {
    inner: TcpStream,
    _guard: ConnectionGuard,
}

impl TrackedStream {
    pub fn new(inner: TcpStream, guard: ConnectionGuard) -> Self {
        Self {
            inner,
            _guard: guard,
        }
    }
}

impl AsyncRead for TrackedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(context, buffer)
    }
}

impl AsyncWrite for TrackedStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(context, buffer)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffers: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write_vectored(context, buffers)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(context)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(context)
    }
}

/// Receives one handed-off TCP socket, or `WouldBlock` when none is queued.
/// An `InvalidData` error means a message was consumed but rejected, so its
/// sender's count must be released (see [`Balancer::discard_received`]).
pub fn receive_descriptor(socket: RawFd) -> io::Result<std::net::TcpStream> {
    let mut byte = [0_u8; 1];
    let mut iov = libc::iovec {
        iov_base: byte.as_mut_ptr().cast(),
        iov_len: byte.len(),
    };
    // Room for a single descriptor, aligned for cmsghdr.
    let mut control = [0_u64; 8];
    // SAFETY: zeroed msghdr is a valid empty header; fields are set below.
    let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
    message.msg_iov = &mut iov;
    message.msg_iovlen = 1;
    message.msg_control = control.as_mut_ptr().cast();
    message.msg_controllen = size_of::<[u64; 8]>() as _;
    // SAFETY: the header points at buffers that outlive the call.
    let received = unsafe {
        libc::recvmsg(
            socket,
            &mut message,
            libc::MSG_DONTWAIT | libc::MSG_CMSG_CLOEXEC,
        )
    };
    if received < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut descriptor = None;
    // SAFETY: CMSG_* walk the control buffer filled in by recvmsg.
    unsafe {
        let mut header = libc::CMSG_FIRSTHDR(&message);
        while !header.is_null() {
            if (*header).cmsg_level == libc::SOL_SOCKET && (*header).cmsg_type == libc::SCM_RIGHTS {
                let data = libc::CMSG_DATA(header).cast::<RawFd>();
                let count =
                    ((*header).cmsg_len as usize - libc::CMSG_LEN(0) as usize) / size_of::<RawFd>();
                for index in 0..count {
                    let fd = OwnedFd::from_raw_fd(std::ptr::read_unaligned(data.add(index)));
                    if descriptor.is_none() {
                        descriptor = Some(fd);
                    }
                }
            }
            header = libc::CMSG_NXTHDR(&message, header);
        }
    }
    let descriptor = descriptor
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "message without a socket"))?;
    if !is_tcp_stream(descriptor.as_raw_fd()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "handed-off descriptor is not a connected TCP stream",
        ));
    }
    Ok(std::net::TcpStream::from(descriptor))
}

fn send_descriptor(socket: RawFd, path: &Path, descriptor: RawFd) -> io::Result<()> {
    let (address, address_length) = unix_address(path)?;
    let mut byte = [1_u8; 1];
    let mut iov = libc::iovec {
        iov_base: byte.as_mut_ptr().cast(),
        iov_len: byte.len(),
    };
    let mut control = [0_u64; 8];
    // SAFETY: zeroed msghdr is a valid empty header; fields are set below.
    let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
    message.msg_name = (&address as *const libc::sockaddr_un).cast_mut().cast();
    message.msg_namelen = address_length;
    message.msg_iov = &mut iov;
    message.msg_iovlen = 1;
    message.msg_control = control.as_mut_ptr().cast();
    // SAFETY: CMSG_* fill one SCM_RIGHTS header inside the control buffer,
    // which is large enough for one descriptor.
    unsafe {
        message.msg_controllen = libc::CMSG_SPACE(size_of::<RawFd>() as u32) as _;
        let header = libc::CMSG_FIRSTHDR(&message);
        (*header).cmsg_level = libc::SOL_SOCKET;
        (*header).cmsg_type = libc::SCM_RIGHTS;
        (*header).cmsg_len = libc::CMSG_LEN(size_of::<RawFd>() as u32) as _;
        std::ptr::write_unaligned(libc::CMSG_DATA(header).cast::<RawFd>(), descriptor);
    }
    // SAFETY: every pointer in the header refers to live local buffers.
    let sent = unsafe { libc::sendmsg(socket, &message, libc::MSG_DONTWAIT | libc::MSG_NOSIGNAL) };
    if sent < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn unix_address(path: &Path) -> io::Result<(libc::sockaddr_un, libc::socklen_t)> {
    use std::os::unix::ffi::OsStrExt;
    // SAFETY: zeroed sockaddr_un is valid; the path is copied below.
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    let bytes = path.as_os_str().as_bytes();
    if bytes.len() >= address.sun_path.len() {
        return Err(io::Error::other("hand-off socket path is too long"));
    }
    for (target, source) in address.sun_path.iter_mut().zip(bytes) {
        *target = *source as libc::c_char;
    }
    let length = size_of::<libc::sa_family_t>() + bytes.len() + 1;
    Ok((address, length as libc::socklen_t))
}

fn is_tcp_stream(descriptor: RawFd) -> bool {
    let option = |name| {
        let mut value: libc::c_int = 0;
        let mut length = size_of::<libc::c_int>() as libc::socklen_t;
        // SAFETY: value and length point to valid storage of the right size.
        let status = unsafe {
            libc::getsockopt(
                descriptor,
                libc::SOL_SOCKET,
                name,
                (&mut value as *mut libc::c_int).cast(),
                &mut length,
            )
        };
        (status == 0).then_some(value)
    };
    option(libc::SO_TYPE) == Some(libc::SOCK_STREAM)
        && matches!(
            option(libc::SO_DOMAIN),
            Some(libc::AF_INET | libc::AF_INET6)
        )
        && option(libc::SO_PROTOCOL) == Some(libc::IPPROTO_TCP)
        // A listening socket would let a sibling inject an accept loop.
        && option(libc::SO_ACCEPTCONN) == Some(0)
}

fn process_alive(pid: u32) -> bool {
    // SAFETY: signal 0 only checks that the process exists.
    i32::try_from(pid).is_ok_and(|pid| unsafe { libc::kill(pid, 0) } == 0)
}

fn socket_path(directory: &Path, generation: u64, slot: usize) -> PathBuf {
    directory.join(format!("balance-{generation}-{slot}.sock"))
}

pub use crate::http_serve::{ConnectionTracker as HandOffTracker, HttpSettings as HandOffHttp};

/// Receives sockets handed over by siblings and serves them with `app`. When
/// the worker starts draining, the inbox is withdrawn so siblings stop
/// choosing this worker; whatever was already queued is still served.
pub fn spawn_receiver(
    socket: UnixDatagram,
    balancer: Arc<Balancer>,
    app: axum::Router,
    drain: Arc<crate::http_serve::Drain>,
    http: HandOffHttp,
    tracker: HandOffTracker,
) {
    // The inbox counts as active work until its final drain, so shutdown's
    // `wait_idle` cannot finish while queued descriptors are still unread.
    let inbox = tracker.begin();
    tokio::spawn(async move {
        let _inbox = inbox;
        let Ok(socket) = tokio::net::UnixDatagram::from_std(socket) else {
            return;
        };
        let mut draining = drain.subscribe();
        loop {
            let stopping = tokio::select! {
                ready = socket.readable() => {
                    if ready.is_err() {
                        return;
                    }
                    false
                }
                _ = crate::http_serve::raised(&mut draining) => true,
            };
            if stopping {
                // Siblings stop choosing this worker and late sends fail on
                // the sender, which then serves the connection itself.
                balancer.close_inbox();
            }
            drain_inbox(&socket, &balancer, &app, &drain, http, &tracker);
            if stopping {
                // A sibling may have resolved the socket name just before it was
                // removed; give such a send time to land, then drain once more
                // so nothing queued is closed (and reset) with the socket.
                tokio::time::sleep(LATE_SEND_GRACE).await;
                drain_inbox(&socket, &balancer, &app, &drain, http, &tracker);
                return;
            }
        }
    });
}

/// How long a draining worker keeps its inbox open after unlinking it.
const LATE_SEND_GRACE: std::time::Duration = std::time::Duration::from_millis(50);

fn drain_inbox(
    socket: &tokio::net::UnixDatagram,
    balancer: &Arc<Balancer>,
    app: &axum::Router,
    drain: &Arc<crate::http_serve::Drain>,
    http: HandOffHttp,
    tracker: &HandOffTracker,
) {
    loop {
        match socket.try_io(tokio::io::Interest::READABLE, || {
            receive_descriptor(socket.as_raw_fd())
        }) {
            Ok(stream) => serve_handed_off(
                stream,
                balancer.received(),
                app.clone(),
                drain.clone(),
                http,
                tracker.begin(),
            ),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                balancer.discard_received();
                eprintln!("pam: rejected handed-off connection: {error}");
            }
            Err(error) => {
                eprintln!("pam: cannot receive handed-off connection: {error}");
                break;
            }
        }
    }
}

fn serve_handed_off(
    stream: std::net::TcpStream,
    guard: ConnectionGuard,
    app: axum::Router,
    drain: Arc<crate::http_serve::Drain>,
    http: HandOffHttp,
    active: crate::http_serve::ActiveConnection,
) {
    tokio::spawn(async move {
        let _active = active;
        if stream.set_nonblocking(true).is_err() {
            return;
        }
        let Ok(stream) = TcpStream::from_std(stream) else {
            return;
        };
        let Ok(remote) = stream.peer_addr() else {
            return;
        };
        crate::http_serve::serve_connection(
            TrackedStream::new(stream, guard),
            remote,
            app,
            drain,
            http,
        )
        .await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(connections: u32) -> Slot {
        Slot {
            pid: AtomicU32::new(1),
            accepting: AtomicU32::new(1),
            connections: AtomicU32::new(connections),
        }
    }

    #[test]
    fn release_saturates_at_zero() {
        let slot = slot(1);
        release(&slot);
        release(&slot);
        assert_eq!(slot.connections.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn accepts_only_connected_tcp_streams() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        assert!(!is_tcp_stream(listener.as_raw_fd()), "listening socket");
        let client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        assert!(is_tcp_stream(client.as_raw_fd()));
        assert!(is_tcp_stream(server.as_raw_fd()));
        let udp = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        assert!(!is_tcp_stream(udp.as_raw_fd()));
        let (unix, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
        assert!(!is_tcp_stream(unix.as_raw_fd()));
    }

    fn scratch_directory(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("pam-balance-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir(&directory).unwrap();
        directory
    }

    #[test]
    fn rejected_descriptors_are_reported_as_consumed() {
        let directory = scratch_directory("reject");
        let path = directory.join("inbox.sock");
        let receiver = UnixDatagram::bind(&path).unwrap();
        let sender = UnixDatagram::unbound().unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        send_descriptor(sender.as_raw_fd(), &path, listener.as_raw_fd()).unwrap();
        let error = receive_descriptor(receiver.as_raw_fd()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        let error = receive_descriptor(receiver.as_raw_fd()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_closed_inbox_makes_late_sends_fail_on_the_sender() {
        let directory = scratch_directory("closed");
        let slots: &'static [Slot] = Box::leak(vec![slot(0), slot(0), slot(0)].into_boxed_slice());
        let path = socket_path(&directory, 7, 2);
        let _receiver = UnixDatagram::bind(&path).unwrap();
        let balancer = Balancer {
            slot: 2,
            slots,
            directory: directory.clone(),
            generation: 7,
            sender: UnixDatagram::unbound().unwrap(),
        };
        balancer.close_inbox();
        assert_eq!(slots[2].accepting.load(Ordering::Relaxed), 0);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        assert!(send_descriptor(balancer.sender.as_raw_fd(), &path, client.as_raw_fd()).is_err());

        slots[2].connections.store(1, Ordering::Relaxed);
        balancer.discard_received();
        balancer.discard_received();
        assert_eq!(slots[2].connections.load(Ordering::Relaxed), 0);
        drop(balancer);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[tokio::test]
    async fn a_draining_receiver_serves_queued_connections_and_releases_counts() {
        use std::io::{Read, Write};
        let directory = scratch_directory("drain");
        let slots: &'static [Slot] = Box::leak(vec![slot(0), slot(0), slot(0)].into_boxed_slice());
        let path = socket_path(&directory, 3, 1);
        let inbox = UnixDatagram::bind(&path).unwrap();
        inbox.set_nonblocking(true).unwrap();
        let balancer = Arc::new(Balancer {
            slot: 1,
            slots,
            directory: directory.clone(),
            generation: 3,
            sender: UnixDatagram::unbound().unwrap(),
        });

        // A sibling counted and queued a connection, then the receiver drains.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (accepted, _) = listener.accept().unwrap();
        slots[1].connections.fetch_add(1, Ordering::Relaxed);
        send_descriptor(balancer.sender.as_raw_fd(), &path, accepted.as_raw_fd()).unwrap();
        drop(accepted);

        let drain = crate::http_serve::Drain::new();
        drain.start();
        let tracker = HandOffTracker::default();
        let app = axum::Router::new()
            .route("/", axum::routing::get(|| async { "served" }))
            .layer(axum::middleware::from_fn_with_state(
                drain.clone(),
                crate::http_serve::close_when_draining,
            ));
        spawn_receiver(
            inbox,
            balancer.clone(),
            app,
            drain,
            HandOffHttp {
                header_read_timeout: std::time::Duration::from_secs(5),
                graceful_timeout: std::time::Duration::from_secs(5),
            },
            tracker.clone(),
        );
        let response = tokio::task::spawn_blocking(move || {
            client
                .write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
                .unwrap();
            let mut response = String::new();
            client.read_to_string(&mut response).unwrap();
            response
        })
        .await
        .unwrap();
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(response.ends_with("served"), "{response}");
        tracker.wait_idle(std::time::Duration::from_secs(5)).await;
        assert_eq!(slots[1].connections.load(Ordering::Relaxed), 0);
        assert!(!path.exists(), "the inbox name is removed while draining");
        assert!(
            response.to_ascii_lowercase().contains("connection: close"),
            "{response}"
        );
        drop(balancer);
        std::fs::remove_dir_all(&directory).unwrap();
    }
}
