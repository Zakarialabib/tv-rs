//! Workspace File and Metadata Audit Utilities.

use std::path::PathBuf;

pub struct WorkspaceAudit;

impl WorkspaceAudit {
    pub fn find_workspace_root() -> PathBuf {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        // tests/e2e -> ../.. -> root
        if let Some(parent) = manifest_dir.parent() {
            if let Some(root) = parent.parent() {
                if root.join("Cargo.toml").exists() {
                    return root.to_path_buf();
                }
            }
        }
        // Fallback
        PathBuf::from("c:/laragon/www/tv-rs")
    }

    pub fn read_core_cargo_toml() -> Result<String, String> {
        let root = Self::find_workspace_root();
        let path = root.join("crates/core/Cargo.toml");
        std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))
    }

    pub fn read_specdd() -> Result<String, String> {
        let root = Self::find_workspace_root();
        let path = root.join("docs/specdd.md");
        std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))
    }

    pub fn read_hil_results() -> Result<String, String> {
        let root = Self::find_workspace_root();
        let path = root.join("docs/08-hil-results.md");
        std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))
    }

    pub fn read_vision_cargo_toml() -> Result<String, String> {
        let root = Self::find_workspace_root();
        let path = root.join("crates/vision/Cargo.toml");
        std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))
    }

    pub fn read_root_cargo_toml() -> Result<String, String> {
        let root = Self::find_workspace_root();
        let path = root.join("Cargo.toml");
        std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))
    }
}
