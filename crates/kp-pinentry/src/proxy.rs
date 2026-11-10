//! Relays gpg-agent ⇄ fallback pinentry one command at a time.
//!
//! Before a `CONFIRM` or `GETPIN` reaches the fallback, the running KryptoPass
//! gets the chance to answer it (see [`crate::remote`]). Anything it cannot
//! answer goes to the fallback, so the local dialog is always the last word.

use std::io::{self, BufRead, Write};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::assuan::{Line, Reply, classify, read_line};
use crate::fallback::{Event, Fallback};
use crate::remote::Remote;
use crate::{log, remote};

/// How long the app gets to answer before the local dialog opens. An app
/// without a connected phone answers "unavailable" well within it.
const QUICK_ANSWER: Duration = Duration::from_millis(300);

/// Added to the local dialog while the phone can also answer.
const PHONE_HINT: &str = "%0A%0ATambién puedes responder en tu teléfono.";

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
    /// A passphrase from the phone already answered a prompt on this connection.
    pub remote_passphrase_used: std::cell::Cell<bool>,
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
    fallback: &mut Fallback,
    events: &Receiver<Event>,
) -> io::Result<()> {
    let mut prompt = Prompt::default();
    log::write("waiting for the fallback greeting");
    for line in fallback.start(events)? {
        send(agent_out, &line)?;
        log::write(format_args!("pinentry> {}", describe(&line, false)));
    }
    let mut number = 0_u64;
    loop {
        let Some(command) = read_line(agent_in)? else {
            log::write("gpg-agent closed the connection");
            return Ok(());
        };
        prompt.observe(&command);
        fallback.remember(&command);
        log::write(format_args!("agent> {}", describe(&command, false)));
        if matches!(
            prompt.last_command.as_str(),
            "GETPIN" | "CONFIRM" | "MESSAGE"
        ) {
            log::write(format_args!("prompt {}", prompt.summary()));
        }
        if prompt.is_question() {
            number += 1;
            question(
                &command, &prompt, number, agent_in, agent_out, fallback, events,
            )?;
            prompt.finish();
            continue;
        }
        fallback.ensure_running(events)?;
        fallback.write(command.bytes())?;
        relay(agent_in, agent_out, fallback, events, &prompt, &mut None)?;
        prompt.finish();
        if prompt.last_command == "BYE" {
            return Ok(());
        }
    }
}

/// A `CONFIRM` or `GETPIN`: the phone and the local dialog race, the first
/// answer wins and the other is withdrawn.
fn question(
    command: &Line,
    prompt: &Prompt,
    number: u64,
    agent_in: &mut impl BufRead,
    agent_out: &mut impl Write,
    fallback: &mut Fallback,
    events: &Receiver<Event>,
) -> io::Result<()> {
    let mut pending = remote::start(prompt, number, fallback.events());
    if let Some(asked) = pending.take() {
        match wait_remote(events, fallback, asked.number, QUICK_ANSWER) {
            Some(Ok(decision)) => {
                if remote::reply(asked.kind, decision, prompt, agent_out)? {
                    return Ok(());
                }
            }
            Some(Err(error)) => log::write(format_args!("remote: failed ({error})")),
            None => pending = Some(asked),
        }
    }
    fallback.ensure_running(events)?;
    let hinted = pending.is_some();
    if hinted {
        log::write("remote: the phone may answer; local dialog opens too");
        set_description(
            fallback,
            events,
            &format!("{}{PHONE_HINT}", prompt.description),
        )?;
    }
    fallback.write(command.bytes())?;
    relay(agent_in, agent_out, fallback, events, prompt, &mut pending)?;
    if hinted && fallback.is_running() {
        set_description(fallback, events, &prompt.description)?;
    }
    Ok(())
}

/// Waits up to `timeout` for the answer to remote request `number`.
fn wait_remote(
    events: &Receiver<Event>,
    fallback: &mut Fallback,
    number: u64,
    timeout: Duration,
) -> Option<io::Result<kryptopass_approval::Decision>> {
    let deadline = Instant::now() + timeout;
    loop {
        let left = deadline.checked_duration_since(Instant::now())?;
        match events.recv_timeout(left) {
            Ok(Event::Remote(answered, result)) if answered == number => return Some(result),
            Ok(Event::Closed(generation)) if generation == fallback.generation() => {
                fallback.closed()
            }
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) => return None,
            Err(RecvTimeoutError::Disconnected) => {
                return Some(Err(io::ErrorKind::BrokenPipe.into()));
            }
        }
    }
}

/// Sends a description to the fallback only, swallowing its reply.
fn set_description(
    fallback: &mut Fallback,
    events: &Receiver<Event>,
    description: &str,
) -> io::Result<()> {
    fallback.write(format!("SETDESC {description}\n").as_bytes())?;
    fallback.read_reply(events)?;
    Ok(())
}

/// Copies the fallback's reply to gpg-agent until `OK` or `ERR`, passing any
/// `INQUIRE` exchange through. If the phone answers first, its answer goes to
/// gpg-agent instead and the dialog is closed.
fn relay(
    agent_in: &mut impl BufRead,
    agent_out: &mut impl Write,
    fallback: &mut Fallback,
    events: &Receiver<Event>,
    prompt: &Prompt,
    pending: &mut Option<Remote>,
) -> io::Result<()> {
    let public_data = prompt.last_command == "GETINFO";
    loop {
        let event = events
            .recv()
            .map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?;
        match event {
            Event::Line(generation, reply) if generation == fallback.generation() => {
                send(agent_out, &reply)?;
                log::write(format_args!("pinentry> {}", describe(&reply, public_data)));
                match classify(&reply) {
                    Reply::Final => {
                        if let Some(asked) = pending.take() {
                            log::write("remote: answered locally first; withdrawing");
                            asked.cancel();
                        }
                        return Ok(());
                    }
                    Reply::Inquire => relay_inquiry(agent_in, fallback)?,
                    Reply::Data | Reply::Other => {}
                }
            }
            Event::Closed(generation) if generation == fallback.generation() => {
                fallback.closed();
                log::write("the fallback pinentry exited in the middle of a command");
                if let Some(asked) = pending.take() {
                    asked.cancel();
                }
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            Event::Remote(number, result)
                if pending.as_ref().is_some_and(|asked| asked.number == number) =>
            {
                let Some(asked) = pending.take() else {
                    continue;
                };
                match result {
                    Ok(decision) => {
                        if remote::reply(asked.kind, decision, prompt, agent_out)? {
                            fallback.kill();
                            return Ok(());
                        }
                    }
                    Err(error) => log::write(format_args!("remote: failed ({error})")),
                }
                log::write("remote: no answer from the phone; the local dialog stays");
            }
            _ => {}
        }
    }
}

/// gpg-agent answers an `INQUIRE` with `D` lines and `END`, or with `CAN`.
fn relay_inquiry(agent_in: &mut impl BufRead, fallback: &mut Fallback) -> io::Result<()> {
    loop {
        let Some(answer) = read_line(agent_in)? else {
            log::write("gpg-agent closed the connection during an inquiry");
            return Err(io::ErrorKind::UnexpectedEof.into());
        };
        fallback.write(answer.bytes())?;
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
