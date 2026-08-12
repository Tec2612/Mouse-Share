use ms_protocol::SessionId;

/// Guards against replayed/stale events: rejects anything tagged with a
/// session id other than the current one (a straggling message from a
/// connection that was superseded by a reconnect racing it), and within a
/// session, rejects sequence numbers that don't advance (defense in depth
/// against a bug or a malicious peer — TCP's own ordering guarantee
/// already rules this out for a well-behaved peer on one connection, so
/// this should never actually trigger in normal operation).
pub struct SequenceGuard {
    active_session: Option<SessionId>,
    last_seq: u32,
}

impl SequenceGuard {
    pub fn new() -> Self {
        Self { active_session: None, last_seq: 0 }
    }

    /// Called once a `Hello`/`HelloAck` handshake completes on a new
    /// connection, establishing which session id is now authoritative.
    pub fn start_session(&mut self, session: SessionId) {
        self.active_session = Some(session);
        self.last_seq = 0;
    }

    /// Returns whether `(session, seq)` should be accepted and applied.
    /// Sequence 0 is treated as "unsequenced" (used by message kinds that
    /// don't need dedup, e.g. one-shot control messages) and always
    /// passes the per-seq check, since not every `Message` variant that
    /// carries a session needs to carry a meaningful sequence number.
    pub fn accept(&mut self, session: SessionId, seq: u32) -> bool {
        if self.active_session != Some(session) {
            return false;
        }
        if seq == 0 {
            return true;
        }
        // Wrapping-aware "did seq advance": treats the u32 space as a
        // circular counter so a legitimate wraparound after ~4 billion
        // events isn't mistaken for a replay.
        let advanced = seq.wrapping_sub(self.last_seq) as i32 > 0;
        if advanced {
            self.last_seq = seq;
        }
        advanced
    }
}

impl Default for SequenceGuard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_from_a_superseded_session_are_rejected() {
        let mut guard = SequenceGuard::new();
        let session_a = SessionId::new_v4();
        let session_b = SessionId::new_v4();
        guard.start_session(session_a);
        assert!(guard.accept(session_a, 1));

        guard.start_session(session_b);
        assert!(!guard.accept(session_a, 2), "a straggler from the old session must not be applied");
        assert!(guard.accept(session_b, 1));
    }

    #[test]
    fn non_advancing_sequence_numbers_are_rejected_within_a_session() {
        let mut guard = SequenceGuard::new();
        let session = SessionId::new_v4();
        guard.start_session(session);
        assert!(guard.accept(session, 5));
        assert!(!guard.accept(session, 5), "duplicate seq must be rejected");
        assert!(!guard.accept(session, 3), "out-of-order/older seq must be rejected");
        assert!(guard.accept(session, 6));
    }

    #[test]
    fn unsequenced_zero_always_passes_within_the_active_session() {
        let mut guard = SequenceGuard::new();
        let session = SessionId::new_v4();
        guard.start_session(session);
        assert!(guard.accept(session, 0));
        assert!(guard.accept(session, 0));
    }

    #[test]
    fn wraparound_is_treated_as_forward_progress() {
        // Directly place the guard at the state it would be in after
        // ~4 billion legitimate advances (last_seq == u32::MAX), rather
        // than trying to actually drive it there one accept() at a time.
        // Fields are private but reachable here since `tests` is a child
        // module of `dedup`.
        let session = SessionId::new_v4();
        let mut guard = SequenceGuard { active_session: Some(session), last_seq: u32::MAX };
        // seq 0 is the reserved "unsequenced" value, so the wrap lands on 1.
        assert!(guard.accept(session, 1), "wrapping from u32::MAX forward must count as progress");
        assert!(!guard.accept(session, u32::MAX), "must not accept a step back after the wrap");
    }
}
