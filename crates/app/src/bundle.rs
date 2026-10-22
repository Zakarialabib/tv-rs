//! An app bundle held in memory: the payload, plus whatever the platform needs.
//!
//! Everything the crate produces goes through here first. Keeping the bundle in
//! memory rather than writing files as it goes has two payoffs: the `.wgt` and the
//! unpacked directory are built from the *same* entries, so they cannot disagree, and
//! a test can assert the whole artifact without touching the filesystem.

use std::path::{Path, PathBuf};

use crate::error::AppError;
use crate::zip::{write_stored_zip, Entry};

/// A bundle: `path -> bytes`, in insertion order.
#[derive(Debug, Default, Clone)]
pub struct Bundle {
    entries: Vec<Entry>,
}

impl Bundle {
    /// An empty bundle.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a file, replacing any existing entry at the same path.
    pub fn insert(&mut self, path: impl Into<String>, bytes: impl Into<Vec<u8>>) {
        let path = path.into();
        let bytes = bytes.into();
        match self.entries.iter_mut().find(|entry| entry.path == path) {
            Some(existing) => existing.bytes = bytes,
            None => self.entries.push(Entry { path, bytes }),
        }
    }

    /// Reads a payload directory into a bundle.
    ///
    /// Paths are relative to `dir` and `/`-separated, which is what both platform
    /// manifests and the ZIP format expect. Subdirectories are walked; symbolic links
    /// are not followed, because a payload that links outside itself is not a payload
    /// this crate can package reproducibly.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Io`] with the path that failed.
    pub fn from_payload_dir(dir: impl AsRef<Path>) -> Result<Self, AppError> {
        let dir = dir.as_ref();
        let mut bundle = Self::new();
        walk(dir, dir, &mut bundle)?;
        Ok(bundle)
    }

    /// The paths in the bundle, in insertion order.
    #[must_use]
    pub fn paths(&self) -> Vec<&str> {
        self.entries
            .iter()
            .map(|entry| entry.path.as_str())
            .collect()
    }

    /// One entry's bytes.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.entries
            .iter()
            .find(|entry| entry.path == path)
            .map(|entry| entry.bytes.as_slice())
    }

    /// Whether the bundle holds a path.
    #[must_use]
    pub fn contains(&self, path: &str) -> bool {
        self.get(path).is_some()
    }

    /// How many files the bundle holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the bundle is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Writes the bundle to a directory, creating it if needed.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Io`] with the path that failed.
    pub fn write_to(&self, dir: impl AsRef<Path>) -> Result<(), AppError> {
        let dir = dir.as_ref();
        for entry in &self.entries {
            let target = dir.join(&entry.path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|error| AppError::io(parent, error))?;
            }
            std::fs::write(&target, &entry.bytes).map_err(|error| AppError::io(&target, error))?;
        }
        Ok(())
    }

    /// The bundle as a ZIP archive — a Tizen `.wgt`, once `config.xml` is in it.
    #[must_use]
    pub fn to_zip(&self) -> Vec<u8> {
        write_stored_zip(&self.entries)
    }
}

fn walk(root: &Path, dir: &Path, bundle: &mut Bundle) -> Result<(), AppError> {
    let reader = std::fs::read_dir(dir).map_err(|error| AppError::io(dir, error))?;
    // Sorted, so a bundle built from a directory is byte-identical across runs and
    // filesystems: directory order is not guaranteed by any of them.
    let mut children: Vec<PathBuf> = Vec::new();
    for child in reader {
        let child = child.map_err(|error| AppError::io(dir, error))?;
        children.push(child.path());
    }
    children.sort();

    for path in children {
        let metadata =
            std::fs::symlink_metadata(&path).map_err(|error| AppError::io(&path, error))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            walk(root, &path, bundle)?;
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        let bytes = std::fs::read(&path).map_err(|error| AppError::io(&path, error))?;
        // Cannot fail: the path came from walking `root`. If it ever does, the error
        // still names the file rather than panicking.
        let relative = path
            .strip_prefix(root)
            .map_err(|error| AppError::io(&path, std::io::Error::other(error.to_string())))?;
        bundle.insert(relative_to_slash(relative), bytes);
    }
    Ok(())
}

fn relative_to_slash(relative: &Path) -> String {
    relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<String>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserting_the_same_path_twice_replaces_it() {
        let mut bundle = Bundle::new();
        bundle.insert("index.html", "<html>first</html>");
        bundle.insert("index.html", "<html>second</html>");
        assert_eq!(bundle.len(), 1);
        assert_eq!(
            bundle.get("index.html"),
            Some(b"<html>second</html>".as_slice())
        );
    }

    #[test]
    fn a_bundle_survives_a_trip_through_the_filesystem() {
        let mut bundle = Bundle::new();
        bundle.insert("config.xml", "<widget/>");
        bundle.insert("assets/app.js", "console.log(1)");

        let dir = std::env::temp_dir().join(format!("tv-rs-app-bundle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        bundle.write_to(&dir).expect("write");

        let read_back = Bundle::from_payload_dir(&dir).expect("read");
        assert_eq!(read_back.get("config.xml"), Some(b"<widget/>".as_slice()));
        assert_eq!(
            read_back.get("assets/app.js"),
            Some(b"console.log(1)".as_slice())
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_payload_directory_is_read_in_a_stable_order() {
        let dir = std::env::temp_dir().join(format!("tv-rs-app-order-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("z")).expect("mkdir");
        std::fs::write(dir.join("b.txt"), b"b").expect("write");
        std::fs::write(dir.join("a.txt"), b"a").expect("write");
        std::fs::write(dir.join("z/c.txt"), b"c").expect("write");

        let bundle = Bundle::from_payload_dir(&dir).expect("read");
        assert_eq!(bundle.paths(), ["a.txt", "b.txt", "z/c.txt"]);
        assert_eq!(
            bundle.to_zip(),
            Bundle::from_payload_dir(&dir).unwrap().to_zip()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
