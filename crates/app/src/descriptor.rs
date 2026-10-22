//! What a TV app is, before it is packaged.
//!
//! A descriptor is the small set of facts both platforms demand and neither shares:
//! an application id, a display name, a version, an entry point and a vendor. Tizen
//! wants them in `config.xml`, webOS wants them in `appinfo.json`, and a plain web
//! bundle needs only the entry point — but the facts themselves are the same, so they
//! are validated once, here.
//!
//! Validation is strict on purpose. A bad application id is not discovered by the
//! packaging step, it is discovered by the television that refuses to install the
//! result, and a TV in a wall two hundred kilometres away is an expensive way to
//! learn that an id had a trailing dot.

use crate::error::AppError;

/// Everything the platform manifests need to know about an app.
///
/// ```no_run
/// use tv_rs_app::AppDescriptor;
///
/// let descriptor = AppDescriptor::new("com.example.wall", "Lobby wall", "1.0.0")
///     .with_vendor("Example Signage")
///     .with_icon("icon.png");
/// descriptor.validate()?;
/// # Ok::<(), tv_rs_app::AppError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppDescriptor {
    /// Reverse-DNS application id, e.g. `com.example.wall`.
    ///
    /// Both platforms use it as the install identity, which is why it may not change
    /// between releases of the same app.
    pub id: String,
    /// Human-readable name, as the TV shows it.
    pub name: String,
    /// Version, `major.minor.patch`.
    pub version: String,
    /// Entry document, relative to the bundle root.
    pub entry: String,
    /// Vendor or operator name, shown in the webOS app list.
    pub vendor: String,
    /// Icon path inside the bundle, when there is one.
    pub icon: Option<String>,
}

impl AppDescriptor {
    /// A descriptor with the given identity, entry point `index.html` and no icon.
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version: version.into(),
            entry: "index.html".to_string(),
            vendor: String::new(),
            icon: None,
        }
    }

    /// Sets the vendor.
    #[must_use]
    pub fn with_vendor(mut self, vendor: impl Into<String>) -> Self {
        self.vendor = vendor.into();
        self
    }

    /// Sets the icon path.
    #[must_use]
    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Sets the entry document.
    #[must_use]
    pub fn with_entry(mut self, entry: impl Into<String>) -> Self {
        self.entry = entry.into();
        self
    }

    /// The Tizen package name: the application id without its last segment.
    ///
    /// Tizen's manifest carries both a package name and an application id, and the
    /// application id must be the package name plus one dot-separated segment. Rather
    /// than ask the caller for a second string that can disagree with the first, the
    /// package name is derived — `com.example.wall` is package `com.example`,
    /// application `com.example.wall`.
    #[must_use]
    pub fn tizen_package(&self) -> String {
        match self.id.rfind('.') {
            Some(at) => self.id[..at].to_string(),
            None => self.id.clone(),
        }
    }

    /// Checks everything a platform would reject later, now.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::InvalidDescriptor`] naming the field and the reason: an id
    /// that is not reverse-DNS, a version that is not `major.minor.patch`, an entry
    /// that escapes the bundle, or an empty name.
    pub fn validate(&self) -> Result<(), AppError> {
        validate_id(&self.id)?;
        validate_version(&self.version)?;
        validate_relative_path("entry", &self.entry)?;
        if let Some(icon) = &self.icon {
            validate_relative_path("icon", icon)?;
        }
        if self.name.trim().is_empty() {
            return Err(AppError::InvalidDescriptor {
                field: "name",
                reason: "must not be empty; the TV shows it in the app list",
            });
        }
        Ok(())
    }
}

