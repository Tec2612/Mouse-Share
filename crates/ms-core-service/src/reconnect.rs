use std::time::Duration;

/// Exponential backoff with a cap, for reconnect attempts after a
/// connection drops. Deliberately excludes random jitter — with typically
/// one or two peers per device rather than thousands of clients hammering
/// one server, the thundering-herd problem jitter defends against doesn't
/// apply here, and deterministic timing makes the reconnect behavior easy
/// to reason about and test.
pub struct ReconnectBackoff {
    initial: Duration,
    max: Duration,
    multiplier: u32,
    current: Duration,
    attempt: u32,
}

impl ReconnectBackoff {
    pub fn new(initial: Duration, max: Duration) -> Self {
        Self { initial, max, multiplier: 2, current: initial, attempt: 0 }
    }

    /// LAN-appropriate defaults: start retrying almost immediately (most
    /// drops on a local network are transient — a Wi-Fi roam, a brief
    /// association hiccup), but cap the interval so a genuinely offline
    /// peer doesn't get hammered indefinitely.
    pub fn with_defaults() -> Self {
        Self::new(Duration::from_millis(500), Duration::from_secs(30))
    }

    /// Returns the delay to wait before the next attempt, and advances
    /// internal state for the attempt after that.
    pub fn next_delay(&mut self) -> Duration {
        let delay = self.current;
        self.attempt += 1;
        self.current = (self.current * self.multiplier).min(self.max);
        delay
    }

    pub fn attempt_count(&self) -> u32 {
        self.attempt
    }

    /// Resets to the initial delay — called once a connection attempt
    /// actually succeeds, so a later, unrelated drop starts backing off
    /// from scratch rather than inheriting a long delay from a previous
    /// outage.
    pub fn reset(&mut self) {
        self.current = self.initial;
        self.attempt = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_doubles_each_attempt_up_to_the_cap() {
        let mut b = ReconnectBackoff::new(Duration::from_millis(100), Duration::from_secs(1));
        assert_eq!(b.next_delay(), Duration::from_millis(100));
        assert_eq!(b.next_delay(), Duration::from_millis(200));
        assert_eq!(b.next_delay(), Duration::from_millis(400));
        assert_eq!(b.next_delay(), Duration::from_millis(800));
        assert_eq!(b.next_delay(), Duration::from_secs(1), "must clamp to max, not overshoot to 1.6s");
        assert_eq!(b.next_delay(), Duration::from_secs(1), "stays clamped on subsequent attempts");
    }

    #[test]
    fn reset_returns_to_the_initial_delay() {
        let mut b = ReconnectBackoff::new(Duration::from_millis(100), Duration::from_secs(1));
        b.next_delay();
        b.next_delay();
        b.reset();
        assert_eq!(b.next_delay(), Duration::from_millis(100));
        assert_eq!(b.attempt_count(), 1);
    }

    #[test]
    fn attempt_count_tracks_calls_to_next_delay() {
        let mut b = ReconnectBackoff::with_defaults();
        assert_eq!(b.attempt_count(), 0);
        b.next_delay();
        b.next_delay();
        assert_eq!(b.attempt_count(), 2);
    }
}
