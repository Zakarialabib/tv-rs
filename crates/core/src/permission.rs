//! Hardware permissions and OS consent flows.

use async_trait::async_trait;

use crate::error::TvError;

/// Types of hardware resources gated by OS permissions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PermissionType {
    /// Screen capture / screen recording access (e.g., MediaProjection on Android, Graphics Capture on Windows).
    ScreenCapture,
    /// Access to local network devices.
    LocalNetwork,
    /// Audio capture / microphone access.
    AudioCapture,
}

impl PermissionType {
    /// A stable snake_case label for logs and diagnostics.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ScreenCapture => "screen_capture",
            Self::LocalNetwork => "local_network",
            Self::AudioCapture => "audio_capture",
        }
    }
}

/// The status of a hardware permission request or check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PermissionStatus {
    /// Permission has been granted by the user or OS.
    Granted,
    /// Permission was denied by the user or OS.
    Denied,
    /// Permission requires user interaction / consent prompt.
    PromptRequired,
    /// Permission control is not supported on this platform or device.
    NotSupported,
}

impl PermissionStatus {
    /// Returns `true` if the permission status is [`PermissionStatus::Granted`].
    #[must_use]
    pub fn is_granted(&self) -> bool {
        matches!(self, Self::Granted)
    }

    /// A stable snake_case label for logs and diagnostics.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Granted => "granted",
            Self::Denied => "denied",
            Self::PromptRequired => "prompt_required",
            Self::NotSupported => "not_supported",
        }
    }
}

/// Gating hardware access via OS permission consent flows.
///
/// On platforms such as Android (MediaProjection) and Windows (Graphics Capture / Privacy Settings),
/// hardware access like screen capture requires explicit user consent before streaming or recording.
#[async_trait]
pub trait HardwarePermission: Send + Sync {
    /// Checks the current status of a hardware permission without triggering an OS prompt.
    async fn check_permission(
        &self,
        permission: PermissionType,
    ) -> Result<PermissionStatus, TvError>;

    /// Triggers the native OS consent dialog for the given hardware permission.
    async fn request_permission(
        &self,
        permission: PermissionType,
    ) -> Result<PermissionStatus, TvError>;

    /// Triggers the native OS screen capture consent flow.
    ///
    /// Calling this from the UI layer triggers the native OS dialog on the platform,
    /// returning `PermissionStatus::Granted` only when permission is granted by the user.
    async fn request_screen_access(&self) -> Result<PermissionStatus, TvError> {
        self.request_permission(PermissionType::ScreenCapture).await
    }

    /// Checks whether screen capture permission is currently granted.
    async fn check_screen_access(&self) -> Result<PermissionStatus, TvError> {
        self.check_permission(PermissionType::ScreenCapture).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_type_as_str() {
        assert_eq!(PermissionType::ScreenCapture.as_str(), "screen_capture");
        assert_eq!(PermissionType::LocalNetwork.as_str(), "local_network");
        assert_eq!(PermissionType::AudioCapture.as_str(), "audio_capture");
    }

    #[test]
    fn permission_status_is_granted_and_as_str() {
        assert!(PermissionStatus::Granted.is_granted());
        assert!(!PermissionStatus::Denied.is_granted());
        assert!(!PermissionStatus::PromptRequired.is_granted());
        assert!(!PermissionStatus::NotSupported.is_granted());

        assert_eq!(PermissionStatus::Granted.as_str(), "granted");
        assert_eq!(PermissionStatus::Denied.as_str(), "denied");
        assert_eq!(PermissionStatus::PromptRequired.as_str(), "prompt_required");
        assert_eq!(PermissionStatus::NotSupported.as_str(), "not_supported");
    }
}
