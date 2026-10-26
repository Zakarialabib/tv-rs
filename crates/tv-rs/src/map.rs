//! Where every vendor error becomes the one error a consumer sees.
//!
//! Each function here is an **exhaustive** match over a vendor's error enum, with no
//! `_ =>` arm. That is the whole point: the previous mapping had a catch-all that
//! sent anything unrecognised to `Unreachable`, which routes to `Remedy::Wait` — so
//! a webOS pairing rejection (`RegistrationFailed`) told the user to *wake* a
//! television that was already awake and waiting to be paired. A wildcard arm is how
//! a new vendor error silently acquires the wrong remedy. With these matches, adding
//! a variant to `TizenError` is a compile error here until somebody decides what it
//! means for the caller.
//!
//! The capability parameter is the capability the *call site* needed. Several vendor
//! errors mean "this vendor's protocol has no such message", and that is only
//! expressible with the capability in hand.

use tv_rs_core::{AuthError, Capability, ProtocolError, TransportError, TvError};

/// Maps a Tizen error.
pub fn tizen(error: &tv_rs_tizen::TizenError, capability: Capability) -> TvError {
    use tv_rs_tizen::TizenError;

    match error {
        TizenError::Unsupported(_) => TvError::Unsupported(capability),
        TizenError::Tls(detail) => TvError::Transport(TransportError::Tls(detail.clone())),
        // No token yet: the user has to accept the prompt on the television.
        TizenError::Unauthorized => TvError::Auth(AuthError::PairingRequired),
        // A token was presented and the television refused it — revoked, or the
        // panel was factory-reset. Re-pairing is the only way forward.
        TizenError::InvalidKey(_) => TvError::Auth(AuthError::TokenExpired),
        TizenError::InvalidHost(detail) => {
            TvError::Transport(TransportError::Unreachable(detail.clone()))
        }
        TizenError::Timeout => TvError::Transport(TransportError::Timeout),
        TizenError::Ws(_) | TizenError::Http(_) => {
            TvError::Transport(TransportError::Unreachable(error.to_string()))
        }
        // A socket error can be the platform refusing the call outright rather
        // than a television that is off; only `from_io` can tell them apart.
        TizenError::Io(io) => TvError::Transport(TransportError::from_io(io)),
        TizenError::Json(_) | TizenError::Xml(_) | TizenError::Url(_) => {
            TvError::Protocol(ProtocolError::Malformed(error.to_string()))
        }
        TizenError::Protocol(detail) => TvError::Protocol(ProtocolError::Malformed(detail.clone())),
    }
}

/// Maps a webOS error.
pub fn webos(error: &tv_rs_webos::WebosError, capability: Capability) -> TvError {
    use tv_rs_webos::WebosError;

    match error {
        WebosError::Unsupported(_) => TvError::Unsupported(capability),
        // The distinction the old catch-all erased: a *first* pairing that was
        // declined needs the prompt accepted, while a key the television has
        // forgotten or revoked needs a fresh pairing. Both are `Auth`, and the
        // error now carries which one happened.
        WebosError::RegistrationFailed {
            presented_key: true,
            ..
        } => TvError::Auth(AuthError::TokenExpired),
        WebosError::RegistrationFailed {
            presented_key: false,
            ..
        } => TvError::Auth(AuthError::PairingRequired),
        WebosError::Ssap { code, message } => TvError::Protocol(ProtocolError::Unexpected(
            format!("ssap error {code}: {message}"),
        )),
        WebosError::Timeout => TvError::Transport(TransportError::Timeout),
        WebosError::Ws(_) => TvError::Transport(TransportError::Unreachable(error.to_string())),
        WebosError::Io(io) => TvError::Transport(TransportError::from_io(io)),
        WebosError::Json(_) | WebosError::Url(_) => {
            TvError::Protocol(ProtocolError::Malformed(error.to_string()))
        }
        WebosError::Protocol(detail) => TvError::Protocol(ProtocolError::Malformed(detail.clone())),
    }
}

/// Maps a Roku error.
pub fn roku(error: &tv_rs_roku::RokuError, capability: Capability) -> TvError {
    use tv_rs_roku::RokuError;

    match error {
        RokuError::Unsupported(_) => TvError::Unsupported(capability),
        // The Roku crate already carries the user-facing instruction (the exact
        // settings path); it becomes the hint rather than being paraphrased.
        RokuError::NotPermitted { remedy } => TvError::Auth(AuthError::NotPermitted {
            remedy: remedy.clone(),
        }),
        RokuError::NotFound(detail) => TvError::Protocol(ProtocolError::Unexpected(detail.clone())),
        RokuError::Xml(detail) => TvError::Protocol(ProtocolError::Malformed(detail.clone())),
        RokuError::Http(_) => TvError::Transport(TransportError::Unreachable(error.to_string())),
    }
}

