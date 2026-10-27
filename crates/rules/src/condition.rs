//! Declarative conditions and logical operators for event matching.

use crate::schedule::Schedule;
use crate::types::{EvaluationContext, RuleEvent, Value};
use serde::{Deserialize, Serialize};
use tv_rs_vision::VisionEvent;

/// Comparison operators for numeric, string, or boolean values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonOp {
    /// Equal (`==`).
    Equal,
    /// Not Equal (`!=`).
    NotEqual,
    /// Greater than (`>`).
    Greater,
    /// Greater than or equal (`>=`).
    GreaterEq,
    /// Less than (`<`).
    Less,
    /// Less than or equal (`<=`).
    LessEq,
    /// String contains substring.
    Contains,
}

impl ComparisonOp {
    /// Evaluates two values with this comparison operator.
    pub fn evaluate(&self, lhs: &Value, rhs: &Value) -> bool {
        match (lhs, rhs) {
            (Value::Integer(l), Value::Integer(r)) => match self {
                Self::Equal => l == r,
                Self::NotEqual => l != r,
                Self::Greater => l > r,
                Self::GreaterEq => l >= r,
                Self::Less => l < r,
                Self::LessEq => l <= r,
                Self::Contains => false,
            },
            (Value::Float(l), Value::Float(r)) => match self {
                Self::Equal => (l - r).abs() < f64::EPSILON,
                Self::NotEqual => (l - r).abs() >= f64::EPSILON,
                Self::Greater => l > r,
                Self::GreaterEq => l >= r,
                Self::Less => l < r,
                Self::LessEq => l <= r,
                Self::Contains => false,
            },
            (Value::Integer(l), Value::Float(r)) => match self {
                Self::Greater => (*l as f64) > *r,
                Self::GreaterEq => (*l as f64) >= *r,
                Self::Less => (*l as f64) < *r,
                Self::LessEq => (*l as f64) <= *r,
                Self::Equal => ((*l as f64) - *r).abs() < f64::EPSILON,
                Self::NotEqual => ((*l as f64) - *r).abs() >= f64::EPSILON,
                Self::Contains => false,
            },
            (Value::Float(l), Value::Integer(r)) => match self {
                Self::Greater => *l > (*r as f64),
                Self::GreaterEq => *l >= (*r as f64),
                Self::Less => *l < (*r as f64),
                Self::LessEq => *l <= (*r as f64),
                Self::Equal => (*l - (*r as f64)).abs() < f64::EPSILON,
                Self::NotEqual => (*l - (*r as f64)).abs() >= f64::EPSILON,
                Self::Contains => false,
            },
            (Value::String(l), Value::String(r)) => match self {
                Self::Equal => l == r,
                Self::NotEqual => l != r,
                Self::Contains => l.contains(r),
                _ => false,
            },
            (Value::Bool(l), Value::Bool(r)) => match self {
                Self::Equal => l == r,
                Self::NotEqual => l != r,
                _ => false,
            },
            _ => false,
        }
    }
}

/// Vision event conditions for matching zone, person/object counts, or gestures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VisionCondition {
    /// Person count detected in a zone matches threshold.
    PersonCount {
        /// Zone ID to match.
        zone: String,
        /// Comparison operator.
        op: ComparisonOp,
        /// Threshold count value.
        count: usize,
    },

    /// Gesture detected in a zone.
    Gesture {
        /// Zone ID to match.
        zone: String,
        /// Expected gesture name (e.g. "hand_wave", "raise_hand").
        gesture: String,
    },

    /// Object detected in a zone.
    ObjectDetected {
        /// Zone ID to match.
        zone: String,
        /// Object label (e.g. "car", "bottle").
        label: String,
        /// Comparison operator for count.
        op: ComparisonOp,
        /// Threshold count value.
        count: usize,
    },

    /// Zone occupancy summary.
    ZoneOccupancy {
        /// Zone ID to match.
        zone: String,
        /// Comparison operator.
        op: ComparisonOp,
        /// Threshold count value.
        count: usize,
    },
}

