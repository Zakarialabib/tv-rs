//! Reconciliation for fleets of televisions.
//!
//! Three pieces, each usable on its own:
//!
//! | Piece | Question it answers |
//! |---|---|
//! | [`desired`] | *What should this screen be doing?* — behind a trait, so a CMS plugs in without the core knowing |
//! | [`reconciler`] | *What is the next action, given belief and capabilities?* — a pure function |
//! | [`healer`] | *This keeps failing; when do we stop?* — a counter with a ladder |
//!
//! Plus [`audit`]: one JSON object per line, readable by someone who has never seen
//! Rust. That is a product requirement, not a debugging aid.
//!
//! # What this crate refuses to do
//!
//! It does not decide content, does not schedule, and does not send `Key::Power` to
//! make a device "off". Power-on is a wake followed by a read; power-off is a sleep
//! recorded as inferred. A device whose state cannot be established is quarantined,
//! not toggled.
//!
//! ```
//! use tv_rs_agent::{decide, Decision, DesiredState};
//! use tv_rs_core::{Capability, CapabilitySet, ObservedState};
//!
//! let desired = DesiredState::default();
//! let observed = ObservedState::nothing_observed();
//! let capabilities = CapabilitySet::empty().with(Capability::PowerState);
//!
//! // Nothing is known yet, so the first step is to look.
//! assert_eq!(decide(&desired, &observed, capabilities), Decision::ProbePower);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod audit;
pub mod desired;
pub mod healer;
#[cfg(feature = "http")]
pub mod http_provider;
#[cfg(feature = "mqtt")]
pub mod mqtt_provider;
pub mod reconciler;
pub mod remedy;
pub mod runner;
pub mod telemetry;

pub use audit::{AuditEntry, AuditLog};
pub use desired::{
    DesiredPower, DesiredState, DesiredStateProvider, ProviderError, StaticFileProvider,
};
pub use healer::{HealerAction, HealerLadder};
#[cfg(feature = "http")]
pub use http_provider::HttpPollProvider;
#[cfg(feature = "mqtt")]
pub use mqtt_provider::{MqttSubscribeProvider, MqttTlsConfig, MqttTransport};
pub use reconciler::{decide, Decision};
pub use remedy::{plan_remedy, symptom, RemedyPolicy, RemedyStep, Symptom};
pub use runner::{Outcome, ReconcilePolicy, Reconciler};
pub use telemetry::{assess_telemetry, TelemetryPolicy, TelemetryVerdict};
