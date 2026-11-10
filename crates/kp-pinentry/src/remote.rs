//! Answers a prompt through the running KryptoPass instead of the fallback.

use std::io::{self, Write};
use std::time::Duration;

use kryptopass_approval::{Decision, Kind, Request};
use zeroize::Zeroizing;

use crate::log;
use crate::proxy::Prompt;

/// How long the app (and later the phone) has to answer before the local
/// dialog opens. A person has to read and decide within it.
const TIMEOUT: Duration = Duration::from_secs(60);

/// gpg-error codes with the pinentry source (5 << 24), as pinentry sends them.
const NOT_CONFIRMED: &str = "ERR 83886194 Not confirmed <Pinentry>\n";

/// Bytes of escaped data per `D` line; Assuan lines stop at 1000.
const DATA_CHUNK: usize = 900;

/// Returns `true` when the app answered and gpg-agent has its reply;
/// `false` sends the prompt to the fallback.
pub fn answer(prompt: &Prompt, agent_out: &mut impl Write) -> io::Result<bool> {
    let kind = if prompt.last_command == "GETPIN" {
        Kind::Passphrase
    } else {
        Kind::Confirm
    };
    if kind == Kind::Passphrase && (prompt.retry || prompt.repeat) {
        // A rejected or new passphrase is always typed here.
        log::write("remote: skipped (retry or new passphrase); using the fallback");
        return Ok(false);
    }
    let request = Request {
        kind,
        key: prompt.key.clone(),
        description: prompt.description.clone(),
        title: prompt.title.clone(),
        owner: prompt.owner.clone(),
        retry: prompt.retry,
    };
    let decision = match kryptopass_approval::ask(&request, TIMEOUT) {
        Ok(decision) => decision,
        Err(error) => {
            log::write(format_args!(
                "remote: no answer ({error}); using the fallback"
            ));
            return Ok(false);
        }
    };
    match (kind, decision) {
        (Kind::Confirm, Decision::Approved) => {
            log::write("remote: confirmed");
            agent_out.write_all(b"OK\n")?;
        }
        (Kind::Confirm, Decision::Denied) => {
            log::write("remote: denied");
            agent_out.write_all(NOT_CONFIRMED.as_bytes())?;
        }
        (Kind::Passphrase, Decision::Passphrase(secret)) => {
            log::write(format_args!(
                "remote: passphrase received ({} bytes hidden)",
                secret.len()
            ));
            if prompt.external_cache {
                // Not typed by the person: gpg-agent must not count it as a try.
                agent_out.write_all(b"S PASSWORD_FROM_CACHE\n")?;
            }
            write_data(agent_out, &secret)?;
            agent_out.write_all(b"OK\n")?;
        }
        (Kind::Passphrase, Decision::Denied) => {
            log::write("remote: passphrase refused");
            agent_out.write_all(b"ERR 83886179 Operation cancelled <Pinentry>\n")?;
        }
        (_, Decision::Unavailable) => {
            log::write("remote: unavailable; using the fallback");
            return Ok(false);
        }
        (kind, _) => {
            log::write(format_args!(
                "remote: answer does not fit a {kind:?}; using the fallback"
            ));
            return Ok(false);
        }
    }
    agent_out.flush()?;
    Ok(true)
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