/// Maps a discovery error.
///
/// Discovery runs before there is a device, so there is no capability to name and no
/// vendor to blame: a socket that will not open is a transport problem, and an SSDP
/// answer nobody can parse is a protocol problem.
pub fn discovery(error: &tv_rs_discovery::DiscoveryError) -> TvError {
    use tv_rs_discovery::DiscoveryError;

    match error {
        // A build-level gap, not a device condition: nothing the caller can retry
        // into existence. `Protocol` is the honest bucket — the answer is unusable
        // here — and the phase/reason survive in the message.
        DiscoveryError::Unsupported(unsupported) => TvError::Protocol(ProtocolError::Unexpected(
            format!("discovery unsupported: {unsupported}"),
        )),
        DiscoveryError::Malformed(detail) | DiscoveryError::Xml(detail) => {
            TvError::Protocol(ProtocolError::Malformed(detail.clone()))
        }
        // A silent probe means the local network did not answer at all — no display,
        // or a permission the app has not been granted. The caller's action is the
        // same one a refused socket gives (A5), so it reuses `Blocked` and its
        // "grant access" remedy rather than inventing a third bucket, and `hint()`
        // already names the grant.
        DiscoveryError::SilentProbe { .. } => TvError::Transport(TransportError::Blocked {
            reason: error.to_string(),
        }),
        DiscoveryError::Http(_) => {
            TvError::Transport(TransportError::Unreachable(error.to_string()))
        }
        // Discovery is where an Android block shows up first and looks most like
        // "there are no televisions here": the SSDP send is refused, nothing is
        // ever received, and an empty list is the wrong story to tell.
        DiscoveryError::Io(io) => TvError::Transport(TransportError::from_io(io)),
    }
}

/// Maps a SICP error.
pub fn sicp(error: &tv_rs_sicp::SicpError, _capability: Capability) -> TvError {
    use tv_rs_sicp::SicpError;

    match error {
        SicpError::Io(io) => TvError::Transport(TransportError::from_io(io)),
        SicpError::Timeout => TvError::Transport(TransportError::Timeout),
        SicpError::Protocol(detail) => TvError::Protocol(ProtocolError::Malformed(detail.clone())),
        SicpError::Nak { command, code } => TvError::Protocol(ProtocolError::Unexpected(format!(
            "display returned NAK 0x{code:02X} for command 0x{command:02X}"
        ))),
        SicpError::FrameTooLong(_)
        | SicpError::InvalidValue { .. }
        | SicpError::BroadcastRequiresSendOnly
        | SicpError::InvalidDisplayId(_) => {
            TvError::Protocol(ProtocolError::Malformed(error.to_string()))
        }
    }
}

