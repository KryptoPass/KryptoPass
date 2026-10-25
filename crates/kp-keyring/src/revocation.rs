//! Revocation dialogues with gpg and validation of revocation certificates.
//!
//! Both dialogues accept only the exact prompt sequence expected; any other
//! question aborts instead of being answered blindly.

use std::{
    fs, io,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use gpgme::{Context, InteractionStatus, Interactor, Key};
use kryptopass_secure_memory::SensitiveBytes;
use tracing::{debug, instrument, trace};

use crate::{
    backend::MAX_KEY_BLOB_BYTES,
    error::{CommandError, Error, FileError, Result, Step, gpg},
    files,
};

const MAX_STATUS_LINE_BYTES: usize = 4096;
const BEGIN: &str = "-----BEGIN PGP PUBLIC KEY BLOCK-----";
const END: &str = "-----END PGP PUBLIC KEY BLOCK-----";

/// Prompts of `gpg --gen-revoke` and the answers given, in order.
const GEN_REVOKE_DIALOGUE: [(&str, &str, &str); 4] = [
    ("GET_BOOL", "gen_revoke.okay", "y"),
    ("GET_LINE", "ask_revocation_reason.code", "0"),
    ("GET_LINE", "ask_revocation_reason.text", ""),
    ("GET_BOOL", "ask_revocation_reason.okay", "y"),
];

pub(crate) fn answer_gen_revoke(
    status: impl Read,
    mut commands: impl Write,
) -> Result<(), CommandError> {
    let mut reader = BufReader::new(status);
    let mut step = 0usize;
    while let Some(line) = read_status_line(&mut reader)? {
        let Some((keyword, arguments)) = parse_status_line(&line)? else {
            continue;
        };
        if !matches!(keyword, "GET_LINE" | "GET_BOOL") {
            continue;
        }
        let Some(&(expected_keyword, expected_arguments, answer)) = GEN_REVOKE_DIALOGUE.get(step)
        else {
            return Err(CommandError::Protocol(
                "more revocation prompts than expected",
            ));
        };
        if (keyword, arguments) != (expected_keyword, expected_arguments) {
            return Err(CommandError::Protocol("unexpected revocation prompt"));
        }
        writeln!(commands, "{answer}")
            .and_then(|()| commands.flush())
            .map_err(CommandError::Io)?;
        step += 1;
    }
    if step != GEN_REVOKE_DIALOGUE.len() {
        return Err(CommandError::Protocol(
            "gpg ended the revocation dialogue early",
        ));
    }
    Ok(())
}

fn read_status_line(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>, CommandError> {
    let mut line = Vec::new();
    let mut oversized = false;
    loop {
        let available = reader.fill_buf().map_err(CommandError::Io)?;
        if available.is_empty() {
            if line.is_empty() {
                return Ok(None);
            }
            break;
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let count = newline.map_or(available.len(), |index| index + 1);
        if !oversized && line.len().saturating_add(count) <= MAX_STATUS_LINE_BYTES {
            line.extend_from_slice(&available[..count]);
        } else {
            oversized = true;
        }
        reader.consume(count);
        if newline.is_some() {
            break;
        }
    }
    if oversized {
        return Err(CommandError::Protocol("status line too long"));
    }
    while matches!(line.last(), Some(b'\n' | b'\r')) {
        line.pop();
    }
    Ok(Some(line))
}

fn parse_status_line(line: &[u8]) -> Result<Option<(&str, &str)>, CommandError> {
    let Some(payload) = line.strip_prefix(b"[GNUPG:] ") else {
        return Ok(None);
    };
    let payload = std::str::from_utf8(payload)
        .map_err(|_| CommandError::Protocol("status line is not UTF-8"))?;
    Ok(Some(payload.split_once(' ').unwrap_or((payload, ""))))
}

/// Revokes subkey number `index` (1-based, as `gpg --edit-key` counts).
#[instrument(level = "debug", skip(context, key), fields(index = index))]
pub(crate) fn revoke_subkey(context: &mut Context, key: &Key, index: usize) -> Result<()> {
    let state = Arc::new(Mutex::new(0usize));
    let interactor = SubkeyRevocation {
        index,
        step: Arc::clone(&state),
    };
    // gpg writes the edit listing into this sink; a read-only buffer such as
    // `""` makes the whole interaction fail with EBADF.
    let mut listing = gpgme::Data::new().map_err(gpg(Step::RevokeSubkey))?;
    context
        .interact(key, interactor, &mut listing)
        .map_err(gpg(Step::RevokeSubkey))?;
    let step = *state
        .lock()
        .map_err(|_| CommandError::Protocol("subkey dialogue state poisoned"))?;
    if step < SUBKEY_DIALOGUE_STEPS {
        return Err(CommandError::Protocol("gpg ended before saving the revocation").into());
    }
    debug!("Keyring: revoked subkey through GPGME");
    Ok(())
}

/// Answers up to and including `save`. GnuPG 2.4 saves without asking
/// `keyedit.save.okay`; the answer to it stays for versions that do ask.
const SUBKEY_DIALOGUE_STEPS: usize = 7;

struct SubkeyRevocation {
    index: usize,
    step: Arc<Mutex<usize>>,
}

impl Interactor for SubkeyRevocation {
    fn interact(
        &mut self,
        status: InteractionStatus<'_>,
        out: Option<&mut dyn Write>,
    ) -> Result<(), gpgme::Error> {
        let keyword = status.keyword().unwrap_or("");
        let arguments = status.args().unwrap_or("");
        let Some(out) = out else {
            // Without a channel to answer on, only non-questions can pass.
            return if matches!(keyword, "GET_LINE" | "GET_BOOL") {
                Err(gpgme::Error::GENERAL)
            } else {
                Ok(())
            };
        };
        let mut step = self.step.lock().map_err(|_| gpgme::Error::GENERAL)?;
        let answer = match (*step, keyword, arguments) {
            (0, "GET_LINE", "keyedit.prompt") => format!("key {}", self.index),
            (1, "GET_LINE", "keyedit.prompt") => "revkey".to_owned(),
            (2, "GET_BOOL", "keyedit.revoke.subkey.okay") => "y".to_owned(),
            (3, "GET_LINE", "ask_revocation_reason.code") => "0".to_owned(),
            (4, "GET_LINE", "ask_revocation_reason.text") => String::new(),
            (5, "GET_BOOL", "ask_revocation_reason.okay") => "y".to_owned(),
            (6, "GET_LINE", "keyedit.prompt") => "save".to_owned(),
            (7, "GET_BOOL", "keyedit.save.okay") => "y".to_owned(),
            _ => return Err(gpgme::Error::GENERAL),
        };
        writeln!(out, "{answer}").map_err(|_| gpgme::Error::GENERAL)?;
        *step += 1;
        Ok(())
    }
}

/// Where gpg stores the certificate it pre-generates for each new key.
pub(crate) fn stored_certificate_path(home: &Path, fingerprint: &str) -> PathBuf {
    home.join("openpgp-revocs.d")
        .join(format!("{fingerprint}.rev"))
}

/// Reads gpg's pre-generated certificate, or `None` when it does not exist.
#[instrument(
    level = "trace",
    skip_all,
    fields(path = %path.display())
)]
pub(crate) fn read_stored_certificate(path: &Path) -> Result<Option<SensitiveBytes>> {
    if let Some(folder) = path.parent() {
        match fs::symlink_metadata(folder) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => return Err(FileError::NotRegular(folder.to_owned()).into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                trace!("Keyring: no stored revocation certificate");
                return Ok(None);
            }
            Err(source) => {
                return Err(FileError::Open {
                    path: folder.to_owned(),
                    source,
                }
                .into());
            }
        }
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            let raw = files::read_bounded(path, MAX_KEY_BLOB_BYTES as u64)?;
            let certificate = normalize_certificate(&raw)?;
            trace!("Keyring: loaded stored revocation certificate");
            Ok(Some(certificate))
        }
        Ok(_) => Err(FileError::NotRegular(path.to_owned()).into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            trace!("Keyring: no stored revocation certificate");
            Ok(None)
        }
        Err(source) => Err(FileError::Open {
            path: path.to_owned(),
            source,
        }
        .into()),
    }
}

