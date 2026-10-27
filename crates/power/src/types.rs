//! Core constants and value types for `tv-rs-power`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::PowerError;

/// Wake-on-LAN magic packets are delivered to UDP port 9 (the discard
/// protocol).
pub const WOL_UDP_PORT: u16 = 9;

/// Default IPv4 broadcast address used when the caller supplies none.
pub const WOL_BROADCAST_ADDR: &str = "255.255.255.255";

/// Total length of a magic packet in bytes: six sync bytes plus the MAC
/// address repeated sixteen times (`6 + 6 * 16`).
pub const MAGIC_PACKET_LEN: usize = 102;

/// Length of an Ethernet MAC address in bytes.
pub const MAC_ADDRESS_LEN: usize = 6;

/// A six-octet Ethernet MAC address.
///
/// Parsing accepts both colon-separated (`AA:BB:CC:DD:EE:FF`) and
/// hyphen-separated (`AA-BB-CC-DD-EE-FF`) forms in any case. [`Display`] and
/// serde always emit the canonical uppercase colon-separated form.
///
/// [`Display`]: fmt::Display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MacAddress([u8; MAC_ADDRESS_LEN]);

impl MacAddress {
    /// Borrows the raw six octets of the address.
    pub fn as_bytes(&self) -> &[u8; MAC_ADDRESS_LEN] {
        &self.0
    }

    /// Returns the raw six octets of the address by value.
    pub fn octets(&self) -> [u8; MAC_ADDRESS_LEN] {
        self.0
    }
}

impl fmt::Display for MacAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}

impl FromStr for MacAddress {
    type Err = PowerError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let normalized = input.trim().replace('-', ":");
        let parts: Vec<&str> = normalized.split(':').collect();
        if parts.len() != MAC_ADDRESS_LEN {
            return Err(PowerError::InvalidMacAddress(input.to_string()));
        }

        let mut octets = [0u8; MAC_ADDRESS_LEN];
        for (index, part) in parts.iter().enumerate() {
            if part.len() != 2 || !part.chars().all(|digit| digit.is_ascii_hexdigit()) {
                return Err(PowerError::InvalidMacAddress(input.to_string()));
            }
            octets[index] = u8::from_str_radix(part, 16)
                .map_err(|_| PowerError::InvalidMacAddress(input.to_string()))?;
        }

        Ok(Self(octets))
    }
}

impl Serialize for MacAddress {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for MacAddress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

/// Observed power state of a TV, as inferred by [`crate::poll_power_state`].
///
/// The semantics implemented by the TCP connect probe are:
/// * [`PowerState::On`] — the connect succeeded, so the application stack is up.
/// * [`PowerState::Standby`] — the connection was refused: the network interface
///   answers but the application services are down (network-only standby, the
///   typical state for Wake-on-Wireless-LAN capable devices).
/// * [`PowerState::Off`] — the connect attempt timed out and nothing answered.
/// * [`PowerState::Unknown`] — the probe failed for any other reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerState {
    /// The main CPU and application services are awake.
    On,
    /// The network interface answers, but application services are down.
    Standby,
    /// Nothing answered within the probe timeout window.
    Off,
    /// The power state could not be determined.
    Unknown,
}

/// Level of confidence in an inferred or reported power state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ConfidenceScore {
    /// Unknown or unverified state.
    Unknown = 0,
    /// Low confidence (e.g. WoL magic packet sent but no response yet).
    Low = 1,
    /// Medium confidence (e.g. network port responded to a TCP connect probe).
    Medium = 2,
    /// High confidence (e.g. TV protocol explicitly reported its state, or energy meter confirms active/standby wattage).
    High = 3,
}

impl fmt::Display for ConfidenceScore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown => write!(f, "Unknown"),
            Self::Low => write!(f, "Low"),
            Self::Medium => write!(f, "Medium"),
            Self::High => write!(f, "High"),
        }
    }
}

/// Source of evidence used to determine a power state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PowerSignalSource {
    /// Explicit TV control protocol report (WebSocket/REST API/MDC/SICP response).
    DirectProtocol,
    /// Power or energy meter measurement (e.g. smart plug reading in Watts).
    EnergyMeter,
    /// TCP port connectivity probe response.
    TcpProbe,
    /// Wake-on-LAN magic packet sent.
    WolPacketSent,
    /// Source is unknown or unrecorded.
    Unknown,
}

