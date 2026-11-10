//! Shared-memory segments owned by the PAM master (Linux).
//!
//! Workers of a cluster are separate processes started from different
//! zygotes, so they cannot inherit an anonymous mapping. The master listens on
//! a Unix socket inside its private runtime directory and hands out sealed-size
//! `memfd` segments by name over `SCM_RIGHTS`. Segments are created on first
//! request (nothing is allocated when no table is used), keep their contents
//! across worker recycling and reloads, and are released by the kernel when
//! the master and its workers exit, even after `SIGKILL`.

use std::collections::HashMap;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt};

const REQUEST_LIMIT: usize = 512;

pub struct SegmentServer {
    task: tokio::task::JoinHandle<()>,
}

impl SegmentServer {
    /// Starts serving segments on `socket` (created with mode 0600 inside a
    /// private directory). `budget` caps the total bytes of all segments.
    pub fn start(socket: &Path, budget: u64) -> Result<Self, String> {
        let listener = tokio::net::UnixListener::bind(socket).map_err(|error| {
            format!(
                "cannot listen for shared memory on {}: {error}",
                socket.display()
            )
        })?;
        let segments: Arc<Mutex<Segments>> = Arc::new(Mutex::new(Segments {
            by_key: HashMap::new(),
            bytes: 0,
            budget,
        }));
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    continue;
                };
                let segments = segments.clone();
                tokio::spawn(async move {
                    let _ = tokio::time::timeout(Duration::from_secs(10), serve(stream, segments))
                        .await;
                });
            }
        });
        Ok(Self { task })
    }
}

impl Drop for SegmentServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct Segments {
    by_key: HashMap<String, (OwnedFd, usize)>,
    bytes: u64,
    budget: u64,
}

async fn serve(mut stream: tokio::net::UnixStream, segments: Arc<Mutex<Segments>>) {
    if !same_user(&stream) {
        return;
    }
    let mut line = Vec::new();
    let read = tokio::io::BufReader::new((&mut stream).take(REQUEST_LIMIT as u64))
        .read_until(b'\n', &mut line)
        .await;
    if line.last() == Some(&b'\n') {
        line.pop();
    }
    let reply = match read.ok().and_then(|_| parse(&line)) {
        Some((length, key)) => segment(&segments, &key, length),
        None => Err("malformed request".to_owned()),
    };
    let _ = stream.writable().await;
    let _ = match reply {
        Ok(fd) => send_fd(stream.as_raw_fd(), b"OK\n", Some(fd)),
        Err(error) => send_fd(stream.as_raw_fd(), format!("ER {error}\n").as_bytes(), None),
    };
}

fn parse(line: &[u8]) -> Option<(usize, String)> {
    let text = std::str::from_utf8(line).ok()?;
    let mut parts = text.splitn(3, ' ');
    if parts.next()? != "PAMSHM1" {
        return None;
    }
    let length: usize = parts.next()?.parse().ok()?;
    let key = parts.next()?;
    if length == 0
        || length % 4096 != 0
        || length as u64 > crate::shared_table::MAX_SEGMENT_BYTES
        || crate::shared_table::validate_name(key).is_err()
    {
        return None;
    }
    Some((length, key.to_owned()))
}

fn segment(segments: &Mutex<Segments>, key: &str, length: usize) -> Result<RawFd, String> {
    let mut segments = segments
        .lock()
        .map_err(|_| "segment registry poisoned".to_owned())?;
    if let Some((fd, existing)) = segments.by_key.get(key) {
        if *existing != length {
            return Err(format!("segment {key} already exists with another size"));
        }
        return Ok(fd.as_raw_fd());
    }
    if segments.bytes.saturating_add(length as u64) > segments.budget {
        return Err(format!(
            "shared tables would exceed the {} byte budget (PAM_SHARED_MEMORY_MAX_BYTES)",
            segments.budget
        ));
    }
    let name =
        std::ffi::CString::new(format!("pam-table:{key}")).map_err(|error| error.to_string())?;
    // SAFETY: plain syscalls on a fresh descriptor.
    let fd = unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        return Err(format!(
            "memfd_create failed: {}",
            io::Error::last_os_error()
        ));
    }
    // SAFETY: fd was just created and is owned here.
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    if unsafe { libc::ftruncate(fd.as_raw_fd(), length as libc::off_t) } != 0 {
        return Err(format!(
            "cannot size shared memory: {}",
            io::Error::last_os_error()
        ));
    }
    let raw = fd.as_raw_fd();
    segments.bytes += length as u64;
    segments.by_key.insert(key.to_owned(), (fd, length));
    Ok(raw)
}

fn same_user(stream: &tokio::net::UnixStream) -> bool {
    stream
        .peer_cred()
        // SAFETY: geteuid has no preconditions.
        .is_ok_and(|credentials| credentials.uid() == unsafe { libc::geteuid() })
}

/// Default budget: half of physical memory (segments are only committed as
/// rows are written).
pub fn default_budget() -> u64 {
    if let Some(value) = std::env::var("PAM_SHARED_MEMORY_MAX_BYTES")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
    {
        return value;
    }
    // SAFETY: sysconf has no preconditions.
    let pages = unsafe { libc::sysconf(libc::_SC_PHYS_PAGES) };
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if pages > 0 && page > 0 {
        (pages as u64).saturating_mul(page as u64) / 2
    } else {
        4 << 30
    }
}

