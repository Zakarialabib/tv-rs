//! A scriptable backend behind `feature = "mock-backend"`.
//!
//! Every phase of the device model is developed against this instead of hardware:
//! it answers immediately (so no runtime is needed), it can be told to fail in the
//! ways real televisions fail, and it records the physical actions it performed so
//! tests can assert what actually happened rather than what was returned.

use std::sync::Mutex;

use async_trait::async_trait;

use crate::capability::{
    AppLaunch, AppRef, Capability, CapabilitySet, DeviceHandle, Input, InputSelect, Key, KeyInput,
    PowerControl, VolumeControl,
};
use crate::command::CommandId;
use crate::descriptor::{DeviceDescriptor, DeviceId, PowerState};
use crate::error::{AuthError, ProtocolError, StateError, TransportError, TvError};

/// A failure the mock should produce for the next mutating command.
///
/// The set is chosen from real-world failures, not from imagination: every variant
/// corresponds to a case that has to be handled by a caller and cannot be discovered
/// from a success path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// Nothing was sent: the connection could not be established.
    TimeoutBeforeSend,
    /// The command was sent and the answer never arrived — the most common real
    /// failure, and the one that causes double-toggles when handled naively.
    TimeoutAfterSend,
    /// The device answered with something unparseable.
    MalformedResponse,
    /// The credential stopped being accepted mid-session.
    AuthExpired,
    /// The device went to standby while the command was in flight.
    PoweredOffDuringCommand,
}

/// A physical action the mock performed.
///
/// Keypresses, launches and volume changes are recorded per call, so a test can
/// prove how many *physical* actions a caller caused. The mock deliberately does
/// **not** deduplicate by [`CommandId`]: deduplication is the caller's contract,
/// and a mock that hides that would let a buggy caller pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// A key was pressed.
    Key(Key),
    /// An application was launched.
    Launch(String),
    /// The device was woken.
    Wake,
    /// The device was put to standby.
    Sleep,
    /// The volume was set.
    SetVolume(u8),
    /// Mute was set.
    SetMute(bool),
    /// An input was selected.
    SelectInput(Input),
}

/// A television that exists only in tests.
pub struct MockBackend {
    descriptor: DeviceDescriptor,
    capabilities: CapabilitySet,
    power: Mutex<PowerState>,
    volume: Mutex<Option<u8>>,
    muted: Mutex<bool>,
    foreground: Mutex<Option<AppRef>>,
    input: Mutex<Option<Input>>,
    actions: Mutex<Vec<Action>>,
    next_failure: Mutex<Option<Failure>>,
}

impl MockBackend {
    /// A fully capable mock, awake, with no state observed yet.
    #[must_use]
    pub fn new(descriptor: DeviceDescriptor) -> Self {
        let mut capabilities = CapabilitySet::empty();
        for capability in [
            Capability::PowerState,
            Capability::Wake,
            Capability::Key,
            Capability::AppLaunch,
            Capability::Foreground,
            Capability::InputSelect,
            Capability::Volume,
        ] {
            capabilities.insert(capability);
        }
        Self {
            descriptor,
            capabilities,
            power: Mutex::new(PowerState::On),
            volume: Mutex::new(Some(30)),
            muted: Mutex::new(false),
            foreground: Mutex::new(None),
            input: Mutex::new(None),
            actions: Mutex::new(Vec::new()),
            next_failure: Mutex::new(None),
        }
    }

    /// A mock that advertises exactly `capabilities` — used to prove that a caller
    /// asking for more gets [`TvError::Unsupported`] instead of a surprise.
    #[must_use]
    pub fn with_capabilities(mut self, capabilities: CapabilitySet) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Starts from a given power state.
    #[must_use]
    pub fn starting_at(mut self, power: PowerState) -> Self {
        self.power = Mutex::new(power);
        self
    }

    /// Makes the next mutating command fail with `failure`.
    pub fn fail_next(&self, failure: Failure) {
        *self.next_failure.lock().expect("mock lock") = Some(failure);
    }

