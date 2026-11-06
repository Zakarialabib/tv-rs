//! Power and input control helpers.

use tv_rs_core::Input;

/// Companion input selection options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompanionInput {
    /// HDMI input port number (1-based).
    Hdmi(u8),
    /// USB or AV input.
    Other(alloc::string::String),
}

impl From<CompanionInput> for Input {
    fn from(input: CompanionInput) -> Self {
        match input {
            CompanionInput::Hdmi(port) => Input::Hdmi(port),
            CompanionInput::Other(name) => Input::Other(name),
        }
    }
}
