//! Private stdin/stdout control harness for cross-runtime integration tests.
//! Bootstrap credentials go only to the controlling pipe, never to tracing logs.

use kryptopass_pairing::LocalDecision;
use kryptopass_pairing_lab::{HostEvent, LabHost};
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    path::PathBuf,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    sync::{mpsc, oneshot, watch},
};

fn emit(value: Value) -> io::Result<()> {
    let mut output = io::stdout().lock();
    serde_json::to_writer(&mut output, &value)?;
    writeln!(output)?;
    output.flush()
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 || args[1] != "--profile" || !cfg!(debug_assertions) {
        eprintln!("This debug test harness requires --profile <fresh private directory>.");
        std::process::exit(2);
    }
    let host = LabHost::ephemeral_test_profile(&PathBuf::from(&args[2]))?;
    let local_id = host.public_identity().device_id.to_string();
    let mut input = BufReader::new(tokio::io::stdin());
    let mut line = String::new();
    let mut last_peer = None;
    let mut reconnect = false;
    loop {
        let (events, mut event_rx) = mpsc::channel(8);
        let (decision, decisions) = oneshot::channel();
        let (cancel, cancelled) = watch::channel(false);
        let mut decision = Some(decision);
        let mut awaiting = false;
        let operation = host.run_once(
            if reconnect { last_peer } else { None },
            events,
            decisions,
            cancelled,
        );
        tokio::pin!(operation);
        loop {
            tokio::select! {
                result = &mut operation => {
                    match result {
                        Ok(peer) => {
                            last_peer = Some(peer);
                            emit(json!({"event":"trusted", "device_id":peer.device_id.to_string(), "local_device_id":local_id, "mode":if reconnect {"reconnect"} else {"pairing"}}))?;
                        }
                        Err(error) => emit(json!({"event":"error", "code":error.to_string()}))?,
                    }
                    break;
                }
                event = event_rx.recv() => match event {
                    Some(HostEvent::Bootstrap(code)) => emit(json!({"event":"bootstrap", "code":code}))?,
                    Some(HostEvent::Sas(sas)) => {
                        awaiting = true;
                        emit(json!({"event":"sas", "indices":sas.indices(), "emojis":sas.emojis.iter().map(|entry|entry.emoji).collect::<Vec<_>>(), "names":sas.names()}))?;
                    }
                    None => {},
                },
                read = input.read_line(&mut line) => {
                    if read? == 0 { return Ok(()); }
                    let parsed: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
                    line.clear();
                    let name = parsed.get("command").and_then(Value::as_str).unwrap_or("");
                    match name {
                        "quit" => return Ok(()),
                        "cancel" => { let _ = cancel.send(true); },
                        "approve" | "reject" if awaiting => {
                            awaiting = false;
                            if let Some(decision) = decision.take() {
                                let _ = decision.send(if name == "approve" { LocalDecision::Approve } else { LocalDecision::Reject });
                            }
                        }
                        _ => emit(json!({"event":"control_error", "code":"unexpected_command"}))?,
                    }
                }
            }
        }
        loop {
            line.clear();
            if input.read_line(&mut line).await? == 0 {
                return Ok(());
            }
            let parsed: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
            match parsed.get("command").and_then(Value::as_str) {
                Some("quit") => return Ok(()),
                Some("reconnect") if last_peer.is_some() => {
                    reconnect = true;
                    break;
                }
                Some("pair") => {
                    reconnect = false;
                    break;
                }
                _ => emit(json!({"event":"control_error", "code":"unexpected_command"}))?,
            }
        }
    }
}