    /// The descriptor the mock was built with.
    #[must_use]
    pub fn descriptor(&self) -> &DeviceDescriptor {
        &self.descriptor
    }

    /// Every physical action performed so far.
    #[must_use]
    pub fn actions(&self) -> Vec<Action> {
        self.actions.lock().expect("mock lock").clone()
    }

    /// Drains the action log.
    #[must_use]
    pub fn take_actions(&self) -> Vec<Action> {
        std::mem::take(&mut self.actions.lock().expect("mock lock"))
    }

    fn record(&self, action: Action) {
        self.actions.lock().expect("mock lock").push(action);
    }

    /// Runs `action`, applying a scripted failure first when one is queued.
    ///
    /// A failure before send performs nothing; every other failure happens after the
    /// command reached the device, so the action is recorded — which is exactly the
    /// distinction a caller must survive.
    fn perform(&self, action: Action) -> Result<(), TvError> {
        match self.next_failure.lock().expect("mock lock").take() {
            None => {
                self.record(action);
                Ok(())
            }
            Some(Failure::TimeoutBeforeSend) => Err(error_for(Failure::TimeoutBeforeSend)),
            Some(failure) => {
                self.record(action);
                Err(error_for(failure))
            }
        }
    }

    fn unsupported(&self, capability: Capability) -> TvError {
        TvError::Unsupported(capability)
    }
}

/// Maps a scripted failure onto the taxonomy.
fn error_for(failure: Failure) -> TvError {
    match failure {
        Failure::TimeoutBeforeSend | Failure::TimeoutAfterSend => {
            TvError::Transport(TransportError::Timeout)
        }
        Failure::MalformedResponse => {
            TvError::Protocol(ProtocolError::Malformed("<html>".to_string()))
        }
        Failure::AuthExpired => TvError::Auth(AuthError::TokenExpired),
        Failure::PoweredOffDuringCommand => TvError::State(StateError::NotReady),
    }
}

impl DeviceHandle for MockBackend {
    fn id(&self) -> &DeviceId {
        &self.descriptor.id
    }

    fn descriptor(&self) -> &DeviceDescriptor {
        &self.descriptor
    }

    fn capabilities(&self) -> CapabilitySet {
        self.capabilities
    }

    fn as_power(&self) -> Option<&dyn PowerControl> {
        self.capabilities
            .contains(Capability::PowerState)
            .then_some(self as &dyn PowerControl)
    }

    fn as_key(&self) -> Option<&dyn KeyInput> {
        self.capabilities
            .contains(Capability::Key)
            .then_some(self as &dyn KeyInput)
    }

    fn as_app(&self) -> Option<&dyn AppLaunch> {
        self.capabilities
            .contains(Capability::AppLaunch)
            .then_some(self as &dyn AppLaunch)
    }

    fn as_input(&self) -> Option<&dyn InputSelect> {
        self.capabilities
            .contains(Capability::InputSelect)
            .then_some(self as &dyn InputSelect)
    }

    fn as_volume(&self) -> Option<&dyn VolumeControl> {
        self.capabilities
            .contains(Capability::Volume)
            .then_some(self as &dyn VolumeControl)
    }
}

#[async_trait]
impl PowerControl for MockBackend {
    async fn power_state(&self) -> Result<PowerState, TvError> {
        Ok(*self.power.lock().expect("mock lock"))
    }

    async fn wake(&self) -> Result<(), TvError> {
        if !self.capabilities.contains(Capability::Wake) {
            return Err(self.unsupported(Capability::Wake));
        }
        self.perform(Action::Wake)?;
        *self.power.lock().expect("mock lock") = PowerState::On;
        Ok(())
    }

    async fn sleep(&self) -> Result<(), TvError> {
        self.perform(Action::Sleep)?;
        *self.power.lock().expect("mock lock") = PowerState::Standby;
        Ok(())
    }
}

