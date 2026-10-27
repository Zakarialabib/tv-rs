//! Rule cooldown management to prevent repetitive triggering.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Manages cooldown intervals per rule ID.
#[derive(Debug, Default)]
pub struct CooldownTracker {
    last_triggered: HashMap<String, Instant>,
}

impl CooldownTracker {
    /// Creates a new `CooldownTracker`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Checks whether a rule is currently on cooldown.
    pub fn is_on_cooldown(&self, rule_id: &str, cooldown: Duration, now: Instant) -> bool {
        if cooldown.is_zero() {
            return false;
        }

        if let Some(last) = self.last_triggered.get(rule_id) {
            now.duration_since(*last) < cooldown
        } else {
            false
        }
    }

    /// Records that a rule was triggered at the specified instant.
    pub fn record_trigger(&mut self, rule_id: impl Into<String>, now: Instant) {
        self.last_triggered.insert(rule_id.into(), now);
    }

    /// Clears cooldown status for a rule.
    pub fn reset(&mut self, rule_id: &str) {
        self.last_triggered.remove(rule_id);
    }
}
