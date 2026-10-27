//! Event models, generic value representation, and evaluation context.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;
use tv_rs_vision::VisionEvent;

/// Generic scalar value used in rule evaluation comparisons.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    /// Boolean value.
    Bool(bool),
    /// Integer value.
    Integer(i64),
    /// Floating point value.
    Float(f64),
    /// String value.
    String(String),
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Integer(v)
    }
}

impl From<usize> for Value {
    fn from(v: usize) -> Self {
        Value::Integer(v as i64)
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Float(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::String(v.to_string())
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::String(v)
    }
}

/// Incoming event processed by the rule engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum RuleEvent {
    /// Vision analytics event from `tv-rs-vision`.
    Vision(VisionEvent),

    /// Device state change event.
    StateChange {
        /// Target device or entity ID.
        target: String,
        /// Property or attribute name.
        property: String,
        /// New property value.
        value: Value,
    },

    /// Generic custom event.
    Custom {
        /// Event topic or name.
        name: String,
        /// Payload key-value map.
        payload: HashMap<String, Value>,
    },
}

impl From<VisionEvent> for RuleEvent {
    fn from(ve: VisionEvent) -> Self {
        RuleEvent::Vision(ve)
    }
}

/// Time representation for rule schedule matching (HH:MM:SS or HH:MM).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TimeOfDay {
    /// Hour of day [0..23].
    pub hour: u32,
    /// Minute of hour [0..59].
    pub minute: u32,
    /// Second of minute [0..59].
    pub second: u32,
}

impl TimeOfDay {
    /// Creates a new `TimeOfDay`.
    pub fn new(hour: u32, minute: u32, second: u32) -> Self {
        Self {
            hour,
            minute,
            second,
        }
    }

    /// Parses a time string in "HH:MM" or "HH:MM:SS" format.
    pub fn parse(s: &str) -> crate::Result<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() < 2 || parts.len() > 3 {
            return Err(crate::RuleError::InvalidTimeFormat(s.to_string()));
        }
        let hour = parts[0]
            .parse::<u32>()
            .map_err(|_| crate::RuleError::InvalidTimeFormat(s.to_string()))?;
        let minute = parts[1]
            .parse::<u32>()
            .map_err(|_| crate::RuleError::InvalidTimeFormat(s.to_string()))?;
        let second = if parts.len() == 3 {
            parts[2]
                .parse::<u32>()
                .map_err(|_| crate::RuleError::InvalidTimeFormat(s.to_string()))?
        } else {
            0
        };

        if hour > 23 || minute > 59 || second > 59 {
            return Err(crate::RuleError::InvalidTimeFormat(s.to_string()));
        }

        Ok(Self {
            hour,
            minute,
            second,
        })
    }

    /// Converts to total seconds from midnight.
    pub fn as_seconds(&self) -> u32 {
        self.hour * 3600 + self.minute * 60 + self.second
    }
}

/// Day of week enum for schedule filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DayOfWeek {
    /// Monday
    Monday = 1,
    /// Tuesday
    Tuesday = 2,
    /// Wednesday
    Wednesday = 3,
    /// Thursday
    Thursday = 4,
    /// Friday
    Friday = 5,
    /// Saturday
    Saturday = 6,
    /// Sunday
    Sunday = 7,
}

/// Context provided during rule condition evaluation.
#[derive(Debug, Clone)]
pub struct EvaluationContext<'a> {
    /// The event triggering evaluation.
    pub event: &'a RuleEvent,
    /// Current wall-clock system time.
    pub now: SystemTime,
    /// Current time of day for schedule matching.
    pub current_time: TimeOfDay,
    /// Current day of week (1 = Monday, 7 = Sunday).
    pub day_of_week: Option<DayOfWeek>,
    /// Global state store for property lookups.
    pub state: HashMap<String, Value>,
}

impl<'a> EvaluationContext<'a> {
    /// Creates a new evaluation context for a given event and time.
    pub fn new(event: &'a RuleEvent, current_time: TimeOfDay) -> Self {
        Self {
            event,
            now: SystemTime::now(),
            current_time,
            day_of_week: None,
            state: HashMap::new(),
        }
    }

    /// Builder method to set day of week.
    pub fn with_day_of_week(mut self, dow: DayOfWeek) -> Self {
        self.day_of_week = Some(dow);
        self
    }

    /// Builder method to populate system state.
    pub fn with_state(mut self, state: HashMap<String, Value>) -> Self {
        self.state = state;
        self
    }
}