impl fmt::Display for PowerSignalSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DirectProtocol => write!(f, "DirectProtocol"),
            Self::EnergyMeter => write!(f, "EnergyMeter"),
            Self::TcpProbe => write!(f, "TcpProbe"),
            Self::WolPacketSent => write!(f, "WolPacketSent"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Evaluated power state accompanied by confidence scoring and evidence source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluatedPowerState {
    /// Estimated or reported power state.
    pub state: PowerState,
    /// Confidence in the evaluated state.
    pub confidence: ConfidenceScore,
    /// Primary signal source behind the evaluation.
    pub source: PowerSignalSource,
}

impl EvaluatedPowerState {
    /// Creates a new evaluated power state.
    pub fn new(state: PowerState, confidence: ConfidenceScore, source: PowerSignalSource) -> Self {
        Self {
            state,
            confidence,
            source,
        }
    }

    /// Default unverified unknown state.
    pub fn unknown() -> Self {
        Self {
            state: PowerState::Unknown,
            confidence: ConfidenceScore::Unknown,
            source: PowerSignalSource::Unknown,
        }
    }
}

impl Default for EvaluatedPowerState {
    fn default() -> Self {
        Self::unknown()
    }
}

/// Energy meter reading (e.g., from a smart plug).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EnergyMeterReading {
    /// Active real power in Watts (W).
    pub power_watts: f64,
    /// Optional AC voltage in Volts (V).
    pub voltage_volts: Option<f64>,
    /// Optional AC current in Amperes (A).
    pub current_amps: Option<f64>,
}

impl EnergyMeterReading {
    /// Creates a reading with active power in Watts.
    pub fn new(power_watts: f64) -> Self {
        Self {
            power_watts,
            voltage_volts: None,
            current_amps: None,
        }
    }

    /// Sets the voltage reading.
    pub fn with_voltage(mut self, voltage_volts: f64) -> Self {
        self.voltage_volts = Some(voltage_volts);
        self
    }

    /// Sets the current reading.
    pub fn with_current(mut self, current_amps: f64) -> Self {
        self.current_amps = Some(current_amps);
        self
    }
}

/// Power profile specifying expected power thresholds and connection ports per device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PowerProfile {
    /// Optional default TCP probe port for power state checks (e.g. 8001 for Tizen, 8060 for Roku).
    pub probe_port: Option<u16>,
    /// Upper wattage limit (inclusive) for standby mode (e.g. 15.0W). Readings below this indicate standby.
    pub standby_power_watts: Option<f64>,
    /// Lower wattage limit (inclusive) for active mode (e.g. 30.0W). Readings above this indicate active power.
    pub active_power_watts: Option<f64>,
    /// Device target MAC address for Wake-on-LAN.
    pub mac_address: Option<MacAddress>,
}

impl PowerProfile {
    /// Default standby threshold in Watts if not customized (15.0W).
    pub const DEFAULT_STANDBY_WATTS: f64 = 15.0;
    /// Default active threshold in Watts if not customized (30.0W).
    pub const DEFAULT_ACTIVE_WATTS: f64 = 30.0;

    /// Creates a new default power profile.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the target probe port.
    pub fn with_probe_port(mut self, port: u16) -> Self {
        self.probe_port = Some(port);
        self
    }

    /// Sets the standby power threshold in Watts.
    pub fn with_standby_power_watts(mut self, watts: f64) -> Self {
        self.standby_power_watts = Some(watts);
        self
    }

    /// Sets the active power threshold in Watts.
    pub fn with_active_power_watts(mut self, watts: f64) -> Self {
        self.active_power_watts = Some(watts);
        self
    }

    /// Sets the MAC address.
    pub fn with_mac_address(mut self, mac: MacAddress) -> Self {
        self.mac_address = Some(mac);
        self
    }

    /// Evaluates power state from energy meter readings based on profile thresholds.
    ///
    /// Semantics:
    /// * Power >= active_power_watts (default 30.0W) -> `PowerState::On` with `ConfidenceScore::High`.
    /// * Power <= standby_power_watts (default 15.0W) -> `PowerState::Standby` with `ConfidenceScore::High`.
    /// * Power between standby and active thresholds -> `PowerState::On` with `ConfidenceScore::Medium`.
    pub fn evaluate_energy(&self, reading: &EnergyMeterReading) -> EvaluatedPowerState {
        let standby_limit = self
            .standby_power_watts
            .unwrap_or(Self::DEFAULT_STANDBY_WATTS);
        let active_limit = self
            .active_power_watts
            .unwrap_or(Self::DEFAULT_ACTIVE_WATTS);

        if reading.power_watts >= active_limit {
            EvaluatedPowerState::new(
                PowerState::On,
                ConfidenceScore::High,
                PowerSignalSource::EnergyMeter,
            )
        } else if reading.power_watts <= standby_limit {
            EvaluatedPowerState::new(
                PowerState::Standby,
                ConfidenceScore::High,
                PowerSignalSource::EnergyMeter,
            )
        } else {
            // Intermediate power draw: device is drawing more than nominal standby but less than full active state.
            EvaluatedPowerState::new(
                PowerState::On,
                ConfidenceScore::Medium,
                PowerSignalSource::EnergyMeter,
            )
        }
    }
}

