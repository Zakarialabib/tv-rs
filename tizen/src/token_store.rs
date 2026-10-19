//! A small file-backed store for per-TV pairing tokens.
//!
//! Samsung TVs (2018+) return a `token` inside the `ms.channel.connect` reply.
//! Presenting it on later connects skips the on-screen allow prompt, which is the
//! difference between "works unattended" and "asks a human every reboot".
//!
//! The store is a plain JSON map keyed by the TV's IP so a settings surface can
//! list and forget pairings without a database, and so a corrupt file can be
//! quarantined rather than taking the application down.
//!
//! ```no_run
//! use tv_rs_tizen::SamsungTokenStore;
//!
//! let mut store = SamsungTokenStore::load("tv-tokens.json");
//! store.upsert("192.168.1.10", "token-abc", "Living Room").expect("write");
//! assert_eq!(store.token_for("192.168.1.10"), Some("token-abc"));
//! ```
//!
//! Every method is synchronous: the file holds a handful of entries and callers
//! invoke it from async commands, not on a hot path.
//!
//! Note the path is caller-chosen. The token is a credential — put the file
//! somewhere only the application can read.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Result, TizenError};

/// One pairing record.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PairedTv {
    /// Pairing token from `ms.channel.connect`. Empty for pre-2018 sets that
    /// never issue one; the entry still documents that the device was paired.
    #[serde(default)]
    pub token: String,
    /// Device name recorded at pairing time (best effort).
    #[serde(default)]
    pub name: String,
    /// Unix seconds of the last successful pairing.
    #[serde(default)]
    pub saved_at: u64,
}

/// File-backed pairing store keyed by TV IP.
#[derive(Debug)]
pub struct SamsungTokenStore {
    path: PathBuf,
    entries: BTreeMap<String, PairedTv>,
}

impl SamsungTokenStore {
    /// Loads the store from `path`.
    ///
    /// A missing file yields an empty store. A corrupt file is renamed to
    /// `<name>.corrupt` and the store starts empty — losing pairings is
    /// recoverable (the TV prompts once), crashing at startup is not.
    pub fn load(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let entries = match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<BTreeMap<String, PairedTv>>(&bytes) {
                Ok(map) => map,
                Err(_) => {
                    let _ = std::fs::rename(&path, path.with_extension("json.corrupt"));
                    BTreeMap::new()
                }
            },
            Err(_) => BTreeMap::new(),
        };
        Self { path, entries }
    }

    /// The file this store reads and writes.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the token for `ip` when one is stored (an empty token counts as
    /// "no token").
    pub fn token_for(&self, ip: &str) -> Option<&str> {
        self.entries
            .get(ip)
            .map(|entry| entry.token.as_str())
            .filter(|token| !token.is_empty())
    }

    /// Returns the whole record for `ip`, including token-less legacy entries.
    pub fn get(&self, ip: &str) -> Option<&PairedTv> {
        self.entries.get(ip)
    }

    /// Inserts or refreshes a pairing.
    ///
    /// An empty `token` only refreshes `name`/`saved_at` when a real token is
    /// already stored: a firmware that stops returning a token must not wipe a
    /// working one.
    pub fn upsert(&mut self, ip: &str, token: &str, name: &str) -> Result<()> {
        let entry = self.entries.entry(ip.to_string()).or_default();
        if !token.is_empty() || entry.token.is_empty() {
            entry.token = token.to_string();
        }
        if !name.trim().is_empty() {
            entry.name = name.trim().to_string();
        }
        entry.saved_at = now();
        self.persist()
    }

    /// Removes the pairing for `ip`; `true` when something was removed.
    pub fn remove(&mut self, ip: &str) -> Result<bool> {
        let removed = self.entries.remove(ip).is_some();
        if removed {
            self.persist()?;
        }
        Ok(removed)
    }

    /// All pairings, ordered by IP.
    pub fn list(&self) -> Vec<(String, PairedTv)> {
        self.entries
            .iter()
            .map(|(ip, entry)| (ip.clone(), entry.clone()))
            .collect()
    }

    /// Number of stored pairings.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` when no pairing is stored.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn persist(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let json = serde_json::to_vec_pretty(&self.entries)?;
        std::fs::write(&self.path, json)?;
        Ok(())
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// Convenience: load the store and read one token.
pub fn load_token(path: impl AsRef<Path>, ip: &str) -> Option<String> {
    SamsungTokenStore::load(path.as_ref().to_path_buf())
        .token_for(ip)
        .map(str::to_string)
}

