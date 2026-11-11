//! Listening sockets owned by the cluster master (Linux).
//!
//! Every HTTP worker binds its own `SO_REUSEPORT` listener. When a worker is
//! recycled or reloaded, closing that listener resets the connections the
//! kernel had already queued on it, or completed the handshake on in the same
//! instant. `net.ipv4.tcp_migrate_req=1` moves most of them to a sibling, but
//! not when the sibling it picks is closing too, which is exactly what happens
//! when balanced workers reach `--max-requests` together or a generation is
//! replaced.
//!
//! Instead, the master keeps a duplicate of each worker's listener, keyed by
//! address and worker id. The next incarnation of that worker (a
//! `--max-requests` replacement, a crash restart or the same worker id of the
//! next generation) adopts the existing socket instead of binding a new one, so
//! a listener with connections queued on it is never closed while the cluster
//! runs: the old incarnation stops accepting and the new one accepts from the
//! same queue. The master releases a listener only when no generation uses its
//! address any more, when the worker id is not restarted at once (crash with
//! backoff), and at shutdown.
//!
//! Protocol: one `SOCK_SEQPACKET` connection per request to a socket in the
//! master's private (0700) runtime directory. A request is
//! `claim|register \0 generation \0 worker id \0 address`; `register` carries the
//! listener in `SCM_RIGHTS`. The reply is one byte (`1` found/stored, `0` not
//! found) and, for a found `claim`, the listener.

use std::collections::HashMap;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const REGISTRY_ENV: &str = "PAM_LISTENER_REGISTRY";
const MAX_MESSAGE: usize = 1024;

#[derive(Default)]
struct Held {
    listener: Option<OwnedFd>,
    created: u64,
    claimed: u64,
}

#[derive(Default)]
struct Table {
    listeners: HashMap<(usize, String), Held>,
}

/// The master side: owns the held listeners and serves workers' requests.
#[derive(Clone)]
pub struct Registry {
    table: Arc<Mutex<Table>>,
    path: PathBuf,
}

impl Registry {
    /// Binds the registry socket in `directory` and serves it on a thread.
    pub fn start(directory: &Path) -> io::Result<Self> {
        let path = directory.join("listeners.sock");
        let _ = std::fs::remove_file(&path);
        let socket = seqpacket()?;
        let (address, length) = unix_address(&path)?;
        // SAFETY: valid socket and address.
        if unsafe {
            libc::bind(
                socket.as_raw_fd(),
                (&address as *const libc::sockaddr_un).cast(),
                length,
            )
        } != 0
            // SAFETY: valid socket.
            || unsafe { libc::listen(socket.as_raw_fd(), 128) } != 0
        {
            return Err(io::Error::last_os_error());
        }
        let registry = Self {
            table: Arc::new(Mutex::new(Table::default())),
            path,
        };
        let served = registry.clone();
        std::thread::Builder::new()
            .name("pam-listeners".to_owned())
            .spawn(move || served.serve(socket))?;
        Ok(registry)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn serve(&self, socket: OwnedFd) {
        loop {
            // SAFETY: accept on a listening socket this thread owns.
            let connection = unsafe {
                libc::accept4(
                    socket.as_raw_fd(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    libc::SOCK_CLOEXEC,
                )
            };
            if connection < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                eprintln!("pam: listener registry stopped: {error}");
                return;
            }
            // SAFETY: accept4 returned a new descriptor owned here.
            let connection = unsafe { OwnedFd::from_raw_fd(connection) };
            if let Err(error) = self.answer(&connection) {
                eprintln!("pam: listener registry request failed: {error}");
            }
        }
    }

    fn answer(&self, connection: &OwnedFd) -> io::Result<()> {
        set_timeout(connection.as_raw_fd(), 2)?;
        let (message, descriptor) = receive(connection.as_raw_fd())?;
        let request = Request::parse(&message)?;
        let mut table = self
            .table
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let key = (request.worker, request.address);
        match request.kind {
            Kind::Claim => {
                let found = table
                    .listeners
                    .get_mut(&key)
                    .and_then(|held| {
                        held.claimed = held.claimed.max(request.generation);
                        held.listener.as_ref()
                    })
                    .map(|listener| listener.as_raw_fd());
                match found {
                    Some(listener) => send(connection.as_raw_fd(), b"1", Some(listener)),
                    None => send(connection.as_raw_fd(), b"0", None),
                }
            }
            Kind::Register => {
                let listener = descriptor.ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "register without a listener")
                })?;
                if !is_tcp_listener(listener.as_raw_fd()) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "registered descriptor is not a listening TCP socket",
                    ));
                }
                table.listeners.insert(
                    key,
                    Held {
                        listener: Some(listener),
                        created: request.generation,
                        claimed: request.generation,
                    },
                );
                send(connection.as_raw_fd(), b"1", None)
            }
        }
    }

    /// Generation `generation` serves now: listeners no worker of it claimed
    /// (an address the new code no longer listens on) are closed.
    pub fn activate(&self, generation: u64) {
        let mut table = self
            .table
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        table.listeners.retain(|_, held| held.claimed >= generation);
    }

    /// Generation `generation` failed to start: listeners only it created are
    /// closed so a port it bound does not keep accepting connections.
    pub fn discard(&self, generation: u64) {
        let mut table = self
            .table
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        table.listeners.retain(|_, held| held.created != generation);
    }

    /// Worker `worker` is not restarted right away: its listeners close, so
    /// the kernel stops queuing connections nobody would accept for a while.
    pub fn release(&self, worker: usize) {
        let mut table = self
            .table
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        table.listeners.retain(|(id, _), _| *id != worker);
    }

    /// Closes every held listener (shutdown, after the workers stopped).
    pub fn clear(&self) {
        let mut table = self
            .table
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        table.listeners.clear();
    }

    #[cfg(test)]
    fn held(&self) -> usize {
        self.table
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .listeners
            .len()
    }
}