/// State evaluator that tracks and scores power state transitions for a device.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PowerEvaluator {
    profile: PowerProfile,
    current_evaluated_state: EvaluatedPowerState,
}

impl PowerEvaluator {
    /// Creates a power evaluator initialized with `profile`.
    pub fn new(profile: PowerProfile) -> Self {
        Self {
            profile,
            current_evaluated_state: EvaluatedPowerState::unknown(),
        }
    }

    /// Returns a reference to the power profile.
    pub fn profile(&self) -> &PowerProfile {
        &self.profile
    }

    /// Returns a mutable reference to the power profile.
    pub fn profile_mut(&mut self) -> &mut PowerProfile {
        &mut self.profile
    }

    /// Returns the currently evaluated power state.
    pub fn current_state(&self) -> EvaluatedPowerState {
        self.current_evaluated_state
    }

    /// Records an explicit power state reported directly by the TV protocol (High confidence).
    pub fn record_direct_protocol(&mut self, state: PowerState) -> EvaluatedPowerState {
        let evaluated = EvaluatedPowerState::new(
            state,
            ConfidenceScore::High,
            PowerSignalSource::DirectProtocol,
        );
        self.current_evaluated_state = evaluated;
        evaluated
    }

    /// Records a TCP connect probe result.
    pub fn record_tcp_probe(&mut self, probe_result: PowerState) -> EvaluatedPowerState {
        let (state, confidence) = match probe_result {
            PowerState::On => (PowerState::On, ConfidenceScore::Medium),
            PowerState::Standby => (PowerState::Standby, ConfidenceScore::Medium),
            PowerState::Off => (PowerState::Off, ConfidenceScore::Low),
            PowerState::Unknown => (PowerState::Unknown, ConfidenceScore::Unknown),
        };

        let evaluated = EvaluatedPowerState::new(state, confidence, PowerSignalSource::TcpProbe);
        self.current_evaluated_state = evaluated;
        evaluated
    }

    /// Records that a Wake-on-LAN magic packet was sent (Low confidence until probe/protocol response).
    pub fn record_wol_sent(&mut self) -> EvaluatedPowerState {
        let evaluated = EvaluatedPowerState::new(
            PowerState::On,
            ConfidenceScore::Low,
            PowerSignalSource::WolPacketSent,
        );
        self.current_evaluated_state = evaluated;
        evaluated
    }

