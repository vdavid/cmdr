//! Friendly hint when a TCC-restricted volume root lists successfully but empty.
//!
//! macOS's TCC-style restrictions (e.g. iCloud Drive without Full Disk Access)
//! don't surface as `EACCES`. `read_dir` succeeds and returns zero entries. That
//! looks identical to a genuinely empty folder. We can't distinguish the two, so
//! we hedge: show a hint only when the volume is one we know is commonly hidden
//! by TCC, and only at the volume root.

use std::path::Path;

use super::{ErrorCategory, FriendlyError};

/// Returns a friendly hint when a directory at a TCC-sensitive volume root listed
/// successfully but came back empty.
///
/// The user gets a "Try again" button via `retry_hint: true` so they can re-list
/// once they've granted access.
///
/// Returns `None` when no hint is warranted (any non-recognized volume, or any
/// non-root path).
pub fn friendly_error_for_restricted_empty_root(volume_id: &str, path: &Path) -> Option<FriendlyError> {
    // Match the literal volume ID (`crate::volumes` is macOS-only, so we can't import
    // the constant from there). Kept in sync with `volumes::ICLOUD_VOLUME_ID` (macOS).
    if volume_id == "cloud-icloud" {
        Some(FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "iCloud Drive looks empty".into(),
            explanation: "Cmdr opened iCloud Drive but it came back with no files. macOS hides iCloud Drive contents \
                from apps that don't have **Full Disk Access**, so granting Cmdr that permission is the most \
                likely fix.\n\nIf your iCloud Drive really is empty, you can ignore this hint."
                .into(),
            suggestion: "Here's what to try:\n\
                - Open [**System Settings > Privacy & Security**](x-apple.systempreferences:com.apple.preference.security?Privacy) and pick **Full Disk Access**\n\
                - Add Cmdr (use the **+** button) and toggle it on\n\
                - Quit and reopen Cmdr\n\
                - Come back here to retry"
                .into(),
            raw_detail: format!("volume={volume_id}, path={}, entries=0", path.display()),
            retry_hint: true,
            action_kind: Some(super::ErrorActionKind::OpenPrivacySettings),
        })
    } else {
        None
    }
}