#[async_trait]
impl KeyInput for MockBackend {
    async fn key_press(&self, key: Key, _cmd: CommandId) -> Result<(), TvError> {
        if !self.capabilities.contains(Capability::Key) {
            return Err(self.unsupported(Capability::Key));
        }
        self.perform(Action::Key(key))
    }
}

#[async_trait]
impl AppLaunch for MockBackend {
    async fn launch(&self, app: AppRef, _cmd: CommandId) -> Result<(), TvError> {
        if !self.capabilities.contains(Capability::AppLaunch) {
            return Err(self.unsupported(Capability::AppLaunch));
        }
        self.perform(Action::Launch(app.id.clone()))?;
        *self.foreground.lock().expect("mock lock") = Some(app);
        Ok(())
    }

    async fn foreground(&self) -> Result<Option<AppRef>, TvError> {
        if !self.capabilities.contains(Capability::Foreground) {
            return Err(self.unsupported(Capability::Foreground));
        }
        Ok(self.foreground.lock().expect("mock lock").clone())
    }
}

#[async_trait]
impl InputSelect for MockBackend {
    async fn active_input(&self) -> Result<Option<Input>, TvError> {
        if !self.capabilities.contains(Capability::InputSelect) {
            return Err(self.unsupported(Capability::InputSelect));
        }
        Ok(self.input.lock().expect("mock lock").clone())
    }

    async fn select_input(&self, input: Input, _cmd: CommandId) -> Result<(), TvError> {
        if !self.capabilities.contains(Capability::InputSelect) {
            return Err(self.unsupported(Capability::InputSelect));
        }
        self.perform(Action::SelectInput(input.clone()))?;
        *self.input.lock().expect("mock lock") = Some(input);
        Ok(())
    }
}

#[async_trait]
impl VolumeControl for MockBackend {
    async fn volume(&self) -> Result<Option<u8>, TvError> {
        if !self.capabilities.contains(Capability::Volume) {
            return Err(self.unsupported(Capability::Volume));
        }
        Ok(*self.volume.lock().expect("mock lock"))
    }

    async fn set_volume(&self, level: u8, _cmd: CommandId) -> Result<(), TvError> {
        if !self.capabilities.contains(Capability::Volume) {
            return Err(self.unsupported(Capability::Volume));
        }
        self.perform(Action::SetVolume(level))?;
        *self.volume.lock().expect("mock lock") = Some(level);
        Ok(())
    }

