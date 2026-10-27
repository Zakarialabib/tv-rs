//! Conflict resolution policies when multiple rules target the same resources.

use crate::action::Action;
use crate::priority::Priority;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Policy for resolving conflicting actions on target resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    /// Highest priority rule's action wins for each target resource and action domain.
    #[default]
    HighestPriorityWins,

    /// Latest rule's action wins.
    LatestWins,

    /// No conflict filtering; retain all actions.
    MergeAll,
}

/// Candidate rule action item with metadata for conflict evaluation.
#[derive(Debug, Clone)]
pub struct CandidateAction {
    /// Priority of the originating rule.
    pub priority: Priority,
    /// Trigger sequence or timestamp index.
    pub sequence: u64,
    /// The action to be performed.
    pub action: Action,
}

/// Conflict resolver that filters candidate actions based on policy.
#[derive(Debug, Default, Clone)]
pub struct ConflictResolver {
    policy: ConflictPolicy,
}

impl ConflictResolver {
    /// Creates a new `ConflictResolver` with the given policy.
    pub fn new(policy: ConflictPolicy) -> Self {
        Self { policy }
    }

    /// Resolves conflicts among a collection of candidate actions.
    pub fn resolve(&self, candidates: Vec<CandidateAction>) -> Vec<Action> {
        match self.policy {
            ConflictPolicy::MergeAll => candidates.into_iter().map(|c| c.action).collect(),

            ConflictPolicy::HighestPriorityWins => {
                let mut resolved_targets: HashMap<String, CandidateAction> = HashMap::new();
                let mut non_target_actions: Vec<Action> = Vec::new();

                for candidate in candidates {
                    if let Some(target) = candidate.action.target_key() {
                        match resolved_targets.get(&target) {
                            Some(existing) => {
                                if candidate.priority > existing.priority
                                    || (candidate.priority == existing.priority
                                        && candidate.sequence > existing.sequence)
                                {
                                    resolved_targets.insert(target, candidate);
                                }
                            }
                            None => {
                                resolved_targets.insert(target, candidate);
                            }
                        }
                    } else {
                        non_target_actions.push(candidate.action);
                    }
                }

                let mut result: Vec<Action> =
                    resolved_targets.into_values().map(|c| c.action).collect();
                result.extend(non_target_actions);
                result
            }

            ConflictPolicy::LatestWins => {
                let mut resolved_targets: HashMap<String, CandidateAction> = HashMap::new();
                let mut non_target_actions: Vec<Action> = Vec::new();

                for candidate in candidates {
                    if let Some(target) = candidate.action.target_key() {
                        match resolved_targets.get(&target) {
                            Some(existing) => {
                                if candidate.sequence >= existing.sequence {
                                    resolved_targets.insert(target, candidate);
                                }
                            }
                            None => {
                                resolved_targets.insert(target, candidate);
                            }
                        }
                    } else {
                        non_target_actions.push(candidate.action);
                    }
                }

                let mut result: Vec<Action> =
                    resolved_targets.into_values().map(|c| c.action).collect();
                result.extend(non_target_actions);
                result
            }
        }
    }
}
