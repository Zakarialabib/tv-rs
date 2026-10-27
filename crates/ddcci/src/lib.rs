//! DDC/CI display protocol for physically attached displays.

#![deny(missing_docs)]

pub mod backend;
pub mod handle;
pub mod vcp;

pub use backend::{AndroidBackend, DdcCiBackend, LinuxBackend, MockBackend, WindowsBackend};
pub use handle::DdcCiDeviceHandle;
pub use vcp::{
    input_to_vcp_value, power_state_to_vcp_value, vcp_value_to_input, vcp_value_to_power_state,
    VcpOpcode,
};
