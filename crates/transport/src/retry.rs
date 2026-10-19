//! Deciding when a failed connection is worth retrying on another port.
//!
//! Both vendor protocols changed ports once: Samsung moved from `ws://:8001` to
//! `wss://:8002`, LG from `ws://:3000` to `wss://:3001`. A client that hard-codes
//! either one works on half the fleet and looks "broken" on the other half.
//!
//! The rule this module encodes is narrow on purpose: **only a transport-level
//! refusal justifies trying the other port.** A certificate that does not match its
//! pin, or a device that speaks a different protocol than expected, is not a reason
//! to retry — it is a reason to stop, because the fallback would be *less* secure or
//! simply wrong.

/// Whether `error` means "nothing is listening there", as opposed to "something
/// answered and we did not like what it said".
///
/// # Examples
///
/// ```
/// use std::io::{Error, ErrorKind};
/// use tv_rs_transport::should_try_next_port;
///
/// let refused = tokio_tungstenite::tungstenite::Error::Io(Error::from(ErrorKind::ConnectionRefused));
/// assert!(should_try_next_port(&refused));
///
/// let answered = tokio_tungstenite::tungstenite::Error::Io(Error::new(
///     ErrorKind::InvalidData,
///     "certificate fingerprint mismatch",
/// ));
/// assert!(!should_try_next_port(&answered), "a mismatch is an answer, not a silence");
/// ```
#[must_use]
pub fn should_try_next_port(error: &tokio_tungstenite::tungstenite::Error) -> bool {
    use std::io::ErrorKind;
    use tokio_tungstenite::tungstenite::Error;

    match error {
        Error::Io(io) => matches!(
            io.kind(),
            ErrorKind::ConnectionRefused
                | ErrorKind::ConnectionReset
                | ErrorKind::ConnectionAborted
                | ErrorKind::AddrNotAvailable
                | ErrorKind::NotFound
                | ErrorKind::TimedOut
        ),
        // TLS and protocol failures are answers, not silences.
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Error, ErrorKind};

    use tokio_tungstenite::tungstenite::error::ProtocolError;

    use super::should_try_next_port;

    #[test]
    fn a_refused_connection_is_worth_retrying_elsewhere() {
        for kind in [
            ErrorKind::ConnectionRefused,
            ErrorKind::TimedOut,
            ErrorKind::AddrNotAvailable,
        ] {
            let error = tokio_tungstenite::tungstenite::Error::Io(Error::from(kind));
            assert!(should_try_next_port(&error), "{kind:?}");
        }
    }

    #[test]
    fn an_answer_is_never_downgraded() {
        let tls = tokio_tungstenite::tungstenite::Error::Io(Error::new(
            ErrorKind::InvalidData,
            "certificate fingerprint mismatch",
        ));
        assert!(!should_try_next_port(&tls));

        let protocol = tokio_tungstenite::tungstenite::Error::Protocol(
            ProtocolError::ResetWithoutClosingHandshake,
        );
        assert!(!should_try_next_port(&protocol));
    }
}
