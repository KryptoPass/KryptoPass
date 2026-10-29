//! Wall-clock time as stored timestamps.

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds since the Unix epoch; 0 if the clock is set before it.
pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}
