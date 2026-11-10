//! `kryptopass-pinentry`: the pinentry gpg-agent runs when KryptoPass handles
//! approvals. A running KryptoPass may answer `CONFIRM` and `GETPIN`; every
//! other command, and every prompt it does not answer, goes to a real pinentry.
//! The flow is logged without secrets. See `docs/aprobacion-desde-telefono.md`.

mod assuan;
mod fallback;
mod log;
mod proxy;
mod remote;

use std::io::BufWriter;
use std::process::ExitCode;

fn main() -> ExitCode {
    log::open();
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    log::write(format_args!("started with arguments {arguments:?}"));

    let Some(program) = fallback::find() else {
        log::write("no fallback pinentry found; refusing to answer");
        eprintln!("kryptopass-pinentry: no fallback pinentry found");
        return ExitCode::FAILURE;
    };
    let (events_tx, events) = std::sync::mpsc::channel();
    let mut fallback = fallback::Fallback::new(program, arguments, events_tx);
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let result = proxy::run(
        &mut stdin.lock(),
        &mut BufWriter::new(stdout.lock()),
        &mut fallback,
        &events,
    );
    drop(fallback);
    log::write(format_args!("finished: {result:?}"));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
