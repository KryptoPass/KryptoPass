//! Relays gpg-agent ⇄ fallback pinentry one command at a time.
//!
//! Before a `CONFIRM` or `GETPIN` reaches the fallback, the running KryptoPass
//! gets the chance to answer it (see [`crate::remote`]). Anything it cannot
//! answer goes to the fallback, so the local dialog is always the last word.

use std::io::{self, BufRead, Write};

use crate::assuan::{Line, Reply, classify, read_line};
use crate::{log, remote};

/// Longest argument text copied into the log.
const LOG_ARGUMENT: usize = 300;

/// What gpg-agent said about the prompt it is about to request.
#[derive(Default)]
pub struct Prompt {
    pub key: Option<String>,
    pub owner: Option<String>,
    pub description: String,
    pub title: Option<String>,
    /// gpg-agent set an error: the previous answer was wrong.
    pub retry: bool,
    /// gpg-agent wants the passphrase twice: a new one is being chosen.
    pub repeat: bool,
    pub external_cache: bool,
    pub last_command: String,
    last_argument: String,
}

impl Prompt {
    fn observe(&mut self, command: &Line) {
        let keyword = command.keyword();
        let argument = String::from_utf8_lossy(command.argument()).into_owned();
        match keyword.as_str() {
            "SETKEYINFO" => {
                self.key =
                    (!argument.is_empty() && argument != "--clear").then(|| argument.clone());
            }
            "SETDESC" => self.description = argument.clone(),
            "SETTITLE" => self.title = (!argument.is_empty()).then(|| argument.clone()),
            "SETERROR" => self.retry = !argument.is_empty(),
            "SETREPEAT" => self.repeat = true,
            "OPTION" => {
                if let Some(owner) = argument.strip_prefix("owner=") {
                    self.owner = Some(owner.to_owned());
                } else if argument == "allow-external-password-cache" {
                    self.external_cache = true;
                }
            }
            _ => {}
        }
        self.last_command = keyword;
        self.last_argument = argument;
    }

    /// `MESSAGE` and `CONFIRM --one-button` only inform; there is nothing to approve.
    pub fn is_question(&self) -> bool {
        match self.last_command.as_str() {
            "GETPIN" => true,
            "CONFIRM" => !self.last_argument.contains("--one-button"),
            _ => false,
        }
    }

    fn summary(&self) -> String {
        format!(
            "{} key={} owner={} retry={} repeat={} external-cache={}",
            self.last_command,
            self.key.as_deref().unwrap_or("-"),
            self.owner.as_deref().unwrap_or("-"),
            self.retry,
            self.repeat,
            self.external_cache,
        )
    }

    /// The command is over; per-prompt state starts clean for the next one.
    fn finish(&mut self) {
        if matches!(self.last_command.as_str(), "GETPIN" | "CONFIRM" | "MESSAGE") {
            self.retry = false;
            self.repeat = false;
        }
    }
}

pub fn run(
    agent_in: &mut impl BufRead,
    agent_out: &mut impl Write,
    pinentry_in: &mut impl BufRead,
    pinentry_out: &mut impl Write,
) -> io::Result<()> {
    let mut prompt = Prompt::default();
    log::write("waiting for the fallback greeting");
    relay_reply(agent_in, agent_out, pinentry_in, pinentry_out, &prompt)?;
    loop {
        let Some(command) = read_line(agent_in)? else {
            log::write("gpg-agent closed the connection");
            return Ok(());
        };
        prompt.observe(&command);
        log::write(format_args!("agent> {}", describe(&command, false)));
        if matches!(
            prompt.last_command.as_str(),
            "GETPIN" | "CONFIRM" | "MESSAGE"
        ) {
            log::write(format_args!("prompt {}", prompt.summary()));
        }
        if prompt.is_question() && remote::answer(&prompt, agent_out)? {
            prompt.finish();
            continue;
        }
        send(pinentry_out, &command)?;
        relay_reply(agent_in, agent_out, pinentry_in, pinentry_out, &prompt)?;
        prompt.finish();
        if prompt.last_command == "BYE" {
            return Ok(());
        }
    }
}

/// Copies the fallback's reply to gpg-agent until `OK` or `ERR`, passing any
/// `INQUIRE` exchange through in both directions.
fn relay_reply(
    agent_in: &mut impl BufRead,
    agent_out: &mut impl Write,
    pinentry_in: &mut impl BufRead,
    pinentry_out: &mut impl Write,
    prompt: &Prompt,
) -> io::Result<()> {
    let public_data = prompt.last_command == "GETINFO";
    loop {
        let Some(reply) = read_line(pinentry_in)? else {
            log::write("the fallback pinentry exited in the middle of a command");
            return Err(io::ErrorKind::UnexpectedEof.into());
        };
        send(agent_out, &reply)?;
        log::write(format_args!("pinentry> {}", describe(&reply, public_data)));
        match classify(&reply) {
            Reply::Final => return Ok(()),
            Reply::Inquire => relay_inquiry(agent_in, pinentry_out)?,
            Reply::Data | Reply::Other => {}
        }
    }
}

/// gpg-agent answers an `INQUIRE` with `D` lines and `END`, or with `CAN`.
fn relay_inquiry(agent_in: &mut impl BufRead, pinentry_out: &mut impl Write) -> io::Result<()> {
    loop {
        let Some(answer) = read_line(agent_in)? else {
            log::write("gpg-agent closed the connection during an inquiry");
            return Err(io::ErrorKind::UnexpectedEof.into());
        };
        send(pinentry_out, &answer)?;
        log::write(format_args!("agent> {}", describe(&answer, false)));
        if matches!(answer.keyword().as_str(), "END" | "CAN") {
            return Ok(());
        }
    }
}

fn send(out: &mut impl Write, line: &Line) -> io::Result<()> {
    out.write_all(line.bytes())?;
    out.flush()
}

/// A loggable form of a line. Data lines and inquiry arguments can hold a
/// passphrase (`D <pin>`, `INQUIRE QUALITY <pin>`), so only their length shows.
fn describe(line: &Line, public_data: bool) -> String {
    let keyword = line.keyword();
    let argument = line.argument();
    match keyword.as_str() {
        "D" if !public_data => format!("D <{} bytes hidden>", argument.len()),
        "INQUIRE" => {
            let what = argument.split(|byte| *byte == b' ').next().unwrap_or(&[]);
            format!(
                "INQUIRE {} <{} bytes hidden>",
                String::from_utf8_lossy(what),
                argument.len().saturating_sub(what.len())
            )
        }
        _ => {
            let shown = &argument[..argument.len().min(LOG_ARGUMENT)];
            let cut = if shown.len() < argument.len() {
                "…"
            } else {
                ""
            };
            format!("{keyword} {}{cut}", String::from_utf8_lossy(shown))
        }
    }
}
