//! Small helpers for running blocking work away from the Qt event loop.

use cxx_qt::{CxxQtThread, Threading};
use std::{pin::Pin, thread};

/// Runs blocking work on a Rust thread and publishes its result on the QObject's Qt thread.
pub fn run_blocking<Q, R, Work, Publish>(target: CxxQtThread<Q>, work: Work, publish: Publish)
where
    Q: Threading + 'static,
    R: Send + 'static,
    Work: FnOnce() -> R + Send + 'static,
    Publish: FnOnce(Pin<&mut Q>, R) + Send + 'static,
{
    thread::spawn(move || {
        let result = work();

        if let Err(error) = target.queue(move |object| {
            publish(object, result);
        }) {
            tracing::warn!(?error, "No se pudo publicar el resultado en el hilo de Qt");
        }
    });
}
