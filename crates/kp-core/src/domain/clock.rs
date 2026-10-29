//! Hybrid logical clocks: wall-clock time that never runs backwards across
//! versions that build on each other.

/// When a version was written: milliseconds since the Unix epoch, and a
/// counter that orders versions written within the same millisecond or by a
/// device whose clock is behind.
///
/// A version's clock is always later than its parents', so ordering by clock
/// respects history even when devices disagree on the time. Between versions
/// that diverged it is only a hint of which is more recent; deciding between
/// them is the person's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Hlc {
    pub millis: u64,
    pub counter: u32,
}

impl Hlc {
    /// The clock of a version written at `now_millis` on top of versions
    /// with the clocks `parents`.
    pub fn next(now_millis: u64, parents: impl IntoIterator<Item = Self>) -> Self {
        match parents.into_iter().max() {
            Some(latest) if latest.millis >= now_millis => Self {
                millis: latest.millis,
                counter: latest.counter.saturating_add(1),
            },
            _ => Self {
                millis: now_millis,
                counter: 0,
            },
        }
    }
}