    /// Records an energy meter reading and updates state based on power profile thresholds.
    pub fn record_energy_reading(&mut self, reading: &EnergyMeterReading) -> EvaluatedPowerState {
        let evaluated = self.profile.evaluate_energy(reading);
        self.current_evaluated_state = evaluated;
        evaluated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_str_accepts_colon_and_hyphen_forms_and_mixed_case() {
        let expected = MacAddress([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);

        assert_eq!("AA:BB:CC:DD:EE:FF".parse::<MacAddress>().unwrap(), expected);
        assert_eq!("aa:bb:cc:dd:ee:ff".parse::<MacAddress>().unwrap(), expected);
        assert_eq!("Aa-Bb-Cc-Dd-Ee-Ff".parse::<MacAddress>().unwrap(), expected);
        assert_eq!("AA-BB-CC-DD-EE-FF".parse::<MacAddress>().unwrap(), expected);
    }

    #[test]
    fn from_str_rejects_malformed_input() {
        assert!("AA:BB:CC:DD:EE".parse::<MacAddress>().is_err());
        assert!("GG:BB:CC:DD:EE:FF".parse::<MacAddress>().is_err());
        assert!("AABBCCDDEEFF".parse::<MacAddress>().is_err());
        assert!("AA:BB:CC:DD:EE:FG".parse::<MacAddress>().is_err());
        assert!("AA.BB.CC.DD.EE.FF".parse::<MacAddress>().is_err());
    }

    #[test]
    fn display_from_str_round_trip_is_canonical_uppercase() {
        let parsed: MacAddress = "aa-bb-cc-dd-ee-ff".parse().unwrap();
        let rendered = parsed.to_string();

        assert_eq!(rendered, "AA:BB:CC:DD:EE:FF");
        assert_eq!(rendered.parse::<MacAddress>().unwrap(), parsed);
    }

    #[test]
    fn confidence_score_ordering_and_display() {
        assert!(ConfidenceScore::High > ConfidenceScore::Medium);
        assert!(ConfidenceScore::Medium > ConfidenceScore::Low);
        assert!(ConfidenceScore::Low > ConfidenceScore::Unknown);

        assert_eq!(ConfidenceScore::High.to_string(), "High");
        assert_eq!(ConfidenceScore::Medium.to_string(), "Medium");
        assert_eq!(ConfidenceScore::Low.to_string(), "Low");
        assert_eq!(ConfidenceScore::Unknown.to_string(), "Unknown");
    }

    #[test]
    fn signal_source_display() {
        assert_eq!(
            PowerSignalSource::DirectProtocol.to_string(),
            "DirectProtocol"
        );
        assert_eq!(PowerSignalSource::EnergyMeter.to_string(), "EnergyMeter");
        assert_eq!(PowerSignalSource::TcpProbe.to_string(), "TcpProbe");
        assert_eq!(
            PowerSignalSource::WolPacketSent.to_string(),
            "WolPacketSent"
        );
        assert_eq!(PowerSignalSource::Unknown.to_string(), "Unknown");
    }

    #[test]
    fn power_profile_evaluate_energy_standby_and_active_thresholds() {
        let profile = PowerProfile::new()
            .with_standby_power_watts(15.0)
            .with_active_power_watts(30.0);

        // Smart plug reports 10W -> Standby with High confidence.
        let standby_reading = EnergyMeterReading::new(10.0)
            .with_voltage(120.0)
            .with_current(0.08);
        let eval_standby = profile.evaluate_energy(&standby_reading);
        assert_eq!(eval_standby.state, PowerState::Standby);
        assert_eq!(eval_standby.confidence, ConfidenceScore::High);
        assert_eq!(eval_standby.source, PowerSignalSource::EnergyMeter);

        // Smart plug reports 120W -> On with High confidence.
        let active_reading = EnergyMeterReading::new(120.0);
        let eval_active = profile.evaluate_energy(&active_reading);
        assert_eq!(eval_active.state, PowerState::On);
        assert_eq!(eval_active.confidence, ConfidenceScore::High);
        assert_eq!(eval_active.source, PowerSignalSource::EnergyMeter);

        // Intermediate reading 20W -> On with Medium confidence.
        let intermediate_reading = EnergyMeterReading::new(20.0);
        let eval_inter = profile.evaluate_energy(&intermediate_reading);
        assert_eq!(eval_inter.state, PowerState::On);
        assert_eq!(eval_inter.confidence, ConfidenceScore::Medium);
        assert_eq!(eval_inter.source, PowerSignalSource::EnergyMeter);
    }

    #[test]
    fn power_evaluator_tracks_state_and_sources() {
        let profile = PowerProfile::new().with_probe_port(8001);
        let mut evaluator = PowerEvaluator::new(profile);

        assert_eq!(evaluator.current_state(), EvaluatedPowerState::unknown());

        // WoL sent -> Low confidence
        let wol_eval = evaluator.record_wol_sent();
        assert_eq!(wol_eval.state, PowerState::On);
        assert_eq!(wol_eval.confidence, ConfidenceScore::Low);
        assert_eq!(wol_eval.source, PowerSignalSource::WolPacketSent);
        assert_eq!(evaluator.current_state(), wol_eval);

        // Port responds -> Medium confidence
        let probe_eval = evaluator.record_tcp_probe(PowerState::On);
        assert_eq!(probe_eval.state, PowerState::On);
        assert_eq!(probe_eval.confidence, ConfidenceScore::Medium);
        assert_eq!(probe_eval.source, PowerSignalSource::TcpProbe);

        // Direct protocol says On -> High confidence
        let direct_eval = evaluator.record_direct_protocol(PowerState::On);
        assert_eq!(direct_eval.state, PowerState::On);
        assert_eq!(direct_eval.confidence, ConfidenceScore::High);
        assert_eq!(direct_eval.source, PowerSignalSource::DirectProtocol);

        // Smart plug reports 10W -> Standby High confidence
        let meter_eval = evaluator.record_energy_reading(&EnergyMeterReading::new(10.0));
        assert_eq!(meter_eval.state, PowerState::Standby);
        assert_eq!(meter_eval.confidence, ConfidenceScore::High);
        assert_eq!(meter_eval.source, PowerSignalSource::EnergyMeter);
    }
}
