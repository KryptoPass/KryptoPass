//! The real pinentry that shows the local dialog.
//!
//! A reader thread turns its output into [`Event`]s, so the proxy can wait on
//! the dialog and on the phone at the same time. When the phone answers first
//! the fallback is killed; the next prompt starts a new one and replays the
//! settings gpg-agent sent (options, texts, key info).

use std::ffi::OsString;
use std::io::{self, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender};

use crate::assuan::{Line, Reply, classify, read_line};
use crate::log;
use kryptopass_approval::Decision;

/// Everything the proxy waits on.
pub enum Event {
    /// A line from the fallback of this generation.
    Line(u64, Line),
    /// The fallback of this generation closed its output.
    Closed(u64),
    /// The app answered (or failed) the remote request with this number.
    Remote(u64, io::Result<Decision>),
}

pub struct Fallback {
    program: PathBuf,
    arguments: Vec<OsString>,
    events: Sender<Event>,
    generation: u64,
    running: Option<(Child, BufWriter<ChildStdin>)>,
    /// Setting commands to replay into a restarted fallback, latest per key.
    settings: Vec<(String, Line)>,
}

impl Fallback {
    pub fn new(program: PathBuf, arguments: Vec<OsString>, events: Sender<Event>) -> Self {
        Self {
            program,
            arguments,
            events,
            generation: 0,
            running: None,
            settings: Vec::new(),
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn events(&self) -> Sender<Event> {
        self.events.clone()
    }

    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// Starts the fallback and returns its greeting lines (up to `OK`/`ERR`).
    pub fn start(&mut self, events: &Receiver<Event>) -> io::Result<Vec<Line>> {
        self.generation += 1;
        let generation = self.generation;
        let mut child = spawn(&self.program, &self.arguments)?;
        let (Some(input), Some(output)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return Err(io::Error::other("the fallback pinentry has no pipes"));
        };
        log::write(format_args!(
            "fallback {} running as pid {} (generation {generation})",
            self.program.display(),
            child.id()
        ));
        let sender = self.events.clone();
        std::thread::Builder::new()
            .name("fallback-reader".into())
            .spawn(move || {
                let mut reader = BufReader::new(output);
                loop {
                    match read_line(&mut reader) {
                        Ok(Some(line)) => {
                            if sender.send(Event::Line(generation, line)).is_err() {
                                return;
                            }
                        }
                        Ok(None) | Err(_) => {
                            let _ = sender.send(Event::Closed(generation));
                            return;
                        }
                    }
                }
            })?;
        self.running = Some((child, BufWriter::new(input)));
        let greeting = self.read_reply(events)?;
        Ok(greeting)
    }

    /// Makes sure a fallback runs, replaying the settings into a new one.
    pub fn ensure_running(&mut self, events: &Receiver<Event>) -> io::Result<()> {
        if self.running.is_some() {
            return Ok(());
        }
        log::write("restarting the fallback and replaying its settings");
        self.start(events)?;
        let settings: Vec<Vec<u8>> = self
            .settings
            .iter()
            .map(|(_, line)| line.bytes().to_vec())
            .collect();
        for setting in settings {
            self.write(&setting)?;
            let reply = self.read_reply(events)?;
            if reply.last().is_some_and(|line| line.keyword() == "ERR") {
                log::write("the restarted fallback refused a replayed setting");
            }
        }
        Ok(())
    }

    /// Remembers a command that configures the dialog, to replay it later.
    pub fn remember(&mut self, command: &Line) {
        let keyword = command.keyword();
        let key = match keyword.as_str() {
            "OPTION" => {
                let argument = String::from_utf8_lossy(command.argument());
                let name = argument.split(['=', ' ']).next().unwrap_or("").to_owned();
                format!("OPTION {name}")
            }
            // SETREPEAT and SETERROR only apply to the next prompt.
            "SETREPEAT" | "SETERROR" => return,
            word if word.starts_with("SET") => keyword,
            _ => return,
        };
        let copy = Line::from_bytes(command.bytes());
        match self
            .settings
            .iter_mut()
            .find(|(existing, _)| *existing == key)
        {
            Some(slot) => slot.1 = copy,
            None => self.settings.push((key, copy)),
        }
    }

    pub fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        let Some((_, input)) = self.running.as_mut() else {
            return Err(io::Error::other("the fallback is not running"));
        };
        input.write_all(bytes)?;
        input.flush()
    }

    /// Reads a whole reply of the current fallback, up to `OK` or `ERR`.
    /// Only for replies nobody else needs to see (greeting, replays).
    pub fn read_reply(&mut self, events: &Receiver<Event>) -> io::Result<Vec<Line>> {
        let mut lines = Vec::new();
        loop {
            match events.recv() {
                Ok(Event::Line(generation, line)) if generation == self.generation => {
                    let done = classify(&line) == Reply::Final;
                    lines.push(line);
                    if done {
                        return Ok(lines);
                    }
                }
                Ok(Event::Closed(generation)) if generation == self.generation => {
                    self.running = None;
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }
                Ok(_) => {}
                Err(_) => return Err(io::ErrorKind::BrokenPipe.into()),
            }
        }
    }

    /// Closes the dialog at once; the next prompt starts a new fallback.
    pub fn kill(&mut self) {
        if let Some((mut child, _)) = self.running.take() {
            let _ = child.kill();
            let _ = child.wait();
            log::write(format_args!(
                "fallback generation {} closed",
                self.generation
            ));
        }
    }

    pub fn closed(&mut self) {
        self.running = None;
    }
}

impl Drop for Fallback {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Starts the fallback with the same arguments gpg-agent gave us. It dies with
/// this process, so a cancelled request never leaves a dialog behind.
fn spawn(program: &Path, arguments: &[OsString]) -> io::Result<Child> {
    use std::os::unix::process::CommandExt;
    let mut command = Command::new(program);
    command
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    // SAFETY: prctl is async-signal-safe and touches no memory of the parent.
    unsafe {
        command.pre_exec(|| {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command.spawn()
}