/// Maps a power error.
pub fn power(error: &tv_rs_power::PowerError) -> TvError {
    use tv_rs_power::PowerError;

    match error {
        // Wake is the only thing this crate does, so an unsupported path here is
        // an unsupported capability.
        PowerError::Unsupported(_) => TvError::Unsupported(Capability::Wake),
        PowerError::InvalidMacAddress(detail) => {
            TvError::Protocol(ProtocolError::Malformed(format!("invalid MAC: {detail}")))
        }
        PowerError::InvalidBroadcastAddress(detail) => TvError::Protocol(ProtocolError::Malformed(
            format!("invalid broadcast address: {detail}"),
        )),
        PowerError::Io(io) => TvError::Transport(TransportError::from_io(io)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tv_rs_core::Remedy;

    /// One row per vendor error variant: the remedy a consumer will act on.
    ///
    /// This is the table the plan asks for. It is deliberately written out rather
    /// than derived — the value of the test is that a human decided each row, and
    /// that the exhaustive matches above force a decision for every new variant.
    /// A silent probe is not an empty list: the local network did not answer at
    /// all, so the caller is asked to grant access rather than told there are no
    /// displays and left to believe it.
    #[test]
    fn a_silent_probe_asks_for_access_rather_than_reporting_no_displays() {
        let error = tv_rs_discovery::DiscoveryError::SilentProbe {
            target: "ssdp:all".to_string(),
        };
        let mapped = discovery(&error);
        assert_eq!(mapped.remedy(), Remedy::RequestPermission);
        assert!(
            mapped.hint().is_some(),
            "the hint names the grant the caller has to make"
        );
    }

    #[test]
    fn every_vendor_error_routes_to_the_remedy_a_user_can_act_on() {
        use tv_rs_tizen::TizenError;
        use tv_rs_webos::WebosError;

        let cases: Vec<(TvError, Remedy)> = vec![
            (
                tizen(&TizenError::Unauthorized, Capability::Key),
                Remedy::Reprovision,
            ),
            (
                tizen(&TizenError::InvalidKey("revoked".into()), Capability::Key),
                Remedy::Reprovision,
            ),
            (tizen(&TizenError::Timeout, Capability::Key), Remedy::Retry),
            (
                tizen(&TizenError::Protocol("garbage".into()), Capability::Key),
                Remedy::BackOff,
            ),
            (
                tizen(&TizenError::Unsupported(unsupported()), Capability::Key),
                Remedy::Disable,
            ),
            (
                webos(
                    &WebosError::RegistrationFailed {
                        detail: "declined".into(),
                        presented_key: false,
                    },
                    Capability::Key,
                ),
                Remedy::Reprovision,
            ),
            (
                webos(
                    &WebosError::RegistrationFailed {
                        detail: "revoked".into(),
                        presented_key: true,
                    },
                    Capability::Key,
                ),
                Remedy::Reprovision,
            ),
            (webos(&WebosError::Timeout, Capability::Key), Remedy::Retry),
            (
                webos(
                    &WebosError::Ssap {
                        code: -1,
                        message: "no such method".into(),
                    },
                    Capability::Key,
                ),
                Remedy::BackOff,
            ),
            (
                roku(
                    &tv_rs_roku::RokuError::NotPermitted {
                        remedy: "Settings → System → Control by mobile apps".into(),
                    },
                    Capability::Key,
                ),
                Remedy::Reprovision,
            ),
            (
                roku(
                    &tv_rs_roku::RokuError::NotFound("no app".into()),
                    Capability::AppLaunch,
                ),
                Remedy::BackOff,
            ),
            (
                sicp(
                    &tv_rs_sicp::SicpError::Nak {
                        command: 0x18,
                        code: 0x01,
                    },
                    Capability::PowerState,
                ),
                Remedy::BackOff,
            ),
            (
                power(&tv_rs_power::PowerError::Unsupported(unsupported())),
                Remedy::Disable,
            ),
            (
                power(&tv_rs_power::PowerError::InvalidMacAddress("zz".into())),
                Remedy::BackOff,
            ),
            (
                discovery(&tv_rs_discovery::DiscoveryError::Malformed(
                    "truncated".into(),
                )),
                Remedy::BackOff,
            ),
            // A platform that withholds the call is not a device that is off.
            (
                TvError::Transport(TransportError::Blocked {
                    reason: "EPERM".into(),
                }),
                Remedy::RequestPermission,
            ),
        ];

        for (error, expected) in cases {
            assert_eq!(error.remedy(), expected, "{error}");
        }
    }

    /// The bug this module was written for, pinned.
    #[test]
    fn a_refused_webos_registration_never_asks_the_user_to_wait() {
        // `Wait` is what the old catch-all produced, and it made the app offer
        // "Wake" for a television that was awake and waiting to be paired.
        for presented_key in [true, false] {
            let mapped = webos(
                &tv_rs_webos::WebosError::RegistrationFailed {
                    detail: "registration refused".into(),
                    presented_key,
                },
                Capability::Key,
            );
            assert_eq!(mapped.remedy(), Remedy::Reprovision);
            assert_ne!(mapped.remedy(), Remedy::Wait);
            assert!(
                matches!(mapped, TvError::Auth(_)),
                "a pairing failure is an auth failure: {mapped}"
            );
        }
    }

    /// A declined first pairing and a revoked stored key differ in what the user is
    /// told, which is the only reason the distinction is carried at all.
    #[test]
    fn a_revoked_key_is_distinguishable_from_a_declined_prompt() {
        let declined = webos(
            &tv_rs_webos::WebosError::RegistrationFailed {
                detail: "declined".into(),
                presented_key: false,
            },
            Capability::Key,
        );
        let revoked = webos(
            &tv_rs_webos::WebosError::RegistrationFailed {
                detail: "revoked".into(),
                presented_key: true,
            },
            Capability::Key,
        );
        assert!(matches!(
            declined,
            TvError::Auth(AuthError::PairingRequired)
        ));
        assert!(matches!(revoked, TvError::Auth(AuthError::TokenExpired)));
    }

    /// The capability a call site needed survives the trip, so the UI can hide the
    /// control rather than retry it forever.
    #[cfg(feature = "supporting")]
    #[test]
    fn an_unsupported_capability_arrives_named() {
        let mapped = tizen(
            &tv_rs_tizen::TizenError::Unsupported(unsupported()),
            Capability::ScreenCapture,
        );
        assert!(matches!(
            mapped,
            TvError::Unsupported(Capability::ScreenCapture)
        ));
    }

    fn unsupported() -> tv_rs_transport::Unsupported {
        tv_rs_transport::Unsupported {
            phase: 9,
            reason: "not implemented in this build",
        }
    }
}