impl VisionCondition {
    /// Evaluates this vision condition against a `VisionEvent`.
    pub fn evaluate(&self, event: &VisionEvent) -> bool {
        match (self, event) {
            (
                VisionCondition::PersonCount { zone, op, count },
                VisionEvent::PersonDetected {
                    zone: ev_zone,
                    count: ev_count,
                    ..
                },
            ) => zone == ev_zone && op.evaluate(&Value::from(*ev_count), &Value::from(*count)),

            (
                VisionCondition::Gesture { zone, gesture },
                VisionEvent::GestureDetected {
                    zone: ev_zone,
                    gesture: ev_gesture,
                    ..
                },
            ) => {
                let g_str = match ev_gesture {
                    tv_rs_vision::GestureType::HandWave => "hand_wave",
                    tv_rs_vision::GestureType::ThumbsUp => "thumbs_up",
                    tv_rs_vision::GestureType::RaiseHand => "raise_hand",
                    tv_rs_vision::GestureType::Standing => "standing",
                    tv_rs_vision::GestureType::Sitting => "sitting",
                    tv_rs_vision::GestureType::Other(s) => s.as_str(),
                };
                zone == ev_zone && (gesture == g_str || gesture == "any")
            }

            (
                VisionCondition::ObjectDetected {
                    zone,
                    label,
                    op,
                    count,
                },
                VisionEvent::ObjectDetected {
                    zone: ev_zone,
                    label: ev_label,
                    count: ev_count,
                    ..
                },
            ) => {
                zone == ev_zone
                    && label == ev_label
                    && op.evaluate(&Value::from(*ev_count), &Value::from(*count))
            }

            (
                VisionCondition::ZoneOccupancy { zone, op, count },
                VisionEvent::ZoneOccupancy {
                    zone: ev_zone,
                    count: ev_count,
                    ..
                },
            ) => zone == ev_zone && op.evaluate(&Value::from(*ev_count), &Value::from(*count)),

            _ => false,
        }
    }
}

/// Rule evaluation condition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "condition_type", rename_all = "snake_case")]
pub enum Condition {
    /// Vision analytics condition.
    Vision(VisionCondition),

    /// Schedule condition checking active time range.
    Schedule(Schedule),

    /// Property value comparison in system state.
    Property {
        /// Property key name in system state map.
        key: String,
        /// Comparison operator.
        op: ComparisonOp,
        /// Value to compare against.
        value: Value,
    },

    /// Logical AND of multiple child conditions.
    And {
        /// List of conditions that must all evaluate to true.
        conditions: Vec<Condition>,
    },

    /// Logical OR of multiple child conditions.
    Or {
        /// List of conditions where at least one must evaluate to true.
        conditions: Vec<Condition>,
    },

    /// Logical NOT of child condition.
    Not {
        /// Condition to negate.
        condition: Box<Condition>,
    },

    /// Always evaluates to true.
    Always,
}

impl Condition {
    /// Helper to create a logical AND condition.
    pub fn and(conditions: Vec<Condition>) -> Self {
        Condition::And { conditions }
    }

    /// Helper to create a logical OR condition.
    pub fn or(conditions: Vec<Condition>) -> Self {
        Condition::Or { conditions }
    }

    /// Helper to create a logical NOT condition.
    #[allow(clippy::should_implement_trait)]
    pub fn not(condition: Condition) -> Self {
        Condition::Not {
            condition: Box::new(condition),
        }
    }

    /// Evaluates whether this condition is satisfied given the context.
    pub fn evaluate(&self, ctx: &EvaluationContext) -> bool {
        match self {
            Condition::Vision(vc) => match ctx.event {
                RuleEvent::Vision(ve) => vc.evaluate(ve),
                _ => false,
            },

            Condition::Schedule(sched) => sched.is_active(&ctx.current_time, ctx.day_of_week),

            Condition::Property { key, op, value } => {
                if let Some(val) = ctx.state.get(key) {
                    op.evaluate(val, value)
                } else {
                    false
                }
            }

            Condition::And { conditions } => conditions.iter().all(|c| c.evaluate(ctx)),

            Condition::Or { conditions } => conditions.iter().any(|c| c.evaluate(ctx)),

            Condition::Not { condition } => !condition.evaluate(ctx),

            Condition::Always => true,
        }
    }
}

impl std::ops::Not for Condition {
    type Output = Self;

    fn not(self) -> Self::Output {
        Condition::Not {
            condition: Box::new(self),
        }
    }
}
