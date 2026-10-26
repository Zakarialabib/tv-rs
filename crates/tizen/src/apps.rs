//! Well-known Samsung application ids and the name → id resolver.
//!
//! Tizen app launch takes a numeric-or-prefixed id, not a name. These are the
//! public ids Samsung ships for the apps every signage deployment asks for; any
//! other app is addressed by its own id.
//!
//! **Where the other ids come from, and where they do not.** Earlier versions of
//! this comment said "the DIAL manifest lists what is installed — see
//! [`crate::TizenClient::dial_device_description`]". That was wrong, and it was
//! wrong in the way that costs work: the DIAL device description carries a
//! `serviceList` and nothing else, and no known client issues DIAL's `/apps`
//! endpoint against a Samsung set. Installed apps are enumerable only over the
//! remote-control channel, by emitting `ed.installedApp.get`, and only on Tizen
//! ≤4 — on Tizen 5+ (2020 and later) Samsung removed the command and the set
//! ignores it silently. So a caller on a modern panel supplies the id, and
//! [`resolve_app_id`] passes a raw id through for exactly that case.

use crate::error::{Result, TizenError};

/// Netflix (`1110110`).
pub const NETFLIX_APP_ID: &str = "1110110";
/// YouTube (`2019010019`).
pub const YOUTUBE_APP_ID: &str = "2019010019";
/// Prime Video (`3201512006786`).
pub const PRIME_VIDEO_APP_ID: &str = "3201512006786";

/// Resolves a human spelling to an app id.
///
/// Accepts the presets case-insensitively (`"Netflix"`, `"netflix "`,
/// `"YouTube"`, `"Prime"`, `"Prime Video"`), and passes any all-digit id through
/// unchanged so callers can use the same function for presets and raw ids.
///
/// # Errors
/// Returns [`TizenError::Protocol`] when the input is neither a preset nor a
/// numeric id — silently launching nothing would be worse than a clear failure.
///
/// ```
/// # fn demo() -> Result<(), tv_rs_tizen::TizenError> {
/// assert_eq!(tv_rs_tizen::resolve_app_id("Netflix")?, "1110110");
/// assert_eq!(tv_rs_tizen::resolve_app_id("2019010019")?, "2019010019");
/// assert!(tv_rs_tizen::resolve_app_id("not an app").is_err());
/// # Ok(()) }
/// ```
pub fn resolve_app_id(input: &str) -> Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(TizenError::Protocol("app name or id is empty".to_string()));
    }

    let normalized = trimmed.to_ascii_lowercase();
    let preset = match normalized.as_str() {
        "netflix" => Some(NETFLIX_APP_ID),
        "youtube" | "you tube" => Some(YOUTUBE_APP_ID),
        "prime" | "prime video" | "amazon prime" | "primevideo" => Some(PRIME_VIDEO_APP_ID),
        _ => None,
    };
    if let Some(id) = preset {
        return Ok(id.to_string());
    }

    if trimmed.chars().all(|character| character.is_ascii_digit()) {
        return Ok(trimmed.to_string());
    }

    Err(TizenError::Protocol(format!(
        "unknown app '{input}': expected a known name (netflix, youtube, prime) or a numeric app id"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_resolve_case_and_space_insensitively() {
        assert_eq!(resolve_app_id("Netflix").unwrap(), NETFLIX_APP_ID);
        assert_eq!(resolve_app_id("  netflix ").unwrap(), NETFLIX_APP_ID);
        assert_eq!(resolve_app_id("YouTube").unwrap(), YOUTUBE_APP_ID);
        assert_eq!(resolve_app_id("Prime Video").unwrap(), PRIME_VIDEO_APP_ID);
        assert_eq!(resolve_app_id("prime").unwrap(), PRIME_VIDEO_APP_ID);
    }

    #[test]
    fn numeric_ids_pass_through() {
        assert_eq!(resolve_app_id("1110110").unwrap(), "1110110");
        assert_eq!(resolve_app_id(" 3201512006786 ").unwrap(), "3201512006786");
    }

    #[test]
    fn unknown_names_are_rejected() {
        assert!(resolve_app_id("spotify").is_err());
        assert!(resolve_app_id("").is_err());
        assert!(resolve_app_id("1110110a").is_err());
    }

    #[test]
    fn boundary_tests_for_resolve_app_id() {
        assert!(resolve_app_id("   ").is_err());
        assert_eq!(resolve_app_id("  netFLIX  ").unwrap(), NETFLIX_APP_ID);
        assert_eq!(
            resolve_app_id("  AMAZON PRIME ").unwrap(),
            PRIME_VIDEO_APP_ID
        );
        // Ultra-long numeric ID passes through cleanly
        let long_id = "1".repeat(100);
        assert_eq!(resolve_app_id(&long_id).unwrap(), long_id);
        // Non-alphanumeric or special chars rejected
        assert!(resolve_app_id("netflix!").is_err());
        assert!(resolve_app_id("123-456").is_err());
    }
}
