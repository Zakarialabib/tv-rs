//! # tv-rs-rules (The Decision Engine)
//!
//! Declarative automation & decision rule engine for the `tv-rs` ecosystem.
//!
//! Evaluates incoming vision, system state, and custom events against declarative rules,
//! schedules, cooldowns, and priorities to produce automated fleet actions.
//!
//! ## Conceptual Example
//!
//! "If entrance camera detects more than 3 people waiting, and time is between 12:00 and 14:00,
//! show queue message on entrance TV and notify manager."
//!
//! ```rust
//! use tv_rs_rules::{
//!     Action, ComparisonOp, Condition, NotificationLevel, Priority, Rule, RuleEngine,
//!     RuleEvent, Schedule, TimeOfDay, VisionCondition,
//! };
//! use tv_rs_vision::VisionEvent;
//! use std::time::Instant;
//!
//! // Build conceptual rule
//! let rule = Rule::new(
//!     "entrance_queue_alert",
//!     "Entrance Queue Overflow Alert",
//!     Condition::and(vec![
//!         Condition::Vision(VisionCondition::PersonCount {
//!             zone: "entrance".to_string(),
//!             op: ComparisonOp::Greater,
//!             count: 3,
//!         }),
//!         Condition::Schedule(
//!             Schedule::time_range("12:00", "14:00").unwrap()
//!         ),
//!     ]),
//!     vec![
//!         Action::ShowMessage {
//!             target: "entrance_tv".to_string(),
//!             message: "High queue volume at entrance. Please proceed to checkout B.".to_string(),
//!             duration_secs: Some(30),
//!         },
//!         Action::Notify {
//!             recipient: "manager".to_string(),
//!             message: "Queue volume exceeded 3 people at entrance camera.".to_string(),
//!             level: NotificationLevel::Warning,
//!         },
//!     ],
//! ).with_priority(Priority::High);
//!
//! let mut engine = RuleEngine::new();
//! engine.add_rule(rule);
//!
//! // Simulate VisionEvent: 4 people detected at entrance
//! let vision_event = VisionEvent::PersonDetected {
//!     zone: "entrance".to_string(),
//!     count: 4,
//!     timestamp: None,
//! };
//!
//! let rule_event = RuleEvent::from(vision_event);
//! let time = TimeOfDay::parse("12:30").unwrap(); // 12:30 PM
//!
//! let actions = engine.process_event_at(&rule_event, time, Instant::now());
//! assert_eq!(actions.len(), 2);
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod action;
pub mod condition;
pub mod conflict;
pub mod cooldown;
pub mod engine;
pub mod error;
pub mod priority;
pub mod scene;
pub mod schedule;
pub mod types;

pub use action::{Action, ActionExecutor, NotificationLevel};
pub use condition::{ComparisonOp, Condition, VisionCondition};
pub use conflict::{ConflictPolicy, ConflictResolver};
pub use cooldown::CooldownTracker;
pub use engine::{Rule, RuleEngine};
pub use error::{Result, RuleError};
pub use priority::Priority;
pub use scene::{Scene, SceneRegistry};
pub use schedule::Schedule;
pub use types::{DayOfWeek, EvaluationContext, RuleEvent, TimeOfDay, Value};
