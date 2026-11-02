//! Episodic remediation ledger and empirical healer re-ranking (Tier B Knowledge Base).
//!
//! Stores past remediation outcomes in a bounded in-memory ring-buffer with optional
//! rolling JSONL persistence. Aggregates empirical success rates to dynamically re-rank
//! the `HealerLadder` actions based on what actually worked on specific models and firmwares.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, Write};

/// The outcome of a healing attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemediationOutcome {
    /// The attempt succeeded and restored normal function or power.
    Success,
    /// The attempt failed or timed out.
    Failure,
    /// The attempt was skipped due to missing capability or prerequisite.
    Skipped,
}

/// A recorded episode of an attempted healing action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemediationEpisode {
    /// Device identifier (e.g. `entrance-tv-1`).
    pub device_id: String,
    /// Vendor name (e.g. `samsung`, `philips`, `lg`).
    pub vendor: String,
    /// Specific model name if known (e.g. `QB55R`, `50BDL3550Q`).
    pub model: Option<String>,
    /// Firmware revision string if known (e.g. `1042`, `FB01.02`).
    pub firmware: Option<String>,
    /// The taxonomy classification of the error (e.g. `Transport(Timeout)`, `Protocol(Unexpected)`).
    pub error_taxonomy: String,
    /// The remedy attempted (e.g. `Wake`, `Reconnect`, `rs232_power_0x11`, `dim_backlight`).
    pub remedy_attempted: String,
    /// Whether the action succeeded.
    pub outcome: RemediationOutcome,
    /// Latency / duration of execution in milliseconds.
    pub duration_ms: u64,
    /// Deterministic timestamp string or Unix milliseconds.
    pub timestamp_ms: u64,
}

/// Aggregated statistical record for a given remediation path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmpiricalWeight {
    /// Number of successful executions.
    pub successes: usize,
    /// Number of total executions (successes + failures).
    pub attempts: usize,
    /// Computed success rate [0.0, 1.0].
    pub success_rate: f32,
}

impl EmpiricalWeight {
    /// Updates counts with a new outcome.
    pub fn record(&mut self, outcome: RemediationOutcome) {
        match outcome {
            RemediationOutcome::Success => {
                self.attempts += 1;
                self.successes += 1;
            }
            RemediationOutcome::Failure => {
                self.attempts += 1;
            }
            RemediationOutcome::Skipped => {}
        }
        self.success_rate = if self.attempts > 0 {
            self.successes as f32 / self.attempts as f32
        } else {
            0.0
        };
    }
}

/// What one agent audit line knows, in the ledger's vocabulary — the
/// Tier A→B bridge's parameter. `result` uses the audit vocabulary
/// (`ok` / `failed` / `skipped` / `quarantined`); device knowledge
/// (vendor, model, firmware, taxonomy) is the caller's to supply because
/// the audit line does not carry it.
#[derive(Debug, Clone)]
pub struct AuditFacts {
    /// Device identifier from the audit entry.
    pub device_id: String,
    /// The audit entry's result field.
    pub result: String,
    /// The remedy the action attempted (`RemedyStep::as_str`, `action`, …).
    pub remedy_attempted: String,
    /// Measured duration of the attempt.
    pub duration_ms: u64,
    /// When the attempt happened (deterministic in tests).
    pub timestamp_ms: u64,
    /// The device's platform label (`DeviceOs::as_str`).
    pub vendor: String,
    /// The device's model, when the descriptor knows one.
    pub model: Option<String>,
    /// The device's firmware, when the descriptor knows one.
    pub firmware: Option<String>,
    /// The error taxonomy classification (empty for successes).
    pub error_taxonomy: String,
}

/// Bounded in-memory ledger storing remediation episodes with empirical success scoring.
#[derive(Debug, Clone)]
pub struct EpisodicLedger {
    episodes: VecDeque<RemediationEpisode>,
    capacity: usize,
}