fn validate_id(id: &str) -> Result<(), AppError> {
    if id.is_empty() {
        return Err(AppError::InvalidDescriptor {
            field: "id",
            reason: "must not be empty",
        });
    }
    // Reverse-DNS with at least two segments: a single-segment id is legal on neither
    // platform, and catching it here beats a failed install on a distant panel.
    if !id.contains('.') {
        return Err(AppError::InvalidDescriptor {
            field: "id",
            reason: "must be reverse-DNS with at least one dot, e.g. com.example.wall",
        });
    }
    if id.starts_with('.') || id.ends_with('.') || id.contains("..") {
        return Err(AppError::InvalidDescriptor {
            field: "id",
            reason: "must not start, end or double a dot",
        });
    }
    for segment in id.split('.') {
        if segment.is_empty() {
            return Err(AppError::InvalidDescriptor {
                field: "id",
                reason: "has an empty segment",
            });
        }
        if !segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(AppError::InvalidDescriptor {
                field: "id",
                reason: "segments may hold only ASCII letters, digits, '-' and '_'",
            });
        }
        if segment.starts_with('-') || segment.starts_with('_') {
            return Err(AppError::InvalidDescriptor {
                field: "id",
                reason: "a segment may not start with '-' or '_'",
            });
        }
    }
    Ok(())
}

fn validate_version(version: &str) -> Result<(), AppError> {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() != 3 {
        return Err(AppError::InvalidDescriptor {
            field: "version",
            reason: "must be major.minor.patch",
        });
    }
    for part in parts {
        if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
            return Err(AppError::InvalidDescriptor {
                field: "version",
                reason: "each part must be digits only",
            });
        }
    }
    Ok(())
}

fn validate_relative_path(field: &'static str, value: &str) -> Result<(), AppError> {
    if value.is_empty() {
        return Err(AppError::InvalidDescriptor {
            field,
            reason: "must not be empty",
        });
    }
    if value.starts_with('/') || value.starts_with('\\') {
        return Err(AppError::InvalidDescriptor {
            field,
            reason: "must be relative to the bundle root",
        });
    }
    if value.contains("..") {
        return Err(AppError::InvalidDescriptor {
            field,
            reason: "must not escape the bundle root",
        });
    }
    if value.contains('\\') {
        return Err(AppError::InvalidDescriptor {
            field,
            reason: "must use '/' as the separator, on every platform",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_well_formed_descriptor_validates() {
        let descriptor = AppDescriptor::new("com.example.wall", "Lobby wall", "1.0.0")
            .with_vendor("Example")
            .with_icon("icon.png");
        assert!(descriptor.validate().is_ok());
    }

    #[test]
    fn the_tizen_package_is_the_id_without_its_last_segment() {
        let descriptor = AppDescriptor::new("com.example.wall", "W", "1.0.0");
        assert_eq!(descriptor.tizen_package(), "com.example");
    }

    #[test]
    fn ids_that_a_television_would_reject_are_rejected_here() {
        for bad in [
            "wall",
            "com..example",
            ".com.example",
            "com.example.",
            "com.ex ample",
        ] {
            let descriptor = AppDescriptor::new(bad, "W", "1.0.0");
            assert!(
                matches!(
                    descriptor.validate(),
                    Err(AppError::InvalidDescriptor { field: "id", .. })
                ),
                "{bad} should not validate"
            );
        }
    }

    #[test]
    fn versions_must_be_three_numeric_parts() {
        for bad in ["1.0", "1.0.0.0", "1.0.x", "v1.0.0", ""] {
            let descriptor = AppDescriptor::new("com.example.wall", "W", bad);
            assert!(
                matches!(
                    descriptor.validate(),
                    Err(AppError::InvalidDescriptor {
                        field: "version",
                        ..
                    })
                ),
                "{bad} should not validate"
            );
        }
    }

    #[test]
    fn paths_may_not_escape_the_bundle() {
        for bad in ["/index.html", "../index.html", "web\\index.html", ""] {
            let descriptor = AppDescriptor::new("com.example.wall", "W", "1.0.0").with_entry(bad);
            assert!(
                matches!(
                    descriptor.validate(),
                    Err(AppError::InvalidDescriptor { field: "entry", .. })
                ),
                "{bad} should not validate"
            );
        }
    }

    #[test]
    fn an_empty_name_is_refused_because_the_tv_shows_it() {
        let descriptor = AppDescriptor::new("com.example.wall", "   ", "1.0.0");
        assert!(matches!(
            descriptor.validate(),
            Err(AppError::InvalidDescriptor { field: "name", .. })
        ));
    }
}
