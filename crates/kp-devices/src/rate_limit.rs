//! Sliding-window limits on what a peer may send per second.

use super::*;

#[derive(Default)]
pub(crate) struct ControlRateLimiter {
    recent: VecDeque<Instant>,
}

#[derive(Default)]
pub(crate) struct DataRateLimiter {
    recent: VecDeque<Instant>,
}

impl DataRateLimiter {
    pub(crate) fn accept(&mut self, now: Instant) -> bool {
        self.accept_limit(now, MAX_DATA_MESSAGES_PER_SECOND)
    }

    pub(crate) fn accept_limit(&mut self, now: Instant, maximum: usize) -> bool {
        while self
            .recent
            .front()
            .is_some_and(|at| now.duration_since(*at) >= Duration::from_secs(1))
        {
            let _ = self.recent.pop_front();
        }
        if self.recent.len() >= maximum {
            return false;
        }
        self.recent.push_back(now);
        true
    }
}

impl ControlRateLimiter {
    pub(crate) fn accept(&mut self, now: Instant) -> bool {
        while self
            .recent
            .front()
            .is_some_and(|at| now.duration_since(*at) >= Duration::from_secs(1))
        {
            let _ = self.recent.pop_front();
        }
        if self.recent.len() >= MAX_CONTROL_MESSAGES_PER_SECOND {
            return false;
        }
        self.recent.push_back(now);
        true
    }
}
