//! The DLNA GENA subscription surface for the `tv-rs` facade.
//!
//! The `dlna` crate owns the GENA subscription machinery; this module is a
//! thin re-export so the facade can hand a `SubscriptionHandle` to callers
//! of the device event stream without leaking dlna internals.


pub use dlna::subscription::{FacadeEvent, SubscriptionHandle};
