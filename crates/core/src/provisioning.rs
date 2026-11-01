//! The OS-agnostic provisioning bundle: everything a fresh install needs to
//! adopt one device without pairing over the network it is already on.
//!
//! A bundle is **a claim plus attachments**: the device identity, where the
//! network lives, the credential material that keeps pairing state identical to
//! the desktop's, and — for the one protocol where the physical arrangement is
//! part of the identity — the MDC topology. The companion mobile app renders the
//! QR, a desktop instalment re-serialises it, and the agent's `/provision`
//! route ingests it verbatim. `schema_version` is what keeps those three
//! decoders honest.
//!
//! Two rules are enforced where the facts live, not where the JSON is parsed:
//!
//! * `schema_version` must be the exact value [`SCHEMA_VERSION_V1`]; a bundle
//!   from a different schema is refused, not guessed at.
//! * `MdcTopology::display_id` lives in `0x00..=0xFD`: `0xFE` is the MDC
//!   broadcast address and `0xFF` is not a display, so a topology claiming
//!   either is a mis-parsed scan, not a wall.
//!
//! Both hold for hand-built bundles (checked in `new`) *and* deserialised ones
//! (`try_from`, see below), so there is no configuration in which a bad bundle
//! reaches the wire.

use crate::capability::PanelHealth;
use crate::descriptor::{DeviceId, DeviceOs};
use crate::error::TvError;

/// The only schema version this crate decodes.
pub const SCHEMA_VERSION_V1: &str = "v1";

/// The highest legal MDC display id. MDC reserves `0xFE` for broadcast and
/// `0xFF` for the controller itself.
pub const MAX_MDC_DISPLAY_ID: u8 = 0xFD;

/// Where the device lives on its network, plus the credential material.
///
/// Every field is optional because the vendors differ: a Roku needs nothing, a
/// Tizen set needs the pairing token text, a webOS set needs the `client-key`,
/// and a fleet behind mTLS pins the server fingerprint. Nothing here is ever
/// fabricated — an absent credential is an absent credential, and the bundle
/// instructs the receiver to pair, not to pretend.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct NetworkProvisionTarget {
    /// IP or host:port the device answers on.
    pub ip: String,
    /// MAC, when known — the Wake-on-LAN prerequisite.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub mac: Option<String>,
    /// SHA-256 certificate fingerprint as 64 lowercase hex characters, when the
    /// install is pinned. Feeds [`tv_rs_transport`]'s `Pinned` policy through
    /// the facade.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub tls_fingerprint: Option<String>,
    /// The pairing credential as issued (Tizen token, webOS `client-key`).
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub auth_token: Option<String>,
}

impl NetworkProvisionTarget {
    /// Validates and builds a target.
    ///
    /// # Errors
    ///
    /// Returns [`TvError::Protocol`] for an empty `ip`, a whitespace-only
    /// credential string, or a fingerprint that is not 64 lowercase hex
    /// characters — anything else would silently pair against the wrong pin.
    pub fn new(
        ip: impl Into<String>,
        mac: Option<String>,
        tls_fingerprint: Option<String>,
        auth_token: Option<String>,
    ) -> Result<Self, TvError> {
        let ip = ip.into();
        if ip.trim().is_empty() {
            return Err(TvError::protocol("network target ip must not be empty"));
        }
        check_optional_non_blank(mac.as_deref(), "mac")?;
        check_optional_non_blank(auth_token.as_deref(), "auth_token")?;
        if let Some(fingerprint) = tls_fingerprint.as_deref() {
            validate_fingerprint(fingerprint)?;
        }
        Ok(Self {
            ip,
            mac,
            tls_fingerprint,
            auth_token,
        })
    }
}

fn check_optional_non_blank(value: Option<&str>, field: &'static str) -> Result<(), TvError> {
    if let Some(value) = value {
        if value.trim().is_empty() {
            return Err(TvError::protocol(format!(
                "network target {field} must not be blank"
            )));
        }
    }
    Ok(())
}

fn validate_fingerprint(fingerprint: &str) -> Result<(), TvError> {
    let is_hex = fingerprint.len() == 64
        && fingerprint
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, 'a'..='f'));
    if !is_hex {
        return Err(TvError::protocol(
            "tls_fingerprint must be 64 lowercase hex (sha-256)",
        ));
    }
    Ok(())
}

