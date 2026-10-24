//! Error types for `tv-rs-discovery`.

/// Convenience result alias using [`DiscoveryError`].
pub type Result<T> = std::result::Result<T, DiscoveryError>;

/// Everything that can go wrong during discovery or descriptor parsing.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    /// The requested capability is not implemented yet.
    #[error(transparent)]
    Unsupported(#[from] tv_rs_transport::Unsupported),
    /// An I/O error occurred while talking to the network.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// An HTTP error occurred while fetching a device descriptor.
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    /// The XML document could not be parsed.
    #[error("xml parse error: {0}")]
    Xml(String),
    /// The payload was syntactically valid but did not contain the expected data.
    #[error("malformed payload: {0}")]
    Malformed(String),
    /// The probe went out and not one datagram came back.
    ///
    /// Deliberately not an empty list: an empty list means the network answered and
    /// no display was among the replies. Silence means nothing answered at all, and
    /// the two need different instructions — silence is either "nothing is listening"
    /// (no displays, or an access point that filters multicast) or "the local network
    /// is not reachable" (a permission the app has not been granted, or a firewall).
    /// Neither is knowable from here, so the message names both rather than asserting
    /// one; the point is that a caller must not read silence as "no displays".
    #[error(
        "the probe for {target} was silent: no datagram came back — either nothing is \
         listening or the local network is unreachable (a permission or a firewall)"
    )]
    SilentProbe {
        /// What was being probed for, e.g. `ssdp:all` — enough to name the query
        /// that went unanswered.
        target: String,
    },
}

#[cfg(test)]
mod tests {
    use super::DiscoveryError;

    /// Silence must not read as a permission problem when it might equally be a
    /// network with nothing on it — the caller needs the distinction, not a guess.
    #[test]
    fn a_silent_probe_names_both_explanations_and_the_query() {
        let error = DiscoveryError::SilentProbe {
            target: "ssdp:all".to_string(),
        };
        let message = error.to_string();
        assert!(message.contains("ssdp:all"), "{message}");
        assert!(message.contains("nothing is listening"), "{message}");
        assert!(message.contains("permission"), "{message}");
        // It must not be an empty list's wording.
        assert!(!message.contains("no devices"), "{message}");
    }
}
