//! The [`SsdpDiscovery`] trait with its C1 diagnostics, and its implementation
//! for [`SsdpScanner`].

use async_trait::async_trait;

use crate::client::SsdpScanner;
use crate::error::Result;
use crate::payload::SsdpMsearchResponse;

/// What the `M-SEARCH` answer window saw, as counts.
///
/// This is the C1 distinction a mobile device needs: multicast dropped by the
/// OS with zero replies arriving looks exactly like an empty network if the
/// only answer is a list. The counts are the raw observation, and
/// [`DiscoveryOutcome`] turns them into the decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscoveryDiagnostics {
    /// Every datagram that reached the socket during the reply window,
    /// including malformed and duplicate ones.
    pub total_datagrams_received: usize,
    /// Distinct `USN` headers observed across those datagrams.
    pub unique_usns: usize,
}

impl DiscoveryDiagnostics {
    /// The mobile-commissioning C1 signal: the subnet answered nothing at all.
    ///
    /// A network full of displays the OS never let the socket hear, and a
    /// network with no displays, are different facts with this same count —
    /// but a *nonzero* count is always "multicast works, keep the answers".
    #[must_use]
    pub fn is_multicast_silence(&self) -> bool {
        self.total_datagrams_received == 0
    }
}

/// What one discovery pass over SSDP concluded.
///
/// [`DiscoveryOutcome::MulticastSilence`] is deliberately not an error and not
/// an empty `Vec`: a bare empty list asserts "no displays answered", while
/// silence asserts "no datagrams arrived" — a different fact on Android, where
/// the OS can drop multicast on hotspot networks, the notify listener needs
/// re-joining, and the caller's next step is the bounded TCP sweep, not a
/// shrug. This is what `02-playbook.md` calls the C1 signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryOutcome {
    /// Datagrams arrived (even if none parsed into a usable response).
    Found {
        /// The parsed, deduplicated responses.
        responses: Vec<SsdpMsearchResponse>,
        /// The raw counts behind them.
        diagnostics: DiscoveryDiagnostics,
    },
    /// Zero datagrams in the whole reply window (C1: multicast silence).
    MulticastSilence(DiscoveryDiagnostics),
}

impl DiscoveryOutcome {
    /// The deduplicated responses, empty under silence.
    #[must_use]
    pub fn responses(&self) -> &[SsdpMsearchResponse] {
        match self {
            Self::Found { responses, .. } => responses,
            Self::MulticastSilence(_) => &[],
        }
    }

    /// The counts the pass recorded.
    #[must_use]
    pub fn diagnostics(&self) -> &DiscoveryDiagnostics {
        match self {
            Self::Found { diagnostics, .. } | Self::MulticastSilence(diagnostics) => diagnostics,
        }
    }
}

/// Object-safe SSDP discovery interface.
///
/// This is the trait the workspace facade depends on, so it must stay
/// dyn-compatible; that is why it is annotated with `#[async_trait]`.
///
/// The return type is [`DiscoveryOutcome`] rather than a bare response list so
/// an implementor cannot bury the multicast-silence fact inside an
/// indistinguishable empty vector.
#[async_trait]
pub trait SsdpDiscovery: Send + Sync {
    /// Discover SSDP devices on the local network, with answer-window
    /// diagnostics.
    async fn discover(&self) -> Result<DiscoveryOutcome>;
}

#[async_trait]
impl SsdpDiscovery for SsdpScanner {
    async fn discover(&self) -> Result<DiscoveryOutcome> {
        SsdpScanner::msearch_outcome(
            self,
            crate::types::SSDP_ALL_DEVICES,
            crate::client::DEFAULT_MX,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostics(datagrams: usize, usns: usize) -> DiscoveryDiagnostics {
        DiscoveryDiagnostics {
            total_datagrams_received: datagrams,
            unique_usns: usns,
        }
    }

    #[test]
    fn silence_is_zero_datagrams_not_zero_responses() {
        // Two malformed datagrams in, zero usable responses out — the multicast
        // path works, so this is Found, not silence.
        assert!(!diagnostics(2, 0).is_multicast_silence());
        assert!(diagnostics(0, 0).is_multicast_silence());
    }

    #[test]
    fn outcomes_answer_the_two_questions_the_caller_has() {
        let silence = DiscoveryOutcome::MulticastSilence(diagnostics(0, 0));
        assert_eq!(silence.responses().len(), 0);
        assert!(silence.diagnostics().is_multicast_silence());

        let found = DiscoveryOutcome::Found {
            responses: Vec::new(),
            diagnostics: diagnostics(5, 3),
        };
        assert_eq!(found.diagnostics().unique_usns, 3);
        assert!(!found.diagnostics().is_multicast_silence());
    }
}