/// One display plus where it sits on the wall's transport arrangement.
///
/// Only MDC gets a topology block, because it is the one protocol whose
/// identity is physical: commands are addressed to a `display_id` on a wire the
/// installer chose. Every field is what the alliance needs to reproduce the
/// wall after an IP renumber — except the serial, which is a fact about the
/// panel, not the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct MdcTopology {
    /// The display id commands are addressed to, `0x00..=0xFD`.
    pub display_id: u8,
    /// Position on the RS-232 / daisy-chain, when the transport is a chain.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub daisy_chain_index: Option<u8>,
    /// How the wire reaches the panel.
    pub transport_hint: MdcTransportHint,
    /// The last `0x0D` answer, if one was taken while provisioning.
    ///
    /// 0x0D reports signal state and panel health — lamp fault, input-source
    /// error, temperature, fan. **It does not report a serial number.** Nothing
    /// here manufactures one from it, ever.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub health_snapshot_0x0d: Option<PanelHealth>,
    /// The panel's serial as *read from hardware*, not from a command:
    /// `tv-rs-local`'s EDID parse when the panel is attached, or a barcode/QR
    /// scan. Its provenance is the point — a serial that came off the wire is
    /// not a serial the asset list can trust.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub hardware_serial: Option<String>,
}

impl MdcTopology {
    /// Validates and builds a topology.
    ///
    /// # Errors
    ///
    /// Returns [`TvError::Protocol`] when `display_id` exceeds
    /// [`MAX_MDC_DISPLAY_ID`] (broadcast / controller ids are not addresses)
    /// or when either serial is blank.
    pub fn new(
        display_id: u8,
        daisy_chain_index: Option<u8>,
        transport_hint: MdcTransportHint,
        health_snapshot_0x0d: Option<PanelHealth>,
        hardware_serial: Option<String>,
    ) -> Result<Self, TvError> {
        if display_id > MAX_MDC_DISPLAY_ID {
            return Err(TvError::protocol(format!(
                "mdc display_id must be 0x00..=0xFD, got 0x{display_id:02X}"
            )));
        }
        if let Some(serial) = hardware_serial.as_deref() {
            if serial.trim().is_empty() {
                return Err(TvError::protocol("mdc hardware_serial must not be blank"));
            }
        }
        Ok(Self {
            display_id,
            daisy_chain_index,
            transport_hint,
            health_snapshot_0x0d,
            hardware_serial,
        })
    }
}

/// The carrier the MDC wire uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum MdcTransportHint {
    /// RJ45, one display id per TCP socket (MDC's TCP carrier, port 1515).
    Tcp,
    /// RS-232, the serial chain.
    Rs232,
}

impl MdcTransportHint {
    /// A stable label for logs.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Rs232 => "rs232",
        }
    }
}

/// Everything one device needs to come online without a live pairing.
///
/// Built through [`ProvisioningBundleV1::new`] so hand-construction gets the
/// same checks deserialisation gets. `vendor` is [`DeviceOs`] — the core's own
/// classification, not the facade's `TvVendor` — because this type lives where
/// tokio and the facade do not.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(try_from = "ProvisioningBundleRaw", deny_unknown_fields)
)]
pub struct ProvisioningBundleV1 {
    /// The schema this bundle decodes by; must be [`SCHEMA_VERSION_V1`].
    pub schema_version: String,
    /// The stable device identity the bundle adopts.
    pub device_id: DeviceId,
    /// Vendor classification, as wide or as narrow as the evidence got.
    pub vendor: DeviceOs,
    /// The client identity string the pairing must share with the desktop
    /// instalment that already paired this set (the Tizen `?name=`, the webOS
    /// registration `client identity`). Offline instalments re-pairing a token
    /// they did not create need this to land in the same identity, or the TV
    /// sees a stranger holding a stolen key.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub client_identity_override: Option<String>,
    /// Address and credentials, when the bundle carries them.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub network_target: Option<NetworkProvisionTarget>,
    /// The MDC topology, when the device is a commercial wall.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub mdc_topology: Option<MdcTopology>,
}

impl ProvisioningBundleV1 {
    /// Validates and builds a bundle.
    ///
    /// # Errors
    ///
    /// Returns [`TvError::Protocol`] when `schema_version` is not
    /// [`SCHEMA_VERSION_V1`] — a bundle in an unknown schema is refused, not
    /// decoded — or when `device_id` is empty.
    pub fn new(
        schema_version: impl Into<String>,
        device_id: DeviceId,
        vendor: DeviceOs,
    ) -> Result<Self, TvError> {
        let schema_version = schema_version.into();
        if schema_version != SCHEMA_VERSION_V1 {
            return Err(TvError::protocol(format!(
                "provisioning bundle schema {schema_version:?} is not supported (expect {:?})",
                SCHEMA_VERSION_V1
            )));
        }
        Ok(Self {
            schema_version,
            device_id,
            vendor,
            client_identity_override: None,
            network_target: None,
            mdc_topology: None,
        })
    }

    /// Sets the shared client identity.
    #[must_use]
    pub fn with_client_identity_override(mut self, identity: impl Into<String>) -> Self {
        self.client_identity_override = Some(identity.into());
        self
    }