impl Drop for Registry {
    fn drop(&mut self) {
        if Arc::strong_count(&self.table) <= 2 {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

enum Kind {
    Claim,
    Register,
}

struct Request {
    kind: Kind,
    generation: u64,
    worker: usize,
    address: String,
}

impl Request {
    fn encode(kind: &str, generation: u64, worker: usize, address: &str) -> Vec<u8> {
        format!("{kind}\0{generation}\0{worker}\0{address}").into_bytes()
    }

    fn parse(message: &[u8]) -> io::Result<Self> {
        let invalid = || io::Error::new(io::ErrorKind::InvalidData, "invalid registry request");
        let text = std::str::from_utf8(message).map_err(|_| invalid())?;
        let mut fields = text.split('\0');
        let kind = match fields.next() {
            Some("claim") => Kind::Claim,
            Some("register") => Kind::Register,
            _ => return Err(invalid()),
        };
        let generation = fields
            .next()
            .and_then(|value| value.parse().ok())
            .ok_or_else(invalid)?;
        let worker = fields
            .next()
            .and_then(|value| value.parse().ok())
            .ok_or_else(invalid)?;
        let address = fields
            .next()
            .filter(|value| !value.is_empty())
            .ok_or_else(invalid)?;
        if fields.next().is_some() {
            return Err(invalid());
        }
        Ok(Self {
            kind,
            generation,
            worker,
            address: address.to_owned(),
        })
    }
}

/// The worker side.
pub struct Client {
    path: PathBuf,
    generation: u64,
    worker: usize,
}

impl Client {
    pub fn from_environment() -> Option<Self> {
        Some(Self {
            path: PathBuf::from(std::env::var_os(REGISTRY_ENV)?),
            generation: std::env::var("PAM_WORKER_GENERATION").ok()?.parse().ok()?,
            worker: std::env::var("PAM_WORKER_ID").ok()?.parse().ok()?,
        })
    }

    /// The listener a previous incarnation of this worker left for `address`.
    pub fn claim(&self, address: &str) -> io::Result<Option<std::net::TcpListener>> {
        let (reply, listener) = self.request(
            &Request::encode("claim", self.generation, self.worker, address),
            None,
        )?;
        match (reply.as_slice(), listener) {
            (b"1", Some(listener)) if is_tcp_listener(listener.as_raw_fd()) => {
                Ok(Some(std::net::TcpListener::from(listener)))
            }
            (b"0", None) => Ok(None),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid listener registry reply",
            )),
        }
    }

    /// Hands a newly bound listener to the master for the next incarnations.
    pub fn register(&self, address: &str, listener: &std::net::TcpListener) -> io::Result<()> {
        let (reply, _) = self.request(
            &Request::encode("register", self.generation, self.worker, address),
            Some(listener.as_raw_fd()),
        )?;
        if reply == b"1" {
            Ok(())
        } else {
            Err(io::Error::other("the master refused the listener"))
        }
    }

    fn request(
        &self,
        message: &[u8],
        descriptor: Option<RawFd>,
    ) -> io::Result<(Vec<u8>, Option<OwnedFd>)> {
        let socket = seqpacket()?;
        set_timeout(socket.as_raw_fd(), 5)?;
        let (address, length) = unix_address(&self.path)?;
        // SAFETY: valid socket and address.
        if unsafe {
            libc::connect(
                socket.as_raw_fd(),
                (&address as *const libc::sockaddr_un).cast(),
                length,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        send(socket.as_raw_fd(), message, descriptor)?;
        receive(socket.as_raw_fd())
    }
}

fn seqpacket() -> io::Result<OwnedFd> {
    // SAFETY: plain socket creation.
    let socket =
        unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC, 0) };
    if socket < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: socket returned a new descriptor.
    Ok(unsafe { OwnedFd::from_raw_fd(socket) })
}

fn set_timeout(socket: RawFd, seconds: libc::time_t) -> io::Result<()> {
    let timeout = libc::timeval {
        tv_sec: seconds,
        tv_usec: 0,
    };
    for option in [libc::SO_RCVTIMEO, libc::SO_SNDTIMEO] {
        // SAFETY: the timeval outlives the call and its size is passed.
        if unsafe {
            libc::setsockopt(
                socket,
                libc::SOL_SOCKET,
                option,
                (&timeout as *const libc::timeval).cast(),
                std::mem::size_of::<libc::timeval>() as libc::socklen_t,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
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
        return Err(io::Error::other("listener registry path is too long"));
    }
    for (target, source) in address.sun_path.iter_mut().zip(bytes) {
        *target = *source as libc::c_char;
    }
    let length = std::mem::size_of::<libc::sa_family_t>() + bytes.len() + 1;
    Ok((address, length as libc::socklen_t))
}

fn send(socket: RawFd, message: &[u8], descriptor: Option<RawFd>) -> io::Result<()> {
    let mut iov = libc::iovec {
        iov_base: message.as_ptr().cast_mut().cast(),
        iov_len: message.len(),
    };
    let mut control = [0_u64; 8];
    // SAFETY: zeroed msghdr is a valid empty header; fields are set below.
    let mut header: libc::msghdr = unsafe { std::mem::zeroed() };
    header.msg_iov = &mut iov;
    header.msg_iovlen = 1;
    if let Some(descriptor) = descriptor {
        header.msg_control = control.as_mut_ptr().cast();
        // SAFETY: CMSG_* fill one SCM_RIGHTS header inside the control buffer,
        // which is large enough for one descriptor.
        unsafe {
            header.msg_controllen = libc::CMSG_SPACE(std::mem::size_of::<RawFd>() as u32) as _;
            let cmsg = libc::CMSG_FIRSTHDR(&header);
            (*cmsg).cmsg_level = libc::SOL_SOCKET;
            (*cmsg).cmsg_type = libc::SCM_RIGHTS;
            (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<RawFd>() as u32) as _;
            std::ptr::write_unaligned(libc::CMSG_DATA(cmsg).cast::<RawFd>(), descriptor);
        }
    }
    // SAFETY: every pointer in the header refers to live local buffers.
    if unsafe { libc::sendmsg(socket, &header, libc::MSG_NOSIGNAL) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn receive(socket: RawFd) -> io::Result<(Vec<u8>, Option<OwnedFd>)> {
    let mut buffer = vec![0_u8; MAX_MESSAGE];
    let mut iov = libc::iovec {
        iov_base: buffer.as_mut_ptr().cast(),
        iov_len: buffer.len(),
    };
    let mut control = [0_u64; 8];
    // SAFETY: zeroed msghdr is a valid empty header; fields are set below.
    let mut header: libc::msghdr = unsafe { std::mem::zeroed() };
    header.msg_iov = &mut iov;
    header.msg_iovlen = 1;
    header.msg_control = control.as_mut_ptr().cast();
    header.msg_controllen = std::mem::size_of::<[u64; 8]>() as _;
    // SAFETY: the header points at buffers that outlive the call.
    let received = unsafe { libc::recvmsg(socket, &mut header, libc::MSG_CMSG_CLOEXEC) };
    if received < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut descriptor = None;
    // SAFETY: CMSG_* walk the control buffer filled in by recvmsg; every
    // received descriptor is taken into an OwnedFd so none leaks.
    unsafe {
        let mut cmsg = libc::CMSG_FIRSTHDR(&header);
        while !cmsg.is_null() {
            if (*cmsg).cmsg_level == libc::SOL_SOCKET && (*cmsg).cmsg_type == libc::SCM_RIGHTS {
                let data = libc::CMSG_DATA(cmsg).cast::<RawFd>();
                let count = ((*cmsg).cmsg_len as usize - libc::CMSG_LEN(0) as usize)
                    / std::mem::size_of::<RawFd>();
                for index in 0..count {
                    let fd = OwnedFd::from_raw_fd(std::ptr::read_unaligned(data.add(index)));
                    if descriptor.is_none() {
                        descriptor = Some(fd);
                    }
                }
            }
            cmsg = libc::CMSG_NXTHDR(&header, cmsg);
        }
    }
    if header.msg_flags & (libc::MSG_TRUNC | libc::MSG_CTRUNC) != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "truncated registry message",
        ));
    }
    buffer.truncate(received as usize);
    Ok((buffer, descriptor))
}

fn is_tcp_listener(descriptor: RawFd) -> bool {
    let option = |name| {
        let mut value: libc::c_int = 0;
        let mut length = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
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
        && option(libc::SO_PROTOCOL) == Some(libc::IPPROTO_TCP)
        && option(libc::SO_ACCEPTCONN) == Some(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    fn directory(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("pam-lr-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn client(registry: &Registry, generation: u64, worker: usize) -> Client {
        Client {
            path: registry.path().to_owned(),
            generation,
            worker,
        }
    }

    #[test]
    fn a_next_incarnation_adopts_the_listener_and_its_queue() {
        let directory = directory("adopt");
        let registry = Registry::start(&directory).unwrap();
        let first = client(&registry, 1, 1);
        assert!(first.claim("127.0.0.1:0").unwrap().is_none());
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        first.register("127.0.0.1:0", &listener).unwrap();
        let address = listener.local_addr().unwrap();

        // A connection queued while the first incarnation exits is served by
        // the next one: the master's copy kept the socket open.
        let mut queued = std::net::TcpStream::connect(address).unwrap();
        drop(listener);
        let next = client(&registry, 1, 1)
            .claim("127.0.0.1:0")
            .unwrap()
            .unwrap();
        assert_eq!(next.local_addr().unwrap(), address);
        let (mut accepted, _) = next.accept().unwrap();
        queued.write_all(b"ping").unwrap();
        let mut buffer = [0_u8; 4];
        accepted.read_exact(&mut buffer).unwrap();
        assert_eq!(&buffer, b"ping");

        // Another worker id or address never gets this socket.
        assert!(
            client(&registry, 1, 2)
                .claim("127.0.0.1:0")
                .unwrap()
                .is_none()
        );
        assert!(
            client(&registry, 1, 1)
                .claim("127.0.0.1:1")
                .unwrap()
                .is_none()
        );
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn listeners_are_released_by_generation_and_worker() {
        let directory = directory("release");
        let registry = Registry::start(&directory).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        client(&registry, 1, 1).register("a:1", &listener).unwrap();
        client(&registry, 1, 2).register("a:1", &listener).unwrap();
        assert_eq!(registry.held(), 2);

        // Generation 2 claims worker 1 only; worker 2's address is gone.
        assert!(client(&registry, 2, 1).claim("a:1").unwrap().is_some());
        registry.activate(2);
        assert_eq!(registry.held(), 1);

        // A failed generation's new listeners are closed; adopted ones stay.
        client(&registry, 3, 3).register("b:1", &listener).unwrap();
        assert!(client(&registry, 3, 1).claim("a:1").unwrap().is_some());
        registry.discard(3);
        assert_eq!(registry.held(), 1);

        registry.release(1);
        assert_eq!(registry.held(), 0);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn rejects_malformed_requests_and_non_listeners() {
        assert!(Request::parse(b"claim\x001\x001\x00a:1").is_ok());
        assert!(Request::parse(b"claim\x001\x001\x00").is_err());
        assert!(Request::parse(b"steal\x001\x001\x00a:1").is_err());
        assert!(Request::parse(b"claim\x00x\x001\x00a:1").is_err());
        assert!(Request::parse(b"claim\x001\x001\x00a:1\x00extra").is_err());

        let directory = directory("reject");
        let registry = Registry::start(&directory).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let stream = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let request = client(&registry, 1, 1);
        // The master answers nothing (the connection just closes) and keeps
        // nothing.
        let reply = request.request(
            &Request::encode("register", 1, 1, "a:1"),
            Some(stream.as_raw_fd()),
        );
        assert!(reply.is_err() || reply.unwrap().0.is_empty());
        assert_eq!(registry.held(), 0);
        let _ = std::fs::remove_dir_all(directory);
    }
}
