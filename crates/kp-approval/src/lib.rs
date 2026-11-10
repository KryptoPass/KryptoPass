//! Approval requests from `kryptopass-pinentry` to the running KryptoPass app.
//!
//! The app listens on `$XDG_RUNTIME_DIR/<app-id>/approval.sock`; the pinentry
//! connects once per prompt, sends one [`Request`] and reads one [`Decision`].
//! Both frames are a big-endian `u32` length and a body; see
//! `docs/aprobacion-desde-telefono.md`. Anything unexpected is an error, and the
//! pinentry treats every error as "ask the local dialog".

use std::io::{self, Read, Write};
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

use zeroize::{Zeroize, Zeroizing};

mod server;
pub use server::{Server, start};

/// The application whose socket both sides use.
pub const APP_ID: &str = env!("KP_APPROVAL_APP_ID");

const MAGIC: &str = "kryptopass-approval 1";
const MAX_FRAME: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// gpg-agent asks yes or no (`CONFIRM`).
    Confirm,
    /// gpg-agent asks for a passphrase (`GETPIN`).
    Passphrase,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Confirm => "confirm",
            Kind::Passphrase => "passphrase",
        }
    }
}

/// What gpg-agent said about the prompt. Text fields keep gpg-agent's Assuan
/// escapes (`%0A`, `%25`); [`unescape`] turns them into display text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub kind: Kind,
    /// `SETKEYINFO`: `n/<keygrip>`, `s/<keygrip>` (SSH) or `u/<id>`.
    pub key: Option<String>,
    /// `SETDESC`.
    pub description: String,
    /// `SETTITLE`.
    pub title: Option<String>,
    /// `OPTION owner=<pid>/<uid> <host>`: who asked gpg-agent.
    pub owner: Option<String>,
    /// gpg-agent rejected the previous answer (`SETERROR`).
    pub retry: bool,
}

/// The answer. A passphrase is wiped when dropped.
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Approved,
    Denied,
    Passphrase(Zeroizing<Vec<u8>>),
    /// Nobody can answer this one here: the pinentry shows its own dialog.
    Unavailable,
}

/// `$XDG_RUNTIME_DIR/<app-id>/approval.sock`, if the runtime directory is
/// private to this user.
pub fn socket_path() -> io::Result<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "XDG_RUNTIME_DIR is missing"))?;
    ensure_private_directory(&runtime)?;
    Ok(runtime.join(APP_ID).join("approval.sock"))
}

pub(crate) fn ensure_private_directory(path: &std::path::Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = std::fs::symlink_metadata(path)?;
    // SAFETY: geteuid has no preconditions.
    let uid = unsafe { libc::geteuid() };
    if metadata.is_dir()
        && !metadata.file_type().is_symlink()
        && metadata.uid() == uid
        && metadata.permissions().mode() & 0o077 == 0
    {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{} is not a private directory", path.display()),
        ))
    }
}

/// A request sent to the app, waiting for its answer.
pub struct Asking {
    stream: std::os::unix::net::UnixStream,
}

impl Asking {
    /// A handle that withdraws the request from another thread: shutting it
    /// down tells the app the answer is no longer wanted.
    pub fn canceller(&self) -> io::Result<std::os::unix::net::UnixStream> {
        self.stream.try_clone()
    }

    /// Waits at most `timeout` for the answer.
    pub fn finish(mut self, timeout: std::time::Duration) -> io::Result<Decision> {
        self.stream.set_read_timeout(Some(timeout))?;
        let body = Zeroizing::new(read_frame(&mut self.stream)?);
        decode_decision(&body)
    }
}

/// Connects to the app and sends the request without waiting for the answer.
pub fn begin(request: &Request) -> io::Result<Asking> {
    let path = socket_path()?;
    if let Some(directory) = path.parent() {
        ensure_private_directory(directory)?;
    }
    let mut stream = std::os::unix::net::UnixStream::connect(&path)?;
    stream.set_write_timeout(Some(std::time::Duration::from_secs(5)))?;
    write_frame(&mut stream, &encode_request(request)?)?;
    Ok(Asking { stream })
}

