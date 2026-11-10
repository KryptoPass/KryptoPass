//! Answers a prompt through the running KryptoPass instead of the fallback.

use std::io::{self, Write};
use std::sync::mpsc::Sender;
use std::time::Duration;

use kryptopass_approval::{Decision, Kind, Request};
use zeroize::Zeroizing;

use crate::fallback::Event;
use crate::log;
use crate::proxy::Prompt;

/// How long the app (and later the phone) has to answer before the local
/// dialog opens. A person has to read and decide within it.
const TIMEOUT: Duration = Duration::from_secs(60);

/// gpg-error codes with the pinentry source (5 << 24), as pinentry sends them.
const NOT_CONFIRMED: &str = "ERR 83886194 Not confirmed <Pinentry>\n";
const CANCELED: &str = "ERR 83886179 Operation cancelled <Pinentry>\n";

/// Bytes of escaped data per `D` line; Assuan lines stop at 1000.
const DATA_CHUNK: usize = 900;

/// A request in flight to the app (and through it, the phone).
pub struct Remote {
    pub number: u64,
    pub kind: Kind,
    canceller: std::os::unix::net::UnixStream,
}

impl Remote {
    /// Withdraws the request: the app sees the socket close and tells the phone.
    pub fn cancel(self) {
        let _ = self.canceller.shutdown(std::net::Shutdown::Both);
    }
}

/// Sends the prompt to the app; its answer arrives later as
/// [`Event::Remote`]. `None` when the app cannot be asked at all.
pub fn start(prompt: &Prompt, number: u64, events: Sender<Event>) -> Option<Remote> {
    let kind = if prompt.last_command == "GETPIN" {
        Kind::Passphrase
    } else {
        Kind::Confirm
    };
    if prompt.retry || (kind == Kind::Passphrase && prompt.repeat) {
        // A rejected or new passphrase is always typed here.
        log::write("remote: skipped (retry or new passphrase)");
        return None;
    }
    if kind == Kind::Passphrase {
        // Only the passphrase of a real key may come from the phone. Prompts
        // without a keygrip (GET_PASSPHRASE, symmetric encryption, card PINs)
        // hand what is typed back to whoever asked gpg-agent.
        if !prompt.key.as_deref().is_some_and(is_keygrip) {
            log::write("remote: skipped (the prompt is not for a key)");
            return None;
        }
        // Like pinentry's own external cache: one remote passphrase per
        // connection, so a wrong stored one cannot be offered in a loop.
        if prompt.remote_passphrase_used.get() {
            log::write("remote: skipped (a phone passphrase was already used)");
            return None;
        }
    }
    let request = Request {
        kind,
        key: prompt.key.clone(),
        description: prompt.description.clone(),
        title: prompt.title.clone(),
        owner: prompt.owner.clone(),
        retry: prompt.retry,
    };
    let asking = match kryptopass_approval::begin(&request) {
        Ok(asking) => asking,
        Err(error) => {
            log::write(format_args!("remote: app not reachable ({error})"));
            return None;
        }
    };
    let canceller = asking.canceller().ok()?;
    let spawned = std::thread::Builder::new()
        .name("remote-answer".into())
        .spawn(move || {
            let _ = events.send(Event::Remote(number, asking.finish(TIMEOUT)));
        });
    if spawned.is_err() {
        return None;
    }
    log::write(format_args!("remote: request {number} sent to the app"));
    Some(Remote {
        number,
        kind,
        canceller,
    })
}

/// Writes the app's answer for gpg-agent. `false` when it is no answer at all
/// (unavailable, or not fitting the prompt) and the local dialog must decide.
pub fn reply(
    kind: Kind,
    decision: Decision,
    prompt: &Prompt,
    agent_out: &mut impl Write,
) -> io::Result<bool> {
    match (kind, decision) {
        (Kind::Confirm, Decision::Approved) => {
            log::write("remote: confirmed on the phone");
            agent_out.write_all(b"OK\n")?;
        }
        (Kind::Confirm, Decision::Denied) => {
            log::write("remote: denied on the phone");
            agent_out.write_all(NOT_CONFIRMED.as_bytes())?;
        }
        (Kind::Passphrase, Decision::Passphrase(secret)) => {
            log::write(format_args!(
                "remote: passphrase from the phone ({} bytes hidden)",
                secret.len()
            ));
            if prompt.external_cache {
                // Not typed by the person: gpg-agent must not count it as a try.
                agent_out.write_all(b"S PASSWORD_FROM_CACHE\n")?;
            }
            write_data(agent_out, &secret)?;
            agent_out.write_all(b"OK\n")?;
            prompt.remote_passphrase_used.set(true);
        }
        (Kind::Passphrase, Decision::Denied) => {
            log::write("remote: passphrase refused on the phone");
            agent_out.write_all(CANCELED.as_bytes())?;
        }
        (_, Decision::Unavailable) => {
            log::write("remote: the phone cannot answer");
            return Ok(false);
        }
        (kind, _) => {
            log::write(format_args!("remote: answer does not fit a {kind:?}"));
            return Ok(false);
        }
    }
    agent_out.flush()?;
    Ok(true)
}

/// `n/<keygrip>` (OpenPGP, X.509) or `s/<keygrip>` (SSH): 40 hex digits.
fn is_keygrip(key: &str) -> bool {
    key.strip_prefix("n/")
        .or_else(|| key.strip_prefix("s/"))
        .is_some_and(|grip| grip.len() == 40 && grip.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

/// Sends a secret as `D` lines with `%`, CR and LF escaped, never splitting an escape.
fn write_data(out: &mut impl Write, secret: &[u8]) -> io::Result<()> {
    let mut escaped = Zeroizing::new(Vec::with_capacity(secret.len() + 16));
    for byte in secret {
        match byte {
            b'%' => escaped.extend_from_slice(b"%25"),
            b'\r' => escaped.extend_from_slice(b"%0D"),
            b'\n' => escaped.extend_from_slice(b"%0A"),
            _ => escaped.push(*byte),
        }
    }
    let mut start = 0;
    while start < escaped.len() {
        let mut end = (start + DATA_CHUNK).min(escaped.len());
        // Every `%` in `escaped` starts an escape: back off so `%XX` never
        // straddles two lines.
        if end < escaped.len() {
            if escaped[end - 1] == b'%' {
                end -= 1;
            } else if escaped[end - 2] == b'%' {
                end -= 2;
            }
        }
        let mut line = Zeroizing::new(Vec::with_capacity(end - start + 3));
        line.extend_from_slice(b"D ");
        line.extend_from_slice(&escaped[start..end]);
        line.push(b'\n');
        out.write_all(&line)?;
        start = end;
    }
    Ok(())
}
