//! The app side: accepts one request per connection and hands it to a handler.

use std::io;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use crate::{Decision, Request, decode_request, encode_decision, read_frame, write_frame};

/// Prompts waiting at the same time; more are refused (the pinentry falls back).
const MAX_CONNECTIONS: usize = 4;
/// Time a connected pinentry has to send its request.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Answers one request. The second argument tells whether the pinentry is
/// still waiting; once it returns `false` the answer is no longer wanted.
pub type Handler = dyn Fn(Request, &dyn Fn() -> bool) -> Decision + Send + Sync;

/// Stops listening and removes the socket when dropped.
pub struct Server {
    path: PathBuf,
    stop: Arc<AtomicBool>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Listens on [`crate::socket_path`]. Fails if another instance already
/// answers there; a stale socket left by a crash is replaced.
pub fn start(handler: Arc<Handler>) -> io::Result<Server> {
    let path = crate::socket_path()?;
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::other("socket path has no directory"))?;
    match std::fs::DirBuilder::new().mode(0o700).create(directory) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    crate::ensure_private_directory(directory)?;
    if std::fs::symlink_metadata(&path).is_ok() {
        if UnixStream::connect(&path).is_ok() {
            return Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                "another KryptoPass already answers approvals",
            ));
        }
        std::fs::remove_file(&path)?;
    }
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let active = Arc::new(AtomicUsize::new(0));
    std::thread::Builder::new()
        .name("approval-listener".into())
        .spawn(move || {
            while !stopping.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => accept(stream, &handler, &active),
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(50))
                    }
                    Err(_) => break,
                }
            }
        })?;
    Ok(Server { path, stop })
}

fn accept(stream: UnixStream, handler: &Arc<Handler>, active: &Arc<AtomicUsize>) {
    if active.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS {
        active.fetch_sub(1, Ordering::AcqRel);
        return;
    }
    let handler = handler.clone();
    let done = active.clone();
    let spawned = std::thread::Builder::new()
        .name("approval-request".into())
        .spawn(move || {
            serve(stream, &handler);
            done.fetch_sub(1, Ordering::AcqRel);
        });
    if spawned.is_err() {
        active.fetch_sub(1, Ordering::AcqRel);
    }
}

fn serve(mut stream: UnixStream, handler: &Arc<Handler>) {
    if !same_user(&stream) {
        return;
    }
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(REQUEST_TIMEOUT));
    let Ok(body) = read_frame(&mut stream) else {
        return;
    };
    let Ok(request) = decode_request(&body) else {
        return;
    };
    let waiting = || still_connected(&stream);
    let decision = handler(request, &waiting);
    let _ = write_frame(&mut stream, &encode_decision(&decision));
}

/// The pinentry closes its end when its local dialog answered first.
fn still_connected(stream: &UnixStream) -> bool {
    use std::os::fd::AsRawFd;
    let mut byte = 0_u8;
    // SAFETY: a one-byte buffer that outlives the call.
    let read = unsafe {
        libc::recv(
            stream.as_raw_fd(),
            (&mut byte as *mut u8).cast(),
            1,
            libc::MSG_PEEK | libc::MSG_DONTWAIT,
        )
    };
    match read {
        0 => false,
        -1 => matches!(
            std::io::Error::last_os_error().kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
        ),
        _ => true,
    }
}

/// Only processes of this user may ask; the directory is already 0700, this
/// is a second fence.
fn same_user(stream: &UnixStream) -> bool {
    use std::os::fd::AsRawFd;
    let mut credentials = libc::ucred {
        pid: 0,
        uid: u32::MAX,
        gid: u32::MAX,
    };
    let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: the buffer and its length describe a valid `ucred`.
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut length,
        )
    };
    // SAFETY: geteuid has no preconditions.
    result == 0 && credentials.uid == unsafe { libc::geteuid() }
}
