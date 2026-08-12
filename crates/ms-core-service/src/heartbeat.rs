use std::time::{Duration, Instant};

/// Tracks connection liveness from received heartbeats. A connection is
/// considered dead after missing three consecutive expected heartbeats —
/// long enough to absorb a single dropped packet or a brief scheduling
/// hiccup without a false-positive disconnect, short enough that a
/// genuinely dead peer is detected quickly (within `3 * interval`) so
/// `EdgeStateMachine::handle(Event::ConnectionLost)` can release control
/// back to the user promptly.
pub struct HeartbeatMonitor {
    interval: Duration,
    last_seen: Instant,
}

impl HeartbeatMonitor {
    pub fn new(interval: Duration, now: Instant) -> Self {
        Self { interval, last_seen: now }
    }

    pub fn on_heartbeat_received(&mut self, now: Instant) {
        self.last_seen = now;
    }

    pub fn is_alive(&self, now: Instant) -> bool {
        now.duration_since(self.last_seen) < self.interval * 3
    }

    pub fn time_since_last_heartbeat(&self, now: Instant) -> Duration {
        now.duration_since(self.last_seen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stays_alive_within_the_grace_window() {
        let t0 = Instant::now();
        let monitor = HeartbeatMonitor::new(Duration::from_secs(1), t0);
        assert!(monitor.is_alive(t0 + Duration::from_millis(2900)));
    }

    #[test]
    fn declared_dead_after_missing_three_intervals() {
        let t0 = Instant::now();
        let monitor = HeartbeatMonitor::new(Duration::from_secs(1), t0);
        assert!(!monitor.is_alive(t0 + Duration::from_millis(3100)));
    }

    #[test]
    fn a_received_heartbeat_resets_the_clock() {
        let t0 = Instant::now();
        let mut monitor = HeartbeatMonitor::new(Duration::from_secs(1), t0);
        let t1 = t0 + Duration::from_millis(2900);
        monitor.on_heartbeat_received(t1);
        assert!(monitor.is_alive(t1 + Duration::from_millis(2900)));
    }
}