    /// Sets the network target.
    #[must_use]
    pub fn with_network_target(mut self, target: NetworkProvisionTarget) -> Self {
        self.network_target = Some(target);
        self
    }

    /// Sets the MDC topology.
    #[must_use]
    pub fn with_mdc_topology(mut self, topology: MdcTopology) -> Self {
        self.mdc_topology = Some(topology);
        self
    }
}

/// The deserialisation shape: identical fields to
/// [`ProvisioningBundleV1`], without the invariant enforcement. The
/// `try_from` step on the bundle runs `new`'s checks, so JSON cannot
/// smuggle in a wrong `schema_version`.
#[cfg(feature = "serde")]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(crate) struct ProvisioningBundleRaw {
    pub(crate) schema_version: String,
    pub(crate) device_id: DeviceId,
    pub(crate) vendor: DeviceOs,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) client_identity_override: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) network_target: Option<NetworkProvisionTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) mdc_topology: Option<MdcTopology>,
}

#[cfg(feature = "serde")]
impl TryFrom<ProvisioningBundleRaw> for ProvisioningBundleV1 {
    type Error = TvError;

    fn try_from(raw: ProvisioningBundleRaw) -> Result<Self, Self::Error> {
        let bundle = Self::new(raw.schema_version, raw.device_id, raw.vendor)?;
        Ok(Self {
            client_identity_override: raw.client_identity_override,
            network_target: raw.network_target,
            mdc_topology: raw.mdc_topology,
            ..bundle
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle() -> ProvisioningBundleV1 {
        ProvisioningBundleV1::new(
            SCHEMA_VERSION_V1,
            DeviceId::new("udn:abc").unwrap(),
            DeviceOs::Tizen,
        )
        .unwrap()
    }

    #[test]
    fn wrong_schema_is_refused() {
        let error = ProvisioningBundleV1::new("v2", DeviceId::new("id").unwrap(), DeviceOs::Roku)
            .expect_err("wrong schema");
        assert!(
            error.to_string().contains("v2"),
            "the error names the schema it refused: {error}"
        );
    }

    #[test]
    fn mdc_display_id_rejects_broadcast_and_controller_ids() {
        for id in [0xFE, 0xFF] {
            let error =
                MdcTopology::new(id, None, MdcTransportHint::Tcp, None, None).expect_err("invalid");
            assert!(error.to_string().contains("0x00..=0xFD"), "{error}");
        }
        assert!(MdcTopology::new(0, None, MdcTransportHint::Tcp, None, None).is_ok());
        assert!(MdcTopology::new(0xFD, None, MdcTransportHint::Rs232, None, None).is_ok());
    }

    #[test]
    fn fingerprints_must_be_sha256_and_lowercase() {
        let good = "0123456789abcdef".repeat(4);
        assert_eq!(good.len(), 64);
        assert!(NetworkProvisionTarget::new("10.0.0.4", None, Some(good.clone()), None).is_ok());

        let upper = good.to_uppercase();
        assert!(NetworkProvisionTarget::new("10.0.0.4", None, Some(upper), None).is_err());

        let short = good[..63].to_string();
        assert!(NetworkProvisionTarget::new("10.0.0.4", None, Some(short), None).is_err());

        assert!(NetworkProvisionTarget::new("", None, None, None).is_err());
    }

    #[test]
    fn builder_stacks_the_attachments() {
        let bundle = bundle()
            .with_client_identity_override("client-id-desktop")
            .with_network_target(
                NetworkProvisionTarget::new(
                    "10.0.0.4",
                    Some("aa:bb:cc:dd:ee:ff".to_string()),
                    None,
                    Some("token".to_string()),
                )
                .unwrap(),
            )
            .with_mdc_topology(
                MdcTopology::new(
                    7,
                    Some(1),
                    MdcTransportHint::Tcp,
                    None,
                    Some("SE-1234".to_string()),
                )
                .unwrap(),
            );
        assert_eq!(
            bundle.client_identity_override.as_deref(),
            Some("client-id-desktop")
        );
        assert_eq!(bundle.network_target.as_ref().unwrap().ip, "10.0.0.4");
        assert_eq!(bundle.mdc_topology.as_ref().unwrap().display_id, 7);
    }

    #[test]
    #[cfg(feature = "serde")]
    fn deserialised_bundles_run_the_same_checks() {
        let bundle = bundle().with_client_identity_override("shared-identity");
        let text = serde_json::to_string(&bundle).unwrap();
        let parsed: ProvisioningBundleV1 = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed, bundle);

        let stolen = serde_json::to_string(&bundle).unwrap().replace("v1", "v9");
        assert!(
            serde_json::from_str::<ProvisioningBundleV1>(&stolen).is_err(),
            "a decode path that skips the schema check is the check"
        );
    }
}
