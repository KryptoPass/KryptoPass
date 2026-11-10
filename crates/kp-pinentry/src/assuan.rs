//! The few pieces of the Assuan line protocol the proxy needs.
//!
//! Lines are raw bytes: data lines may carry any percent-escaped value, so
//! nothing here assumes UTF-8.

use std::io::{self, BufRead};

use zeroize::Zeroize;

/// Assuan allows 1000 bytes per line; accept some slack before giving up.
const MAX_LINE: usize = 4096;

/// One protocol line, newline included. Wiped on drop because data lines can
/// hold a passphrase.
pub struct Line(Vec<u8>);

impl Line {
    /// A line built here rather than read; must end with a newline to be sent.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(bytes.to_vec())
    }

    pub fn bytes(&self) -> &[u8] {
        &self.0
    }

    /// The first word, upper-cased (`getpin` and `GETPIN` are the same command).
    pub fn keyword(&self) -> String {
        let text = self.trimmed();
        let end = text
            .iter()
            .position(|byte| *byte == b' ')
            .unwrap_or(text.len());
        String::from_utf8_lossy(&text[..end]).to_ascii_uppercase()
    }

    /// Everything after the first word, without the separating space.
    pub fn argument(&self) -> &[u8] {
        let text = self.trimmed();
        match text.iter().position(|byte| *byte == b' ') {
            Some(space) => &text[space + 1..],
            None => &[],
        }
    }

    fn trimmed(&self) -> &[u8] {
        let mut text = self.0.as_slice();
        while let [rest @ .., b'\n' | b'\r'] = text {
            text = rest;
        }
        text
    }
}

impl Drop for Line {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// Reads one line; `None` at end of stream.
pub fn read_line(reader: &mut impl BufRead) -> io::Result<Option<Line>> {
    let mut buffer = Vec::with_capacity(128);
    loop {
        let available = match reader.fill_buf() {
            Ok(available) => available,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => {
                buffer.zeroize();
                return Err(error);
            }
        };
        if available.is_empty() {
            if buffer.is_empty() {
                return Ok(None);
            }
            // A last line without newline still counts.
            return Ok(Some(Line(buffer)));
        }
        let (taken, done) = match available.iter().position(|byte| *byte == b'\n') {
            Some(newline) => (newline + 1, true),
            None => (available.len(), false),
        };
        buffer.extend_from_slice(&available[..taken]);
        reader.consume(taken);
        if buffer.len() > MAX_LINE {
            buffer.zeroize();
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Assuan line too long",
            ));
        }
        if done {
            return Ok(Some(Line(buffer)));
        }
    }
}

/// How a pinentry reply line ends (or continues) a command.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    /// `OK` or `ERR`: the command is over.
    Final,
    /// `INQUIRE`: the pinentry asks gpg-agent for something.
    Inquire,
    /// `D`: data, possibly secret.
    Data,
    /// `S`, `#` or anything else that only informs.
    Other,
}

pub fn classify(line: &Line) -> Reply {
    match line.keyword().as_str() {
        "OK" | "ERR" => Reply::Final,
        "INQUIRE" => Reply::Inquire,
        "D" => Reply::Data,
        _ => Reply::Other,
    }
}
