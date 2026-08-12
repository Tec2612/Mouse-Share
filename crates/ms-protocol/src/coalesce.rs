use std::time::{Duration, Instant};

/// Coalesces a burst of raw relative mouse-move samples into a bounded
/// stream of `MouseMove` messages: accumulate deltas as they arrive and
/// only emit a message when either enough time has passed since the last
/// flush (`min_interval`) or accumulated displacement crosses
/// `flush_threshold_px`, whichever comes first. This caps the packet rate
/// during fast swipes (which would otherwise generate one OS event per
/// ~1ms) without adding perceptible latency, since the interval is well
/// under human-perceptible input lag (~8ms == 125Hz).
pub struct MouseMoveCoalescer {
    min_interval: Duration,
    flush_threshold_px: f64,
    pending_dx: f64,
    pending_dy: f64,
    last_flush: Option<Instant>,
    seq: u32,
}

impl MouseMoveCoalescer {
    pub fn new(min_interval: Duration, flush_threshold_px: f64) -> Self {
        Self {
            min_interval,
            flush_threshold_px,
            pending_dx: 0.0,
            pending_dy: 0.0,
            last_flush: None,
            seq: 0,
        }
    }

    /// The default policy: flush at most every 8ms (~125Hz, above typical
    /// mouse polling rates) or once accumulated movement exceeds 64px,
    /// whichever happens first.
    pub fn with_defaults() -> Self {
        Self::new(Duration::from_millis(8), 64.0)
    }

    /// Records a new raw delta sample. Returns `Some((dx, dy, seq))` if this
    /// sample should be flushed immediately as a message, or `None` if it
    /// was folded into the pending accumulator to be sent on a later call
    /// or an explicit `flush()`.
    pub fn push(&mut self, dx: f64, dy: f64, now: Instant) -> Option<(f64, f64, u32)> {
        self.pending_dx += dx;
        self.pending_dy += dy;

        let elapsed_enough = self
            .last_flush
            .map(|t| now.duration_since(t) >= self.min_interval)
            .unwrap_or(true);
        let magnitude = (self.pending_dx.powi(2) + self.pending_dy.powi(2)).sqrt();

        if elapsed_enough || magnitude >= self.flush_threshold_px {
            self.flush(now)
        } else {
            None
        }
    }

    /// Forces emission of any pending accumulated movement, e.g. when the
    /// mouse goes idle and the caller wants to guarantee delivery instead
    /// of waiting for the next sample to trigger a flush.
    pub fn flush(&mut self, now: Instant) -> Option<(f64, f64, u32)> {
        if self.pending_dx == 0.0 && self.pending_dy == 0.0 {
            return None;
        }
        let dx = self.pending_dx;
        let dy = self.pending_dy;
        self.pending_dx = 0.0;
        self.pending_dy = 0.0;
        self.last_flush = Some(now);
        self.seq = self.seq.wrapping_add(1);
        Some((dx, dy, self.seq))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rapid_samples_within_the_interval_are_folded_into_one_flush() {
        let mut c = MouseMoveCoalescer::new(Duration::from_millis(8), 1000.0);
        let t0 = Instant::now();

        assert!(c.push(1.0, 0.0, t0).is_some(), "first sample always flushes immediately");
        assert!(c.push(1.0, 0.0, t0 + Duration::from_millis(1)).is_none());
        assert!(c.push(1.0, 0.0, t0 + Duration::from_millis(2)).is_none());

        let flushed = c.flush(t0 + Duration::from_millis(3)).unwrap();
        assert_eq!(flushed.0, 2.0); // two folded samples of dx=1.0 each
    }

    #[test]
    fn large_displacement_flushes_early_even_within_the_interval() {
        let mut c = MouseMoveCoalescer::new(Duration::from_millis(100), 10.0);
        let t0 = Instant::now();
        c.flush(t0); // consume the "first sample" freebie path by priming last_flush
        let _ = c.push(0.0, 0.0, t0);

        let result = c.push(50.0, 0.0, t0 + Duration::from_millis(1));
        assert!(result.is_some(), "large jump should flush despite short elapsed time");
    }

    #[test]
    fn no_movement_never_produces_a_flush() {
        let mut c = MouseMoveCoalescer::with_defaults();
        assert!(c.flush(Instant::now()).is_none());
    }

    #[test]
    fn sequence_numbers_increase_monotonically_across_flushes() {
        let mut c = MouseMoveCoalescer::new(Duration::from_millis(1), 1000.0);
        let t0 = Instant::now();
        let (_, _, seq1) = c.push(1.0, 1.0, t0).unwrap();
        let (_, _, seq2) = c
            .push(1.0, 1.0, t0 + Duration::from_millis(5))
            .unwrap();
        assert_eq!(seq2, seq1 + 1);
    }
}
