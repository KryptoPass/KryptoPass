//! Bounded GnuPG command interaction and key-edit protocols.

use std::{
    io::{self, BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, Command, ExitStatus, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use gpgme::{Context, InteractionStatus, Interactor, Key};
use kryptopass_secure_memory::SensitiveBytes;

use super::files::{self, MAX_KEY_BLOB_BYTES};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_STATUS_LINE_BYTES: usize = 4096;
const MAX_REVOCATION_PROMPTS: usize = 4;
const MAX_SUBKEY_EDIT_RESPONSES: usize = 8;

pub(super) fn run_gpg_quiet(arguments: &[&str]) -> Result<(), String> {
    let mut child = Command::new("gpg")
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            crate::i18n::format("No se pudo iniciar GnuPG: %1", &[error.to_string()])
        })?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "No se pudo capturar la salida de GnuPG.".to_owned())?;
    let drain = thread::spawn(move || drain_bounded_stderr(stderr));
    let status = match wait_with_timeout(&mut child, COMMAND_TIMEOUT) {
        Ok(status) => status,
        Err(error) => {
            let _ = drain.join();
            return Err(error);
        }
    };
    drain
        .join()
        .map_err(|_| "No se pudo cerrar la salida acotada de GnuPG.".to_owned())?
        .map_err(|error| {
            crate::i18n::format(
                "No se pudo leer la salida de GnuPG: %1",
                &[error.to_string()],
            )
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("GnuPG terminó con estado {status}."))
    }
}

fn drain_bounded_stderr(mut stderr: impl Read) -> io::Result<()> {
    let mut buffer = [0u8; 4096];
    // Read and discard in fixed-size chunks. This drains the pipe without
    // allowing a network helper's diagnostics to grow without bound.
    loop {
        if stderr.read(&mut buffer)? == 0 {
            return Ok(());
        }
    }
}

fn wait_with_timeout(child: &mut Child, timeout: Duration) -> Result<ExitStatus, String> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("GnuPG superó el tiempo máximo de espera.".into());
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(crate::i18n::format(
                    "No se pudo esperar a GnuPG: %1",
                    &[error.to_string()],
                ));
            }
        }
    }
}

pub(super) fn generate_revocation_certificate(fingerprint: &str) -> Result<SensitiveBytes, String> {
    let temporary =
        files::create_secure_temp_directory(std::env::temp_dir(), "kryptopass-revocation")?;
    let output_path = temporary.path().join("revocation.asc");
    let mut child = Command::new("gpg")
        .args([
            "--no-tty",
            "--status-fd",
            "2",
            "--command-fd",
            "0",
            "--pinentry-mode",
            "ask",
            "--output",
        ])
        .arg(&output_path)
        .arg("--gen-revoke")
        .arg(fingerprint)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            crate::i18n::format(
                "No se pudo iniciar GnuPG para preparar la revocación: %1",
                &[error.to_string()],
            )
        })?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "No se pudo abrir el canal de comandos de GnuPG.".to_owned())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "No se pudo abrir el canal de estado de GnuPG.".to_owned())?;
    let driver = thread::spawn(move || drive_revocation_protocol(stderr, stdin));

    let status = match wait_with_timeout(&mut child, COMMAND_TIMEOUT) {
        Ok(status) => status,
        Err(error) => {
            let _ = driver.join();
            return Err(error);
        }
    };
    let protocol = driver
        .join()
        .map_err(|_| "La interacción de revocación de GnuPG terminó inesperadamente.".to_owned())?;
    if !status.success() {
        return Err("GnuPG no pudo generar el certificado de revocación solicitado.".into());
    }
    protocol?;

    let raw = files::read_bounded_file(
        &output_path,
        MAX_KEY_BLOB_BYTES as u64,
        "El certificado de revocación generado",
    )?;
    normalize_revocation_certificate(raw)
}

