//! The audit log: what the agent did, in a shape a technician can read.
//!
//! One JSON object per line, append-only, local. This is a product requirement, not
//! a debugging aid: when a screen misbehaves at 03:00, the person investigating runs
//! `grep` on this file, not `cargo`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

/// One line of the audit log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    /// RFC 3339 timestamp.
    pub ts: String,
    /// Device the action targeted.
    pub device_id: String,
    /// What the desired state asked for.
    pub desired: String,
    /// What the agent did.
    pub action: String,
    /// `ok`, `failed`, `skipped` or `quarantined`.
    pub result: String,
    /// Confidence in the state afterwards.
    pub confidence_after: String,
}

impl AuditEntry {
    /// Builds an entry with the current UTC time.
    #[must_use]
    pub fn now(
        device_id: impl Into<String>,
        desired: impl Into<String>,
        action: impl Into<String>,
        result: impl Into<String>,
        confidence_after: impl Into<String>,
    ) -> Self {
        Self {
            ts: now_rfc3339(),
            device_id: device_id.into(),
            desired: desired.into(),
            action: action.into(),
            result: result.into(),
            confidence_after: confidence_after.into(),
        }
    }
}

/// An append-only JSONL file.
#[derive(Debug, Clone)]
pub struct AuditLog {
    path: PathBuf,
}

impl AuditLog {
    /// Writes to `path`, creating it on first append.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The file being written.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Appends one entry.
    ///
    /// # Errors
    ///
    /// Returns the I/O error. The agent treats a failed audit write as a warning,
    /// not as a reason to stop converging devices — but it is never silently
    /// swallowed by this type.
    pub async fn record(&self, entry: &AuditEntry) -> std::io::Result<()> {
        let mut line = serde_json::to_string(entry)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        line.push('\n');

        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await?;
        file.write_all(line.as_bytes()).await?;
        file.flush().await
    }
}

/// Formats `SystemTime::now()` as an RFC 3339 UTC timestamp.
///
/// Hand-rolled to avoid a date-time dependency in a crate that otherwise needs none.
fn now_rfc3339() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();

    let days = seconds / 86_400;
    let time_of_day = seconds % 86_400;
    let (hour, minute, second) = (
        time_of_day / 3_600,
        (time_of_day % 3_600) / 60,
        time_of_day % 60,
    );

    let (year, month, day) = civil_from_days(days as i64);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Howard Hinnant's `civil_from_days`: days since the Unix epoch to a calendar date.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_serialise_to_one_line_each() {
        let entry = AuditEntry::now("tv-1", "on/netflix", "launch", "ok", "inferred");
        let line = serde_json::to_string(&entry).unwrap();
        assert!(!line.contains('\n'), "{line}");
        let parsed: AuditEntry = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed.device_id, "tv-1");
        assert_eq!(parsed.confidence_after, "inferred");
    }

    #[test]
    fn timestamps_are_rfc3339() {
        let ts = now_rfc3339();
        assert_eq!(ts.len(), 20, "{ts}");
        assert!(ts.ends_with('Z'), "{ts}");
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[10..11], "T");
    }

    #[test]
    fn the_civil_calendar_matches_known_dates() {
        // 2026-10-19 is 20 745 days after 1970-01-01 (14 leap days between 1970
        // and 2025, then 291 days into 2026).
        assert_eq!(civil_from_days(20_745), (2026, 10, 19));
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }

    #[tokio::test]
    async fn entries_append_as_jsonl() {
        let dir = std::env::temp_dir().join(format!("tv-rs-audit-{}", std::process::id()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let path = dir.join("audit.jsonl");
        let log = AuditLog::new(&path);

        log.record(&AuditEntry::now("tv-1", "on", "wake", "ok", "high"))
            .await
            .unwrap();
        log.record(&AuditEntry::now("tv-2", "off", "sleep", "ok", "inferred"))
            .await
            .unwrap();

        let text = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(text.lines().count(), 2);
        assert!(text
            .lines()
            .all(|line| serde_json::from_str::<AuditEntry>(line).is_ok()));

        tokio::fs::remove_dir_all(&dir).await.unwrap();
    }
}
