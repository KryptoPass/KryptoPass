//! Summarizes repeated packet warnings without publishing candidate IP addresses.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tracing::{Event, Subscriber, field::Visit};
use tracing_log::NormalizeEvent;
use tracing_subscriber::{layer::Context, layer::Filter};

const GROUPS: [&str; 5] = [
    "ice_unknown_remote",
    "ice_unhandled_stun",
    "mdns_address_mismatch",
    "mdns_source_fallback",
    "stun_timeout",
];

/// Only these recurring dependency diagnostics are aggregated. Other warnings
/// and all application session failures retain their normal visibility.
pub(crate) fn packet_warning_group(target: &str, message: &str) -> Option<usize> {
    match target {
        "rtc_ice::agent"
            if message.contains("discard success message")
                && message.contains("no such remote") =>
        {
            Some(0)
        }
        "rtc::peer_connection::handler" if message.contains("unhandled STUN packet") => Some(1),
        "rtc_mdns::proto" if message.contains("mDNS answers with different local ip") => Some(2),
        "rtc_mdns::proto" if message.contains("mDNS answers without AResource") => Some(3),
        "webrtc::peer_connection::transport::stun_gatherer"
            if message.contains("STUN binding request from") && message.contains("timed out") =>
        {
            Some(4)
        }
        _ => None,
    }
}

/// Raw packet diagnostics are opt-in; the default retains counts without IPs.
pub(crate) struct PacketWarningFilter {
    counts: Arc<[AtomicU64; 5]>,
    raw: bool,
}

impl PacketWarningFilter {
    pub(crate) fn new() -> Self {
        let counts = Arc::new(std::array::from_fn(|_| AtomicU64::new(0)));
        let raw = std::env::var_os("KP_TRANSPORT_PACKET_LOGS").is_some_and(|value| value == "1");
        if !raw {
            let counters = Arc::clone(&counts);
            // This reporter owns counters only, never sockets, addresses or payloads.
            // It exits with the application process and cannot keep a session alive.
            let _ = std::thread::Builder::new()
                .name("kp-packet-diagnostics".to_owned())
                .spawn(move || {
                    loop {
                        std::thread::sleep(Duration::from_secs(10));
                        for (index, counter) in counters.iter().enumerate() {
                            let count = counter.swap(0, Ordering::Relaxed);
                            if count > 0 {
                                tracing::warn!(
                                    target: "kryptopass::transport_diagnostics",
                                    category = GROUPS[index],
                                    count,
                                    window_seconds = 10,
                                    "Repeated transport diagnostics (addresses omitted)"
                                );
                            }
                        }
                    }
                });
        }
        Self { counts, raw }
    }
}

impl<S: Subscriber> Filter<S> for PacketWarningFilter {
    fn enabled(&self, _metadata: &tracing::Metadata<'_>, _context: &Context<'_, S>) -> bool {
        true
    }

    fn event_enabled(&self, event: &Event<'_>, _context: &Context<'_, S>) -> bool {
        if self.raw || *event.metadata().level() != tracing::Level::WARN {
            return true;
        }
        let normalized = event.normalized_metadata();
        let mut visitor = WarningVisitor {
            target: normalized
                .as_ref()
                .map_or_else(|| event.metadata().target(), tracing::Metadata::target),
            group: None,
        };
        event.record(&mut visitor);
        if let Some(group) = visitor.group {
            self.counts[group].fetch_add(1, Ordering::Relaxed);
            false
        } else {
            true
        }
    }
}

struct WarningVisitor<'a> {
    target: &'a str,
    group: Option<usize>,
}

impl Visit for WarningVisitor<'_> {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.group = packet_warning_group(self.target, value);
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.group = packet_warning_group(self.target, &format!("{value:?}"));
        }
    }
}

#[cfg(test)]
#[path = "transport_diagnostics_tests.rs"]
mod tests;
