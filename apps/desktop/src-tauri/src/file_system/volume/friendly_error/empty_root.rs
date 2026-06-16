//! Friendly hint when a TCC-restricted volume root lists successfully but empty.
//!
//! macOS's TCC-style restrictions (e.g. iCloud Drive without Full Disk Access)
//! don't surface as `EACCES`. `read_dir` succeeds and returns zero entries. That
//! looks identical to a genuinely empty folder. We can't distinguish the two, so
//! we hedge: show a hint only when the volume is one we know is commonly hidden
//! by TCC, and only at the volume root.

use std::path::Path;

use super::{ErrorActionKind, ErrorCategory, ListingError, ListingErrorReason};

/// Returns a typed hint when a directory at a TCC-sensitive volume root listed
/// successfully but came back empty.
///
/// The user gets a "Try again" button via `retry_hint: true` so they can re-list
/// once they've granted access.
///
/// Returns `None` when no hint is warranted (any non-recognized volume, or any
/// non-root path).
pub fn listing_error_for_restricted_empty_root(volume_id: &str, path: &Path) -> Option<ListingError> {
    // Match the literal volume ID (`crate::volumes` is macOS-only, so we can't import
    // the constant from there). Kept in sync with `volumes::ICLOUD_VOLUME_ID` (macOS).
    if volume_id == "cloud-icloud" {
        Some(ListingError {
            category: ErrorCategory::NeedsAction,
            reason: ListingErrorReason::EmptyRootICloud,
            provider: None,
            action_kind: Some(ErrorActionKind::OpenPrivacySettings),
            retry_hint: true,
            raw_detail: format!("volume={volume_id}, path={}, entries=0", path.display()),
        })
    } else {
        None
    }
}