/// Convenience: load the store, record a pairing, write it back.
pub fn save_token(path: impl AsRef<Path>, ip: &str, token: &str, name: &str) -> Result<()> {
    let mut store = SamsungTokenStore::load(path.as_ref().to_path_buf());
    store.upsert(ip, token, name)
}

/// Maps a JSON decode failure onto the crate error type so callers keep one
/// `Result`.
#[allow(dead_code)]
fn json_error(error: serde_json::Error) -> TizenError {
    TizenError::Json(error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "tv-rs-tizen-tokens-{tag}-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or_default()
        ))
    }

    #[test]
    fn missing_file_is_an_empty_store() {
        let store = SamsungTokenStore::load(temp_path("missing"));
        assert!(store.is_empty());
        assert_eq!(store.token_for("10.0.0.1"), None);
    }

    #[test]
    fn upsert_roundtrips_through_disk() {
        let path = temp_path("roundtrip");
        let mut store = SamsungTokenStore::load(&path);
        store
            .upsert("192.168.1.10", "token-a", "Kitchen")
            .expect("write");

        let reloaded = SamsungTokenStore::load(&path);
        assert_eq!(reloaded.token_for("192.168.1.10"), Some("token-a"));
        assert_eq!(reloaded.get("192.168.1.10").unwrap().name, "Kitchen");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn empty_token_does_not_clobber_a_stored_one() {
        let path = temp_path("noclobber");
        let mut store = SamsungTokenStore::load(&path);
        store.upsert("10.0.0.5", "keep-me", "Hall").expect("write");
        store.upsert("10.0.0.5", "", "Hall renamed").expect("write");

        let reloaded = SamsungTokenStore::load(&path);
        assert_eq!(reloaded.token_for("10.0.0.5"), Some("keep-me"));
        assert_eq!(reloaded.get("10.0.0.5").unwrap().name, "Hall renamed");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn legacy_entry_without_token_is_recorded_but_not_returned_as_a_token() {
        let path = temp_path("legacy");
        let mut store = SamsungTokenStore::load(&path);
        store.upsert("10.0.0.7", "", "Old TV").expect("write");

        let reloaded = SamsungTokenStore::load(&path);
        assert_eq!(reloaded.token_for("10.0.0.7"), None);
        assert!(reloaded.get("10.0.0.7").is_some());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn corrupt_file_is_quarantined_and_the_store_starts_empty() {
        let path = temp_path("corrupt");
        std::fs::write(&path, b"{not json").expect("seed corrupt file");

        let store = SamsungTokenStore::load(&path);
        assert!(store.is_empty());
        assert!(path.with_extension("json.corrupt").exists());
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("json.corrupt"));
    }

    #[test]
    fn remove_reports_whether_something_was_deleted() {
        let path = temp_path("remove");
        let mut store = SamsungTokenStore::load(&path);
        store.upsert("10.0.0.9", "t", "Office").expect("write");
        assert!(store.remove("10.0.0.9").expect("remove"));
        assert!(!store.remove("10.0.0.9").expect("remove"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn list_is_ordered_by_ip() {
        let path = temp_path("list");
        let mut store = SamsungTokenStore::load(&path);
        store.upsert("10.0.0.20", "b", "").expect("write");
        store.upsert("10.0.0.3", "a", "").expect("write");

        let ips: Vec<String> = store.list().into_iter().map(|(ip, _)| ip).collect();
        assert_eq!(ips, vec!["10.0.0.20".to_string(), "10.0.0.3".to_string()]);
        let _ = std::fs::remove_file(&path);
    }
}