fn drive_revocation_protocol(stderr: impl Read, mut command: impl Write) -> Result<(), String> {
    let mut reader = BufReader::new(stderr);
    let mut state = 0usize;
    let mut responses = 0usize;
    while let Some(line) = read_status_line(&mut reader)? {
        let Some((keyword, arguments)) = parse_status_line(&line)? else {
            continue;
        };
        if !matches!(keyword, "GET_LINE" | "GET_BOOL") {
            continue;
        }
        if responses >= MAX_REVOCATION_PROMPTS {
            return Err("GnuPG solicitó más respuestas de revocación de las admitidas.".into());
        }
        let answer = match (state, keyword, arguments) {
            (0, "GET_BOOL", "gen_revoke.okay") => "y",
            (1, "GET_LINE", "ask_revocation_reason.code") => "0",
            (2, "GET_LINE", "ask_revocation_reason.text") => "",
            (3, "GET_BOOL", "ask_revocation_reason.okay") => "y",
            _ => {
                return Err("GnuPG presentó una pregunta de revocación inesperada.".into());
            }
        };
        command
            .write_all(answer.as_bytes())
            .and_then(|()| command.write_all(b"\n"))
            .and_then(|()| command.flush())
            .map_err(|error| {
                crate::i18n::format("No se pudo responder a GnuPG: %1", &[error.to_string()])
            })?;
        state += 1;
        responses += 1;
    }
    if state != MAX_REVOCATION_PROMPTS || responses != MAX_REVOCATION_PROMPTS {
        return Err("GnuPG cerró el canal antes de completar la preparación de revocación.".into());
    }
    Ok(())
}