/// Extracts the single armored block from gpg's certificate file, whose
/// comment lines and `:` escape before the block are dropped.
#[instrument(level = "trace", skip_all, fields(bytes = raw.len()))]
pub(crate) fn normalize_certificate(raw: &[u8]) -> Result<SensitiveBytes> {
    let begin = single_position(raw, BEGIN.as_bytes())?;
    let end = single_position(raw, END.as_bytes())?;
    if end <= begin {
        return Err(Error::MalformedRevocation("not one complete block"));
    }
    let block = &raw[begin..end + END.len()];
    validate_armor(block)?;
    trace!(
        bytes = block.len(),
        "Keyring: normalized revocation certificate"
    );
    Ok(SensitiveBytes::from(block))
}

fn single_position(haystack: &[u8], needle: &[u8]) -> Result<usize> {
    let mut positions = haystack
        .windows(needle.len())
        .enumerate()
        .filter_map(|(index, window)| (window == needle).then_some(index));
    match (positions.next(), positions.next()) {
        (Some(position), None) => Ok(position),
        _ => Err(Error::MalformedRevocation("not one complete block")),
    }
}

fn validate_armor(block: &[u8]) -> Result<()> {
    let malformed = Error::MalformedRevocation;
    let text = std::str::from_utf8(block)
        .ok()
        .filter(|text| text.is_ascii())
        .ok_or(malformed("not ASCII"))?;
    let mut lines = text.lines();
    if lines.next() != Some(BEGIN) {
        return Err(malformed("bad header line"));
    }
    let mut in_body = false;
    let mut body_lines = 0usize;
    let mut checksum_seen = false;
    let mut footer_seen = false;
    for line in lines {
        if footer_seen {
            return Err(malformed("data after the footer"));
        }
        if !in_body {
            if line.is_empty() {
                in_body = true;
            } else if line.len() > 128 || !line.bytes().all(|byte| (0x20..=0x7e).contains(&byte)) {
                return Err(malformed("bad armor header"));
            }
            continue;
        }
        if line == END {
            footer_seen = true;
        } else if let Some(checksum) = line.strip_prefix('=') {
            if checksum_seen || checksum.len() != 4 || !checksum.bytes().all(is_base64) {
                return Err(malformed("bad checksum"));
            }
            checksum_seen = true;
        } else if checksum_seen
            || line.is_empty()
            || line.len() > 76
            || !line.bytes().all(is_base64)
        {
            return Err(malformed("bad armored data"));
        } else {
            body_lines += 1;
        }
    }
    if !in_body || !footer_seen || body_lines == 0 {
        return Err(malformed("incomplete"));
    }
    Ok(())
}

fn is_base64(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=')
}
