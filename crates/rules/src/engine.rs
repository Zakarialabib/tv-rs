//! Declarative Rule definition and RuleEngine process executor.

use crate::action::Action;
use crate::condition::Condition;
use crate::conflict::{CandidateAction, ConflictPolicy, ConflictResolver};
use crate::cooldown::CooldownTracker;
use crate::priority::Priority;
use crate::scene::SceneRegistry;
use crate::types::{EvaluationContext, RuleEvent, TimeOfDay};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// A declarative automation rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    /// Unique identifier for the rule.
    pub id: String,
    /// Human readable name/description.
    pub name: String,
    /// Priority level for conflict resolution.
    pub priority: Priority,
    /// Cooldown window in seconds.
    #[serde(default)]
    pub cooldown_secs: u64,
    /// Condition that triggers this rule.
    pub condition: Condition,
    /// List of actions to execute when rule fires.
    pub actions: Vec<Action>,
    /// Whether rule is currently enabled.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

impl Rule {
    /// Creates a new declarative rule.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        condition: Condition,
        actions: Vec<Action>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            priority: Priority::default(),
            cooldown_secs: 0,
            condition,
            actions,
            enabled: true,
        }
    }

    /// Builder method to set rule priority.
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// Builder method to set cooldown duration in seconds.
    pub fn with_cooldown_secs(mut self, secs: u64) -> Self {
        self.cooldown_secs = secs;
        self
    }
}

/// Rule Engine turning events into automated decisions and actions.
#[derive(Debug)]
pub struct RuleEngine {
    rules: Vec<Rule>,
    scene_registry: SceneRegistry,
    cooldown_tracker: CooldownTracker,
    conflict_resolver: ConflictResolver,
    sequence_counter: u64,
    state: HashMap<String, crate::types::Value>,
}

impl Default for RuleEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl RuleEngine {
    /// Creates a new `RuleEngine` with default settings.
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            scene_registry: SceneRegistry::new(),
            cooldown_tracker: CooldownTracker::new(),
            conflict_resolver: ConflictResolver::new(ConflictPolicy::HighestPriorityWins),
            sequence_counter: 0,
            state: HashMap::new(),
        }
    }

    /// Adds a rule to the engine.
    pub fn add_rule(&mut self, rule: Rule) {
        self.rules.push(rule);
    }

    /// Registers a scene with the scene registry.
    pub fn register_scene(&mut self, scene: crate::scene::Scene) {
        self.scene_registry.register(scene);
    }

    /// Sets the conflict policy.
    pub fn set_conflict_policy(&mut self, policy: ConflictPolicy) {
        self.conflict_resolver = ConflictResolver::new(policy);
    }

    /// Updates or inserts a global state value.
    pub fn set_state(&mut self, key: impl Into<String>, value: crate::types::Value) {
        self.state.insert(key.into(), value);
    }

    /// Evaluates an event at the current system time and time of day, returning the resolved actions.
    pub fn process_event_at(
        &mut self,
        event: &RuleEvent,
        time_of_day: TimeOfDay,
        now: Instant,
    ) -> Vec<Action> {
        let ctx = EvaluationContext::new(event, time_of_day).with_state(self.state.clone());
        let mut candidate_actions = Vec::new();

        for rule in &self.rules {
            if !rule.enabled {
                continue;
            }

            // Cooldown check
            if rule.cooldown_secs > 0
                && self.cooldown_tracker.is_on_cooldown(
                    &rule.id,
                    Duration::from_secs(rule.cooldown_secs),
                    now,
                )
            {
                continue;
            }

            // Evaluate condition
            if rule.condition.evaluate(&ctx) {
                // Record trigger for cooldown
                if rule.cooldown_secs > 0 {
                    self.cooldown_tracker.record_trigger(&rule.id, now);
                }

                self.sequence_counter += 1;

                // Expand scenes and collect candidate actions
                for action in &rule.actions {
                    let expanded = self.scene_registry.expand_action(action);
                    for exp_act in expanded {
                        candidate_actions.push(CandidateAction {
                            priority: rule.priority,
                            sequence: self.sequence_counter,
                            action: exp_act,
                        });
                    }
                }
            }
        }

        self.conflict_resolver.resolve(candidate_actions)
    }

    /// Evaluates an event using `Instant::now()` and `00:00:00` time of day default.
    pub fn process_event(&mut self, event: &RuleEvent) -> Vec<Action> {
        self.process_event_at(event, TimeOfDay::new(0, 0, 0), Instant::now())
    }
}