/// Asks the app and waits at most `timeout` for the answer.
pub fn ask(request: &Request, timeout: std::time::Duration) -> io::Result<Decision> {
    begin(request)?.finish(timeout)
}

pub(crate) fn encode_request(request: &Request) -> io::Result<Vec<u8>> {
    let mut body = format!("{MAGIC}\nkind {}\n", request.kind.name());
    let mut field = |name: &str, value: &str| -> io::Result<()> {
        if value.contains(['\n', '\r']) {
            return Err(invalid("a request field contains a line break"));
        }
        body.push_str(name);
        body.push(' ');
        body.push_str(value);
        body.push('\n');
        Ok(())
    };
    if let Some(key) = &request.key {
        field("key", key)?;
    }
    if let Some(title) = &request.title {
        field("title", title)?;
    }
    if let Some(owner) = &request.owner {
        field("owner", owner)?;
    }
    if request.retry {
        field("retry", "1")?;
    }
    field("description", &request.description)?;
    Ok(body.into_bytes())
}

pub(crate) fn decode_request(body: &[u8]) -> io::Result<Request> {
    let text = std::str::from_utf8(body).map_err(|_| invalid("request is not UTF-8"))?;
    let mut lines = text.lines();
    if lines.next() != Some(MAGIC) {
        return Err(invalid("unknown request format"));
    }
    let mut request = Request {
        kind: Kind::Confirm,
        key: None,
        description: String::new(),
        title: None,
        owner: None,
        retry: false,
    };
    let mut kind = None;
    for line in lines {
        let (name, value) = line.split_once(' ').unwrap_or((line, ""));
        match name {
            "kind" => {
                kind = Some(match value {
                    "confirm" => Kind::Confirm,
                    "passphrase" => Kind::Passphrase,
                    _ => return Err(invalid("unknown request kind")),
                })
            }
            "key" => request.key = Some(value.to_owned()),
            "title" => request.title = Some(value.to_owned()),
            "owner" => request.owner = Some(value.to_owned()),
            "retry" => request.retry = value == "1",
            "description" => request.description = value.to_owned(),
            // Newer pinentries may add fields; ignore what this side does not know.
            _ => {}
        }
    }
    request.kind = kind.ok_or_else(|| invalid("request kind is missing"))?;
    Ok(request)
}

pub(crate) fn encode_decision(decision: &Decision) -> Zeroizing<Vec<u8>> {
    let mut body = Zeroizing::new(Vec::new());
    match decision {
        Decision::Approved => body.push(1),
        Decision::Denied => body.push(2),
        Decision::Unavailable => body.push(3),
        Decision::Passphrase(secret) => {
            body.push(4);
            body.extend_from_slice(secret);
        }
    }
    body
}

fn decode_decision(body: &[u8]) -> io::Result<Decision> {
    match body {
        [1] => Ok(Decision::Approved),
        [2] => Ok(Decision::Denied),
        [3] => Ok(Decision::Unavailable),
        [4, secret @ ..] if !secret.is_empty() => {
            Ok(Decision::Passphrase(Zeroizing::new(secret.to_vec())))
        }
        _ => Err(invalid("unknown decision")),
    }
}

pub(crate) fn write_frame(out: &mut impl Write, body: &[u8]) -> io::Result<()> {
    if body.len() > MAX_FRAME {
        return Err(invalid("frame too large"));
    }
    let length = u32::try_from(body.len()).map_err(|_| invalid("frame too large"))?;
    out.write_all(&length.to_be_bytes())?;
    out.write_all(body)?;
    out.flush()
}

pub(crate) fn read_frame(input: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut length = [0; 4];
    input.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_FRAME {
        return Err(invalid("frame too large"));
    }
    let mut body = vec![0; length];
    if let Err(error) = input.read_exact(&mut body) {
        body.zeroize();
        return Err(error);
    }
    Ok(body)
}

/// Decodes Assuan `%XX` escapes for display; invalid escapes stay as written.
pub fn unescape(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && let Some(hex) = bytes.get(index + 1..index + 3)
            && let Ok(hex) = std::str::from_utf8(hex)
            && let Ok(value) = u8::from_str_radix(hex, 16)
        {
            out.push(value);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