fn read_status_line(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>, String> {
    let mut line = Vec::new();
    let mut oversized = false;
    loop {
        let available = reader.fill_buf().map_err(|error| {
            crate::i18n::format(
                "No se pudo leer el estado de GnuPG: %1",
                &[error.to_string()],
            )
        })?;
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
        return Err("GnuPG emitió una línea de estado demasiado larga.".into());
    }
    if line.last() == Some(&b'\n') {
        line.pop();
    }
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    Ok(Some(line))
}

fn parse_status_line(line: &[u8]) -> Result<Option<(&str, &str)>, String> {
    let Some(payload) = line.strip_prefix(b"[GNUPG:] ") else {
        return Ok(None);
    };
    let payload = std::str::from_utf8(payload)
        .map_err(|_| "GnuPG emitió un estado que no es UTF-8.".to_owned())?;
    let Some((keyword, arguments)) = payload.split_once(' ') else {
        return Ok(Some((payload, "")));
    };
    Ok(Some((keyword, arguments)))
}

pub(super) fn interact_revoking_subkey(
    context: &mut Context,
    key: &Key,
    subkey_index: usize,
) -> Result<(), String> {
    let state = Arc::new(Mutex::new(SubkeyEditState {
        subkey_index,
        step: 0,
        responses: 0,
    }));
    let interactor = SubkeyRevocationInteractor {
        state: Arc::clone(&state),
    };
    context.interact(key, interactor, "").map_err(|error| {
        crate::i18n::format(
            "No se pudo completar la revocación de la subclave: %1",
            &[error.to_string()],
        )
    })?;
    let state = state
        .lock()
        .map_err(|_| "Se bloqueó la interacción de revocación de la subclave.".to_owned())?;
    if state.step < 7 {
        return Err("GnuPG terminó antes de guardar la revocación de la subclave.".into());
    }
    Ok(())
}

struct SubkeyEditState {
    subkey_index: usize,
    step: usize,
    responses: usize,
}

struct SubkeyRevocationInteractor {
    state: Arc<Mutex<SubkeyEditState>>,
}

impl Interactor for SubkeyRevocationInteractor {
    fn interact(
        &mut self,
        status: InteractionStatus<'_>,
        mut out: Option<&mut dyn Write>,
    ) -> Result<(), gpgme::Error> {
        let keyword = status.keyword().unwrap_or("");
        let arguments = status.args().unwrap_or("");
        let Some(writer) = out.as_mut() else {
            return if matches!(keyword, "GET_LINE" | "GET_BOOL") {
                Err(gpgme::Error::GENERAL)
            } else {
                Ok(())
            };
        };

        let mut state = self.state.lock().map_err(|_| gpgme::Error::GENERAL)?;
        if state.responses >= MAX_SUBKEY_EDIT_RESPONSES {
            return Err(gpgme::Error::GENERAL);
        }
        let answer = match (state.step, keyword, arguments) {
            (0, "GET_LINE", "keyedit.prompt") => format!("key {}", state.subkey_index),
            (1, "GET_LINE", "keyedit.prompt") => "revkey".to_owned(),
            (2, "GET_BOOL", "keyedit.revoke.subkey.okay") => "y".to_owned(),
            (3, "GET_LINE", "ask_revocation_reason.code") => "0".to_owned(),
            (4, "GET_LINE", "ask_revocation_reason.text") => String::new(),
            (5, "GET_BOOL", "ask_revocation_reason.okay") => "y".to_owned(),
            (6, "GET_LINE", "keyedit.prompt") => "save".to_owned(),
            (7, "GET_BOOL", "keyedit.save.okay") => "y".to_owned(),
            _ => return Err(gpgme::Error::GENERAL),
        };
        writer
            .write_all(answer.as_bytes())
            .and_then(|()| writer.write_all(b"\n"))
            .map_err(|_| gpgme::Error::GENERAL)?;
        state.step += 1;
        state.responses += 1;
        Ok(())
    }
}

pub(super) fn revocation_file_path(context: &Context, fingerprint: &str) -> Option<PathBuf> {
    let engine = context.engine_info();
    let home = engine.home_dir().ok()?;
    Some(
        PathBuf::from(home)
            .join("openpgp-revocs.d")
            .join(format!("{fingerprint}.rev")),
    )
}

pub(super) fn normalize_revocation_certificate(
    raw: SensitiveBytes,
) -> Result<SensitiveBytes, String> {
    const BEGIN: &[u8] = b"-----BEGIN PGP PUBLIC KEY BLOCK-----";
    const END: &[u8] = b"-----END PGP PUBLIC KEY BLOCK-----";

    let begins: Vec<usize> = raw
        .windows(BEGIN.len())
        .enumerate()
        .filter_map(|(index, window)| (window == BEGIN).then_some(index))
        .collect();
    let ends: Vec<usize> = raw
        .windows(END.len())
        .enumerate()
        .filter_map(|(index, window)| (window == END).then_some(index))
        .collect();
    if begins.len() != 1 || ends.len() != 1 || ends[0] <= begins[0] {
        return Err("El archivo debe contener un único certificado de revocación completo.".into());
    }

    let start = begins[0];
    let end = ends[0] + END.len();
    let block = &raw[start..end];
    validate_armored_public_key_block(block)?;
    Ok(SensitiveBytes::from(block))
}

fn validate_armored_public_key_block(block: &[u8]) -> Result<(), String> {
    let text = std::str::from_utf8(block)
        .map_err(|_| "El certificado de revocación no es texto ASCII válido.".to_owned())?;
    if !text.is_ascii() {
        return Err("El certificado de revocación debe estar en formato ASCII.".into());
    }
    let mut lines = text.lines();
    if lines.next() != Some("-----BEGIN PGP PUBLIC KEY BLOCK-----") {
        return Err("El certificado de revocación tiene un encabezado incorrecto.".into());
    }
    let mut in_body = false;
    let mut body_lines = 0usize;
    let mut checksum_seen = false;
    let mut footer_seen = false;
    for line in lines {
        if footer_seen {
            return Err("El certificado de revocación contiene datos tras su cierre.".into());
        }
        if !in_body {
            if line.is_empty() {
                in_body = true;
            } else if line.len() > 128 || !line.bytes().all(|byte| (0x20..=0x7e).contains(&byte)) {
                return Err("El certificado de revocación tiene una cabecera inválida.".into());
            }
            continue;
        }
        if line == "-----END PGP PUBLIC KEY BLOCK-----" {
            footer_seen = true;
            continue;
        }
        if let Some(checksum) = line.strip_prefix('=') {
            if checksum_seen || checksum.len() != 4 || !checksum.bytes().all(is_base64_character) {
                return Err(
                    "El certificado de revocación tiene una suma de comprobación inválida.".into(),
                );
            }
            checksum_seen = true;
            continue;
        }
        if checksum_seen
            || line.is_empty()
            || line.len() > 76
            || !line.bytes().all(is_base64_character)
        {
            return Err("El certificado de revocación contiene datos armados inválidos.".into());
        }
        body_lines += 1;
    }
    if !in_body || !footer_seen || body_lines == 0 {
        return Err("El certificado de revocación está incompleto.".into());
    }
    Ok(())
}

fn is_base64_character(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=')
}
