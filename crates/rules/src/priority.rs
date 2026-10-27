//! Rule priority levels and ordering for conflict resolution.

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// Priority level assigned to a rule.
/// Higher priority rules take precedence during conflict resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    /// Low priority background automation.
    Low,
    /// Normal operational priority (default).
    #[default]
    Normal,
    /// High priority rule override.
    High,
    /// Emergency / critical priority that overrides all standard rules.
    Emergency,
    /// Custom numeric priority level.
    Custom(u32),
}

impl Priority {
    /// Returns the integer score of the priority.
    pub fn score(&self) -> u32 {
        match self {
            Self::Low => 10,
            Self::Normal => 50,
            Self::High => 80,
            Self::Emergency => 100,
            Self::Custom(val) => *val,
        }
    }
}

impl PartialOrd for Priority {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Priority {
    fn cmp(&self, other: &Self) -> Ordering {
        self.score().cmp(&other.score())
    }
}
