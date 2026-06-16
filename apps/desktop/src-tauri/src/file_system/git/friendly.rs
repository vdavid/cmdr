//! Git-specific error CLASSIFICATION.
//!
//! Carries a typed `FriendlyGitErrorKind`; the user-facing words live on the
//! frontend (`src/lib/errors/git-error-messages.ts`), warm and active voice,
//! never the words "error" or "failed".
//!
//! ## How git errors reach the user
//!
//! 1. A volume hook (`try_route_listing`, `try_route_metadata`, `try_open_blob_stream`) returns
//!    `Err(FriendlyGitError)` from the git module.
//! 2. `mod.rs::friendly_to_volume_error` wraps it as `VolumeError::FriendlyGit(FriendlyGitError)`:
//!    a typed variant that carries the kind + path + optional raw detail end-to-end.
//! 3. The streaming pipeline (`listing/streaming.rs`) emits a `listing-error` event.
//!    `listing_error_from_volume_error` matches on `FriendlyGit` (FIRST, the Layer-0
//!    pass-through) and ships the typed kind as the `Git` reason so the FE renders
//!    git-specific copy from its parallel git factory.

use std::error::Error as StdError;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::file_system::volume::friendly_error::ErrorCategory;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum FriendlyGitErrorKind {
    /// `gix::discover` returned `Discover(_)` – we walked up to the FS root and
    /// found no `.git`.
    NotARepo,
    /// `gix::discover` opened the gitlink but the linked dir is gone, or
    /// `.git` is a file pointing at a worktree that no longer exists.
    OrphanedWorktree,
    /// gix returned an open / parse error that suggests on-disk damage.
    CorruptRepo,
    /// `.git/index.lock` is held – a concurrent git process is mid-write.
    IndexLocked,
    /// We can read the path but the OS denied the gitdir contents.
    PermissionDenied,
    /// The repo is bare. We don't anchor the UX on bare repos.
    BareRepo,
    /// Blob is larger than `MAX_BLOB_BYTES` (256 MB by default). Reading
    /// would allocate the whole thing into RAM (gix limitation), so we
    /// refuse instead of OOM-ing.
    BlobTooLarge,
    /// User typed a SHA that's beyond the shallow-clone boundary.
    ShallowBoundary,
    /// gix can't find a referenced object – the pack file is missing or
    /// corrupt.
    MissingObject,
    /// We can read the worktree but the OS denied the `.git` directory itself.
    GitDirPermissionDenied,
}

impl FriendlyGitErrorKind {
    /// Maps each variant to the closest `ErrorCategory` so `ErrorPane` picks
    /// the right icon and severity color.
    pub fn category(&self) -> ErrorCategory {
        match self {
            // The user can act on these (clone differently, prune, fetch, fix permissions).
            FriendlyGitErrorKind::NotARepo
            | FriendlyGitErrorKind::OrphanedWorktree
            | FriendlyGitErrorKind::PermissionDenied
            | FriendlyGitErrorKind::BareRepo
            | FriendlyGitErrorKind::BlobTooLarge
            | FriendlyGitErrorKind::ShallowBoundary
            | FriendlyGitErrorKind::GitDirPermissionDenied => ErrorCategory::NeedsAction,
            // Goes away on its own once the other git command completes.
            FriendlyGitErrorKind::IndexLocked => ErrorCategory::Transient,
            // On-disk damage: serious, retry won't help.
            FriendlyGitErrorKind::CorruptRepo | FriendlyGitErrorKind::MissingObject => ErrorCategory::Serious,
        }
    }

