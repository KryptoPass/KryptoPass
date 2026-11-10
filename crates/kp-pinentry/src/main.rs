//! `kryptopass-pinentry`: the pinentry gpg-agent runs when KryptoPass handles
//! approvals. A running KryptoPass may answer `CONFIRM` and `GETPIN`; every
//! other command, and every prompt it does not answer, goes to a real pinentry.
//! The flow is logged without secrets. See `docs/aprobacion-desde-telefono.md`.

mod assuan;
mod fallback;
mod log;
mod proxy;
mod remote;

use std::io::{BufReader, BufWriter};
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
    let mut child = match fallback::spawn(&program, &arguments) {
        Ok(child) => child,
        Err(error) => {
            log::write(format_args!(
                "could not start {}: {error}",
                program.display()
            ));
            eprintln!("kryptopass-pinentry: could not start the fallback pinentry");
            return ExitCode::FAILURE;
        }
    };
    log::write(format_args!(
        "fallback {} running as pid {}",
        program.display(),
        child.id()
    ));

    let (Some(child_in), Some(child_out)) = (child.stdin.take(), child.stdout.take()) else {
        log::write("the fallback pinentry has no pipes");
        let _ = child.kill();
        return ExitCode::FAILURE;
    };
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let result = proxy::run(
        &mut stdin.lock(),
        &mut BufWriter::new(stdout.lock()),
        &mut BufReader::new(child_out),
        &mut BufWriter::new(child_in),
    );

    let _ = child.kill();
    let status = child.wait();
    log::write(format_args!("finished: {result:?}; fallback {status:?}"));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