fn send_fd(socket: RawFd, data: &[u8], fd: Option<RawFd>) -> io::Result<()> {
    let mut iov = libc::iovec {
        iov_base: data.as_ptr() as *mut libc::c_void,
        iov_len: data.len(),
    };
    // SAFETY: CMSG_SPACE is a pure size computation.
    let space = unsafe { libc::CMSG_SPACE(std::mem::size_of::<RawFd>() as u32) } as usize;
    let mut control = vec![0_u8; space];
    // SAFETY: msghdr is plain data; zero is a valid initial state.
    let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
    message.msg_iov = &mut iov;
    message.msg_iovlen = 1;
    if let Some(fd) = fd {
        message.msg_control = control.as_mut_ptr().cast();
        message.msg_controllen = space as _;
        // SAFETY: control holds exactly one SCM_RIGHTS header plus one fd.
        unsafe {
            let header = libc::CMSG_FIRSTHDR(&message);
            (*header).cmsg_level = libc::SOL_SOCKET;
            (*header).cmsg_type = libc::SCM_RIGHTS;
            (*header).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<RawFd>() as u32) as _;
            std::ptr::write_unaligned(libc::CMSG_DATA(header).cast::<RawFd>(), fd);
        }
    }
    for _ in 0..200 {
        // SAFETY: message points at live buffers for the duration of the call.
        let sent = unsafe { libc::sendmsg(socket, &message, libc::MSG_NOSIGNAL) };
        if sent >= 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::WouldBlock && error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    Err(io::ErrorKind::TimedOut.into())
}

/// Receives a reply and at most one descriptor (blocking socket).
pub fn receive_fd(socket: RawFd) -> io::Result<(Vec<u8>, Option<RawFd>)> {
    let mut data = vec![0_u8; 512];
    let mut iov = libc::iovec {
        iov_base: data.as_mut_ptr().cast(),
        iov_len: data.len(),
    };
    // SAFETY: pure size computation.
    let space = unsafe { libc::CMSG_SPACE(std::mem::size_of::<RawFd>() as u32) } as usize;
    let mut control = vec![0_u8; space];
    // SAFETY: zero is a valid msghdr.
    let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
    message.msg_iov = &mut iov;
    message.msg_iovlen = 1;
    message.msg_control = control.as_mut_ptr().cast();
    message.msg_controllen = space as _;
    let received = loop {
        // SAFETY: buffers outlive the call.
        let received = unsafe { libc::recvmsg(socket, &mut message, libc::MSG_CMSG_CLOEXEC) };
        if received >= 0 {
            break received as usize;
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    };
    data.truncate(received);
    let mut fd = None;
    // SAFETY: walking the control buffer filled by the kernel.
    unsafe {
        let mut header = libc::CMSG_FIRSTHDR(&message);
        while !header.is_null() {
            if (*header).cmsg_level == libc::SOL_SOCKET && (*header).cmsg_type == libc::SCM_RIGHTS {
                let received_fd = std::ptr::read_unaligned(libc::CMSG_DATA(header).cast::<RawFd>());
                if fd.is_some() {
                    libc::close(received_fd);
                } else {
                    fd = Some(received_fd);
                }
            }
            header = libc::CMSG_NXTHDR(&message, header);
        }
    }
    Ok((data, fd))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_table::{Layout, Patch};

    #[test]
    fn parses_only_well_formed_requests() {
        assert_eq!(
            parse(b"PAMSHM1 4096 octane.cache"),
            Some((4096, "octane.cache".to_owned()))
        );
        assert_eq!(parse(b"PAMSHM1 4097 octane.cache"), None);
        assert_eq!(parse(b"PAMSHM1 0 a"), None);
        assert_eq!(parse(b"PAMSHM1 4096 ../etc"), None);
        assert_eq!(parse(b"PAMSHM2 4096 a"), None);
        assert_eq!(parse(b"PAMSHM1 4096"), None);
        assert_eq!(parse(&[0xff; 20]), None);
    }

    #[test]
    fn master_hands_out_one_segment_per_key_within_budget() {
        let directory = std::env::temp_dir().join(format!("pam-shm-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir(&directory).unwrap();
        let socket = directory.join("shm.sock");
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let _server = runtime
            .block_on(async { SegmentServer::start(&socket, 3 * 4096 * 4) })
            .unwrap();

        let layout = Layout::new(4, 16, 8, 1).unwrap();
        let first =
            crate::shared_table::segment_client_request(&socket, "t.one", layout.segment_bytes)
                .unwrap();
        let second =
            crate::shared_table::segment_client_request(&socket, "t.one", layout.segment_bytes)
                .unwrap();
        let first = crate::shared_table::Table::attach(Arc::new(first), layout).unwrap();
        let second = crate::shared_table::Table::attach(Arc::new(second), layout).unwrap();
        first
            .set(
                b"k",
                &[Patch {
                    offset: 0,
                    bytes: &42_i64.to_ne_bytes(),
                }],
            )
            .unwrap();
        let mut out = [0_u8; 8];
        assert!(second.get(b"k", &mut out));
        assert_eq!(i64::from_ne_bytes(out), 42);

        assert!(
            crate::shared_table::segment_client_request(
                &socket,
                "t.one",
                layout.segment_bytes + 4096
            )
            .is_err()
        );
        let huge = crate::shared_table::segment_client_request(&socket, "t.huge", 1 << 30);
        assert!(huge.err().unwrap().contains("budget"));
        drop(runtime);
        std::fs::remove_dir_all(&directory).unwrap();
    }
}
