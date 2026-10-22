use super::PacketWarningFilter;
use std::io::{self, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;

#[test]
fn default_packet_warning_filter_hides_packet_addresses_and_keeps_app_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let output = Arc::new(Mutex::new(Vec::new()));
    let counts = Arc::new(std::array::from_fn::<_, 5, _>(|_| AtomicU64::new(0)));
    let filter = PacketWarningFilter {
        counts: Arc::clone(&counts),
        raw: false,
    };
    let subscriber = tracing_subscriber::registry()
        .with(EnvFilter::new("info"))
        .with(
            tracing_subscriber::fmt::layer()
                .without_time()
                .with_ansi(false)
                .with_writer(CaptureMakeWriter(Arc::clone(&output)))
                .with_filter(filter),
        );

    tracing::subscriber::with_default(subscriber, || -> Result<(), Box<dyn std::error::Error>> {
        emit_upstream_log_warning(
            "rtc_ice::agent",
            "discard success message for no such remote 203.0.113.81",
        )?;
        emit_upstream_log_warning(
            "rtc::peer_connection::handler",
            "unhandled STUN packet from 198.51.100.91",
        )?;
        emit_upstream_log_warning(
            "rtc_mdns::proto",
            "mDNS answers with different local ip 192.168.14.9 and 192.168.14.10",
        )?;
        emit_upstream_log_warning(
            "rtc_mdns::proto",
            "mDNS answers without AResource for 10.42.0.7",
        )?;
        emit_upstream_log_warning(
            "webrtc::peer_connection::transport::stun_gatherer",
            "STUN binding request from 203.0.113.82 timed out",
        )?;

        tracing::error!(
            target: "kryptopass::devices",
            "authenticated session closed after trust revocation"
        );
        tracing::warn!(
            target: "kryptopass::network_policy",
            "new network policy warning remains visible"
        );
        Ok(())
    })?;

    let output_bytes = output
        .lock()
        .map_err(|_| io::Error::other("captured output mutex poisoned"))?
        .clone();
    let output = String::from_utf8(output_bytes)?;
    for address in [
        "203.0.113.81",
        "198.51.100.91",
        "192.168.14.9",
        "192.168.14.10",
        "10.42.0.7",
        "203.0.113.82",
    ] {
        assert!(
            !output.contains(address),
            "known packet address leaked into normal logging: {address}"
        );
    }
    assert!(output.contains("authenticated session closed after trust revocation"));
    assert!(output.contains("new network policy warning remains visible"));
    assert_eq!(
        counts
            .iter()
            .map(|counter| counter.load(Ordering::Relaxed))
            .collect::<Vec<_>>(),
        vec![1, 1, 1, 1, 1],
        "each recognized packet warning increments only its safe counter"
    );
    Ok(())
}

fn emit_upstream_log_warning(target: &'static str, message: &'static str) -> Result<(), io::Error> {
    let args = format_args!("{message}");
    let record = tracing_log::log::Record::builder()
        .level(tracing_log::log::Level::Warn)
        .target(target)
        .args(args)
        .build();
    tracing_log::format_trace(&record)
}

#[derive(Clone)]
struct CaptureMakeWriter(Arc<Mutex<Vec<u8>>>);

impl<'a> MakeWriter<'a> for CaptureMakeWriter {
    type Writer = CaptureWriter;

    fn make_writer(&'a self) -> Self::Writer {
        CaptureWriter(Arc::clone(&self.0))
    }
}

struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

impl Write for CaptureWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("captured output mutex poisoned"))?
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