    /// Stable identifier used inside `raw_detail` so power users can grep
    /// logs and bug reports for a specific kind.
    fn token(&self) -> &'static str {
        match self {
            FriendlyGitErrorKind::NotARepo => "NotARepo",
            FriendlyGitErrorKind::OrphanedWorktree => "OrphanedWorktree",
            FriendlyGitErrorKind::CorruptRepo => "CorruptRepo",
            FriendlyGitErrorKind::IndexLocked => "IndexLocked",
            FriendlyGitErrorKind::PermissionDenied => "PermissionDenied",
            FriendlyGitErrorKind::BareRepo => "BareRepo",
            FriendlyGitErrorKind::BlobTooLarge => "BlobTooLarge",
            FriendlyGitErrorKind::ShallowBoundary => "ShallowBoundary",
            FriendlyGitErrorKind::MissingObject => "MissingObject",
            FriendlyGitErrorKind::GitDirPermissionDenied => "GitDirPermissionDenied",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendlyGitError {
    pub kind: FriendlyGitErrorKind,
    /// Repo path or path under inspection, for log triage.
    pub path: String,
    /// Raw underlying message (for technical-details panels). Never shown
    /// without context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
}

impl FriendlyGitError {
    pub fn new(kind: FriendlyGitErrorKind, path: impl Into<String>) -> Self {
        Self {
            kind,
            path: path.into(),
            raw: None,
        }
    }

    pub fn with_source(kind: FriendlyGitErrorKind, raw: impl Into<String>, _err: impl StdError) -> Self {
        Self {
            kind,
            path: String::new(),
            raw: Some(raw.into()),
        }
    }

    pub fn corrupt(path: &std::path::Path, err: &impl StdError) -> Self {
        Self {
            kind: FriendlyGitErrorKind::CorruptRepo,
            path: path.display().to_string(),
            raw: Some(err.to_string()),
        }
    }

    /// Builds the technical-details string for the disclosure. The path or raw
    /// source rides here so power users can copy-paste it; the kind token lets
    /// them grep logs and bug reports for a specific kind.
    pub fn raw_detail(&self) -> String {
        match &self.raw {
            Some(raw) if !raw.is_empty() => format!("git: {} ({})", self.kind.token(), raw),
            _ => format!("git: {} (path={})", self.kind.token(), self.path),
        }
    }
}

impl fmt::Display for FriendlyGitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "git: {} ({})", self.kind.token(), self.path)
    }
}

impl StdError for FriendlyGitError {}

#[cfg(test)]
mod tests {
    use super::*;

    // The user-facing words now live on the frontend
    // (`src/lib/errors/git-error-messages.ts`); the writing-rules checks moved
    // there too (`friendly-error-style.test.ts`, every kind × rendered output).
    // These tests assert only the typed mapping (category) and that the technical
    // detail preserves the path.

    #[test]
    fn category_assignments_match_intent() {
        // Spot-check the three categories so future renames don't silently
        // change severity colors.
        assert_eq!(FriendlyGitErrorKind::IndexLocked.category(), ErrorCategory::Transient);
        assert_eq!(FriendlyGitErrorKind::NotARepo.category(), ErrorCategory::NeedsAction);
        assert_eq!(FriendlyGitErrorKind::CorruptRepo.category(), ErrorCategory::Serious);
        assert_eq!(
            FriendlyGitErrorKind::ShallowBoundary.category(),
            ErrorCategory::NeedsAction
        );
        assert_eq!(FriendlyGitErrorKind::MissingObject.category(), ErrorCategory::Serious);
        assert_eq!(
            FriendlyGitErrorKind::GitDirPermissionDenied.category(),
            ErrorCategory::NeedsAction
        );
    }

    #[test]
    fn typed_variant_preserves_path_with_colon_chars() {
        use crate::file_system::volume::VolumeError;
        use crate::file_system::volume::friendly_error::{ListingErrorReason, listing_error_from_volume_error};

        // macOS resource fork style, Windows drive letter, a stash spec, and
        // a path with embedded colons all need to ride through the typed
        // variant unchanged. The earlier `:`-split sentinel path mangled
        // these; the typed variant just carries them.
        for path in [
            "/Users/me/repo/file:rsrc",
            "C:/Users/me/repo",
            "stash@{0}",
            "/repo/.git/stash/0/path:with:colons.txt",
        ] {
            let err = VolumeError::FriendlyGit(FriendlyGitError::new(FriendlyGitErrorKind::ShallowBoundary, path));
            let listing = listing_error_from_volume_error(&err, std::path::Path::new(path));
            assert!(
                matches!(
                    listing.reason,
                    ListingErrorReason::Git {
                        kind: FriendlyGitErrorKind::ShallowBoundary
                    }
                ),
                "should carry the git kind as the Git reason, got {:?}",
                listing.reason
            );
            assert_eq!(listing.category, ErrorCategory::NeedsAction);
            assert!(!listing.retry_hint, "needs-action variants don't retry");
            // The path lands in `raw_detail` via `raw_detail()`'s `path=...`
            // branch, so log greps still find it.
            assert!(
                listing.raw_detail.contains(path),
                "raw_detail {:?} should preserve path {path:?}",
                listing.raw_detail
            );
        }
    }
}