    async fn set_mute(&self, muted: bool, _cmd: CommandId) -> Result<(), TvError> {
        if !self.capabilities.contains(Capability::Volume) {
            return Err(self.unsupported(Capability::Volume));
        }
        self.perform(Action::SetMute(muted))?;
        *self.muted.lock().expect("mock lock") = muted;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Remedy;
    use futures::executor::block_on;

    fn descriptor() -> DeviceDescriptor {
        DeviceDescriptor::new(
            DeviceId::new("mock-1").unwrap(),
            crate::DeviceOs::Roku,
            "10.0.0.5",
        )
        .with_name("Mock TV")
    }

    fn mock() -> MockBackend {
        MockBackend::new(descriptor())
    }

    #[test]
    fn capabilities_are_visible_before_any_command() {
        let mock = mock();
        assert!(mock.as_power().is_some());
        assert!(mock.as_key().is_some());

        let limited = MockBackend::new(descriptor())
            .with_capabilities(CapabilitySet::of(Capability::PowerState));
        assert!(limited.as_power().is_some());
        assert!(limited.as_key().is_none());
        assert!(limited.as_input().is_none());
    }

    #[test]
    fn an_absent_capability_is_unsupported_not_silence() {
        let limited = MockBackend::new(descriptor())
            .with_capabilities(CapabilitySet::of(Capability::PowerState));
        let error = block_on(KeyInput::key_press(&limited, Key::Home, CommandId::new()))
            .expect_err("must fail");
        assert_eq!(error.remedy(), Remedy::Disable);
        assert!(matches!(error, TvError::Unsupported(Capability::Key)));
        assert!(limited.actions().is_empty());
    }

    #[test]
    fn timeout_before_send_performs_no_action() {
        let mock = mock();
        mock.fail_next(Failure::TimeoutBeforeSend);
        let error = block_on(KeyInput::key_press(&mock, Key::Home, CommandId::new()))
            .expect_err("must fail");
        assert_eq!(error.remedy(), Remedy::Retry);
        assert!(mock.actions().is_empty());
    }

    #[test]
    fn timeout_after_send_still_performed_the_action() {
        let mock = mock();
        mock.fail_next(Failure::TimeoutAfterSend);
        let error = block_on(KeyInput::key_press(&mock, Key::Power, CommandId::new()))
            .expect_err("must fail");
        assert_eq!(error.remedy(), Remedy::Retry);
        assert_eq!(mock.actions(), vec![Action::Key(Key::Power)]);
    }

    #[test]
    fn a_scripted_failure_is_consumed_once() {
        let mock = mock();
        mock.fail_next(Failure::MalformedResponse);
        assert!(block_on(mock.sleep()).is_err());
        assert!(
            block_on(mock.sleep()).is_ok(),
            "only the next command fails"
        );
        // A malformed reply means the command did reach the device, so both calls
        // are recorded; only the first one failed.
        assert_eq!(mock.actions(), vec![Action::Sleep, Action::Sleep]);
    }

    #[test]
    fn auth_expiry_routes_to_reprovisioning() {
        let mock = mock();
        mock.fail_next(Failure::AuthExpired);
        let error = block_on(mock.sleep()).expect_err("must fail");
        assert_eq!(error.remedy(), Remedy::Reprovision);
    }

    #[test]
    fn a_powered_off_device_reports_not_ready_and_back_off() {
        let mock = mock().starting_at(PowerState::Standby);
        assert_eq!(block_on(mock.power_state()).unwrap(), PowerState::Standby);
        mock.fail_next(Failure::PoweredOffDuringCommand);
        let error =
            block_on(mock.launch(AppRef::new("netflix"), CommandId::new())).expect_err("must fail");
        // A device that vanished mid-command is a state problem, not a protocol one.
        assert_eq!(error.remedy(), Remedy::Wait);
    }

    #[test]
    fn the_mock_does_not_deduplicate_command_ids() {
        // Deduplication belongs to the caller; the mock proves the contract by
        // recording two physical actions for two deliveries of one id.
        let mock = mock();
        let cmd = CommandId::new();
        block_on(KeyInput::key_press(&mock, Key::VolumeUp, cmd)).unwrap();
        block_on(KeyInput::key_press(&mock, Key::VolumeUp, cmd)).unwrap();
        assert_eq!(
            mock.actions(),
            vec![Action::Key(Key::VolumeUp), Action::Key(Key::VolumeUp)]
        );
    }

    #[test]
    fn state_is_readable_after_a_write() {
        let mock = mock();
        block_on(mock.set_volume(64, CommandId::new())).unwrap();
        block_on(mock.set_mute(true, CommandId::new())).unwrap();
        block_on(mock.select_input(Input::Hdmi(2), CommandId::new())).unwrap();
        block_on(mock.launch(
            AppRef::new("youtube").with_name("YouTube"),
            CommandId::new(),
        ))
        .unwrap();

        assert_eq!(block_on(mock.volume()).unwrap(), Some(64));
        assert_eq!(block_on(mock.active_input()).unwrap(), Some(Input::Hdmi(2)));
        assert_eq!(
            block_on(mock.foreground()).unwrap().map(|app| app.id),
            Some("youtube".to_string())
        );
    }

    #[test]
    fn waking_reports_on() {
        let mock = mock().starting_at(PowerState::Off);
        block_on(mock.wake()).unwrap();
        assert_eq!(block_on(mock.power_state()).unwrap(), PowerState::On);
        assert_eq!(mock.take_actions(), vec![Action::Wake]);
        assert!(mock.actions().is_empty(), "take_actions drains");
    }
}
