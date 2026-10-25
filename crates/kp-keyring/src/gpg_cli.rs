//! The few operations GPGME cannot do, run through the `gpg` binary with a
//! deadline and bounded output.

use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use kryptopass_secure_memory::SensitiveBytes;
use tracing::{debug, instrument};

use crate::{
    backend::MAX_KEY_BLOB_BYTES,
    error::{CommandError, Result},
    files, revocation,
    types::Fingerprint,
};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(120);

/// The `gpg` binary behind GPGME, pointed at the same home.
pub(crate) struct Gpg {
    program: PathBuf,
    home: Option<PathBuf>,
}

impl Gpg {
    pub fn new(program: Option<String>, home: Option<&Path>) -> Self {
        Self {
            program: program.map_or_else(|| PathBuf::from("gpg"), PathBuf::from),
            home: home.map(Path::to_owned),
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        if let Some(home) = &self.home {
            command.arg("--homedir").arg(home);
        }
        command.arg("--no-tty");
        command
    }

    /// Runs a batch command whose output is discarded.
    #[instrument(
        level = "debug",
        skip(self, arguments),
        fields(program = %self.program.display(), arguments = ?arguments)
    )]
    pub fn run_batch(&self, arguments: &[&str]) -> Result<(), CommandError> {
        let mut child = self
            .command()
            .arg("--batch")
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(CommandError::Spawn)?;
        let stderr = child
            .stderr
            .take()
            .ok_or(CommandError::Protocol("no stderr pipe"))?;
        let drain = thread::spawn(move || discard(stderr));
        let status = wait_with_timeout(&mut child, COMMAND_TIMEOUT);
        let drained = drain.join();
        let status = status?;
        let success = status.success();
        drained
            .map_err(|_| CommandError::Protocol("stderr reader panicked"))?
            .map_err(CommandError::Io)?;
        let result = exit_ok(status);
        debug!(success, "Keyring: gpg batch command finished");
        result
    }

    /// Asks gpg for a revocation certificate, answering its fixed dialogue
    /// with reason "no reason specified". Pinentry may prompt for the key.
    #[instrument(level = "debug", skip(self), fields(fingerprint = %fingerprint))]
    pub fn generate_revocation(&self, fingerprint: &Fingerprint) -> Result<SensitiveBytes> {
        let folder = files::private_temp_dir(".kryptopass-revocation-")?;
        let output = folder.path().join("revocation.asc");
        let mut child = self
            .command()
            .args([
                "--status-fd",
                "2",
                "--command-fd",
                "0",
                "--pinentry-mode",
                "ask",
            ])
            .arg("--output")
            .arg(&output)
            .arg("--gen-revoke")
            .arg(fingerprint.as_str())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(CommandError::Spawn)?;
        let commands = child
            .stdin
            .take()
            .ok_or(CommandError::Protocol("no stdin pipe"))?;
        let status = child
            .stderr
            .take()
            .ok_or(CommandError::Protocol("no stderr pipe"))?;
        let driver = thread::spawn(move || revocation::answer_gen_revoke(status, commands));

        let exit = wait_with_timeout(&mut child, COMMAND_TIMEOUT);
        let dialogue = driver.join();
        exit_ok(exit?)?;
        dialogue.map_err(|_| CommandError::Protocol("dialogue driver panicked"))??;

        let raw = files::read_bounded(&output, MAX_KEY_BLOB_BYTES as u64)?;
        let certificate = revocation::normalize_certificate(&raw)?;
        debug!(
            bytes = certificate.len(),
            "Keyring: generated revocation certificate"
        );
        Ok(certificate)
    }
}

/// Reads and drops output in fixed chunks, so a chatty helper can neither
/// block on a full pipe nor grow memory.
fn discard(mut stream: impl Read) -> io::Result<()> {
    let mut buffer = [0u8; 4096];
    while stream.read(&mut buffer)? != 0 {}
    Ok(())
}

fn wait_with_timeout(child: &mut Child, timeout: Duration) -> Result<ExitStatus, CommandError> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            outcome => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(match outcome {
                    Err(error) => CommandError::Io(error),
                    _ => CommandError::Timeout,
                });
            }
        }
    }
}

fn exit_ok(status: ExitStatus) -> Result<(), CommandError> {
    if status.success() {
        Ok(())
    } else {
        Err(CommandError::Exit(status))
    }
}