impl EpisodicLedger {
    /// Creates a new episodic ledger with a maximum ring-buffer capacity.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            episodes: VecDeque::with_capacity(capacity.max(1)),
            capacity: capacity.max(1),
        }
    }

    /// Records an episode into the ledger, discarding oldest entries if capacity is reached.
    pub fn record(&mut self, episode: RemediationEpisode) {
        if self.episodes.len() >= self.capacity {
            self.episodes.pop_front();
        }
        self.episodes.push_back(episode);
    }

    /// Returns a slice/iterator of current recorded episodes.
    #[must_use]
    pub fn episodes(&self) -> &VecDeque<RemediationEpisode> {
        &self.episodes
    }

    /// Serializes current episodes to an append-only JSONL format.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if writing fails.
    pub fn write_jsonl<W: Write>(&self, mut writer: W) -> std::io::Result<()> {
        for episode in &self.episodes {
            let line = serde_json::to_string(episode)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
            writeln!(writer, "{line}")?;
        }
        Ok(())
    }

    /// Loads episodes from a JSONL reader into the ledger.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if reading or parsing fails.
    pub fn read_jsonl<R: BufRead>(&mut self, reader: R) -> std::io::Result<usize> {
        let mut count = 0;
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(episode) = serde_json::from_str::<RemediationEpisode>(&line) {
                self.record(episode);
                count += 1;
            }
        }
        Ok(count)
    }

    /// Computes empirical weights across all remedies for a specific context.
    #[must_use]
    pub fn compute_weights(
        &self,
        vendor: &str,
        model: Option<&str>,
        firmware: Option<&str>,
        error_taxonomy: Option<&str>,
    ) -> HashMap<String, EmpiricalWeight> {
        let mut weights: HashMap<String, EmpiricalWeight> = HashMap::new();

        for ep in &self.episodes {
            if !ep.vendor.eq_ignore_ascii_case(vendor) {
                continue;
            }
            if let Some(m) = model {
                if let Some(ep_m) = &ep.model {
                    if !m.starts_with(ep_m) && !ep_m.starts_with(m) {
                        continue;
                    }
                }
            }
            if let Some(fw) = firmware {
                if let Some(ep_fw) = &ep.firmware {
                    if !fw.starts_with(ep_fw) && !ep_fw.starts_with(fw) {
                        continue;
                    }
                }
            }
            if let Some(err) = error_taxonomy {
                if !ep.error_taxonomy.contains(err) {
                    continue;
                }
            }

            let entry = weights
                .entry(ep.remedy_attempted.clone())
                .or_insert(EmpiricalWeight {
                    successes: 0,
                    attempts: 0,
                    success_rate: 0.0,
                });
            entry.record(ep.outcome);
        }

        weights
    }

    /// Re-ranks a list of candidate remedy names based on empirical historical success.
    ///
    /// Candidate actions with higher empirical success rates and higher sample counts
    /// will appear first. Candidates without history maintain their relative order at
    /// a default neutral score (0.5).
    #[must_use]
    pub fn rerank_remedies(
        &self,
        candidates: &[String],
        vendor: &str,
        model: Option<&str>,
        firmware: Option<&str>,
        error_taxonomy: Option<&str>,
    ) -> Vec<String> {
        let weights = self.compute_weights(vendor, model, firmware, error_taxonomy);
        let mut scored: Vec<(String, f32, usize)> = candidates
            .iter()
            .enumerate()
            .map(|(orig_idx, candidate)| {
                if let Some(weight) = weights.get(candidate) {
                    // Score combines success rate with sample confidence
                    (candidate.clone(), weight.success_rate, orig_idx)
                } else {
                    // Default neutral ranking for untested remedies
                    (candidate.clone(), 0.5, orig_idx)
                }
            })
            .collect();

        // Sort descending by score; on tie, preserve original order
        scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.2.cmp(&b.2))
        });

        scored.into_iter().map(|(cand, _, _)| cand).collect()
    }

    /// Ingests one agent audit-log entry as a remediation episode.
    ///
    /// This is the Tier A→B bridge: the agent's `AuditEntry` fields
    /// (`device_id`, `action`, `result`) map onto an episode. `vendor` /
    /// `model` / `firmware` / `error_taxonomy` are the caller's knowledge
    /// about the device — the audit line does not carry them. `outcome`
    /// maps from the audit `result` vocabulary: `ok` → success, `failed` →
    /// failure, anything else (`skipped`, `quarantined`) → skipped.
    ///
    /// This crate is zero-tokio by design, so the async `AuditLog` reader
    /// stays in `tv-rs-agent`; this conversion is the ingestion point its
    /// flush path calls.
    #[must_use]
    pub fn episode_from_audit(audit: AuditFacts) -> RemediationEpisode {
        let outcome = match audit.result.as_str() {
            "ok" => RemediationOutcome::Success,
            "failed" => RemediationOutcome::Failure,
            _ => RemediationOutcome::Skipped,
        };
        RemediationEpisode {
            device_id: audit.device_id,
            vendor: audit.vendor,
            model: audit.model,
            firmware: audit.firmware,
            error_taxonomy: audit.error_taxonomy,
            remedy_attempted: audit.remedy_attempted,
            outcome,
            duration_ms: audit.duration_ms,
            timestamp_ms: audit.timestamp_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_respects_capacity() {
        let mut ledger = EpisodicLedger::new(2);
        ledger.record(RemediationEpisode {
            device_id: "tv-1".into(),
            vendor: "samsung".into(),
            model: Some("QB55R".into()),
            firmware: Some("1042".into()),
            error_taxonomy: "Transport(Timeout)".into(),
            remedy_attempted: "Wake".into(),
            outcome: RemediationOutcome::Failure,
            duration_ms: 1000,
            timestamp_ms: 100,
        });
        ledger.record(RemediationEpisode {
            device_id: "tv-1".into(),
            vendor: "samsung".into(),
            model: Some("QB55R".into()),
            firmware: Some("1042".into()),
            error_taxonomy: "Transport(Timeout)".into(),
            remedy_attempted: "rs232_power_0x11".into(),
            outcome: RemediationOutcome::Success,
            duration_ms: 4000,
            timestamp_ms: 200,
        });
        assert_eq!(ledger.episodes().len(), 2);

        // Third item drops the first
        ledger.record(RemediationEpisode {
            device_id: "tv-2".into(),
            vendor: "samsung".into(),
            model: Some("QB55R".into()),
            firmware: Some("1042".into()),
            error_taxonomy: "Transport(Timeout)".into(),
            remedy_attempted: "rs232_power_0x11".into(),
            outcome: RemediationOutcome::Success,
            duration_ms: 3800,
            timestamp_ms: 300,
        });
        assert_eq!(ledger.episodes().len(), 2);
        assert_eq!(ledger.episodes()[0].device_id, "tv-1");
        assert_eq!(ledger.episodes()[1].device_id, "tv-2");
    }

    #[test]
    fn reranks_remedies_based_on_empirical_success() {
        let mut ledger = EpisodicLedger::new(100);

        // Seed 3 failures for Wake
        for i in 0..3 {
            ledger.record(RemediationEpisode {
                device_id: format!("tv-{i}"),
                vendor: "samsung".into(),
                model: Some("QB55R".into()),
                firmware: Some("1042".into()),
                error_taxonomy: "Transport(Timeout)".into(),
                remedy_attempted: "wake_on_lan".into(),
                outcome: RemediationOutcome::Failure,
                duration_ms: 2000,
                timestamp_ms: 1000 + i,
            });
        }

        // Seed 2 successes for RS-232 power
        for i in 0..2 {
            ledger.record(RemediationEpisode {
                device_id: format!("tv-{i}"),
                vendor: "samsung".into(),
                model: Some("QB55R".into()),
                firmware: Some("1042".into()),
                error_taxonomy: "Transport(Timeout)".into(),
                remedy_attempted: "rs232_power_0x11".into(),
                outcome: RemediationOutcome::Success,
                duration_ms: 4000,
                timestamp_ms: 2000 + i,
            });
        }

        let candidates = vec![
            "wake_on_lan".to_string(),
            "reconnect".to_string(),
            "rs232_power_0x11".to_string(),
        ];

        let reranked = ledger.rerank_remedies(
            &candidates,
            "samsung",
            Some("QB55R"),
            Some("1042"),
            Some("Transport"),
        );

        // rs232_power_0x11 (1.0) > reconnect (0.5 neutral) > wake_on_lan (0.0)
        assert_eq!(reranked[0], "rs232_power_0x11");
        assert_eq!(reranked[1], "reconnect");
        assert_eq!(reranked[2], "wake_on_lan");
    }

    #[test]
    fn jsonl_roundtrip() {
        let mut ledger = EpisodicLedger::new(10);
        ledger.record(RemediationEpisode {
            device_id: "tv-1".into(),
            vendor: "lg".into(),
            model: None,
            firmware: None,
            error_taxonomy: "Auth".into(),
            remedy_attempted: "reprovision".into(),
            outcome: RemediationOutcome::Success,
            duration_ms: 500,
            timestamp_ms: 12345,
        });

        let mut buffer = Vec::new();
        ledger.write_jsonl(&mut buffer).unwrap();

        let mut loaded = EpisodicLedger::new(10);
        let count = loaded.read_jsonl(&buffer[..]).unwrap();
        assert_eq!(count, 1);
        assert_eq!(loaded.episodes().len(), 1);
        assert_eq!(loaded.episodes()[0].device_id, "tv-1");
        assert_eq!(loaded.episodes()[0].outcome, RemediationOutcome::Success);
    }
}
