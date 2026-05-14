//! Git-specific friendly errors.
//!
//! Mirrors the conventions of `volume::friendly_error`: warm, active voice,
//! never the words "error" or "failed". Each variant is tested in
//! `tests::friendly_*`.
//!
//! ## How git errors reach the user
//!
//! 1. A volume hook (`try_route_listing`, `try_route_metadata`,
//!    `try_open_blob_stream`) returns `Err(FriendlyGitError)` from the
//!    git module.
//! 2. `mod.rs::friendly_to_volume_error` wraps it as
//!    `VolumeError::FriendlyGit(FriendlyGitError)`: a typed variant that
//!    carries the kind + path + optional raw detail end-to-end.
//! 3. The streaming pipeline (`listing/streaming.rs`) emits a
//!    `listing-error` event. `friendly_error_from_volume_error` matches
//!    on `FriendlyGit` and calls `to_friendly_error()` so `ErrorPane`
//!    renders title + explanation + suggestion + category.

use std::error::Error as StdError;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::file_system::volume::friendly_error::{ErrorCategory, FriendlyError};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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
    pub fn title(&self) -> &'static str {
        match self {
            FriendlyGitErrorKind::NotARepo => "No git repo here",
            FriendlyGitErrorKind::OrphanedWorktree => "This worktree is orphaned",
            FriendlyGitErrorKind::CorruptRepo => "This repo looks damaged",
            FriendlyGitErrorKind::IndexLocked => "Another git is mid-write",
            FriendlyGitErrorKind::PermissionDenied => "Cmdr can't read this repo",
            FriendlyGitErrorKind::BareRepo => "Bare repos aren't supported yet",
            FriendlyGitErrorKind::BlobTooLarge => "This file's too big to load from history",
            FriendlyGitErrorKind::ShallowBoundary => "Beyond the shallow-clone boundary",
            FriendlyGitErrorKind::MissingObject => "A git object is missing",
            FriendlyGitErrorKind::GitDirPermissionDenied => "Cmdr can't open the `.git` folder",
        }
    }

    pub fn explanation(&self) -> &'static str {
        match self {
            FriendlyGitErrorKind::NotARepo => "Cmdr looked up the folder tree and didn't find a `.git` here.",
            FriendlyGitErrorKind::OrphanedWorktree => {
                "This is a linked worktree but its main repo is missing, so git can't follow the link."
            }
            FriendlyGitErrorKind::CorruptRepo => {
                "Some of the on-disk repo data is unreadable. The folder might have been edited outside git."
            }
            FriendlyGitErrorKind::IndexLocked => {
                "Git's index is locked, which usually means another git command is still running."
            }
            FriendlyGitErrorKind::PermissionDenied => {
                "The OS won't let Cmdr open the `.git` folder, so git info isn't available."
            }
            FriendlyGitErrorKind::BareRepo => {
                "Bare repos don't have a working tree, and the git browser is built around one."
            }
            FriendlyGitErrorKind::BlobTooLarge => {
                "Cmdr reads git blobs whole-file at a time, and this one's over the safety cap."
            }
            FriendlyGitErrorKind::ShallowBoundary => {
                "This commit lives past the boundary of your shallow clone, so its data isn't on disk."
            }
            FriendlyGitErrorKind::MissingObject => {
                "Git is looking for an object that's no longer in the pack files. The repo might be partially fetched or damaged."
            }
            FriendlyGitErrorKind::GitDirPermissionDenied => {
                "The OS denied access to the `.git` folder, even though the working tree is readable."
            }
        }
    }

    pub fn suggestion(&self) -> &'static str {
        match self {
            FriendlyGitErrorKind::NotARepo => "Open a folder inside a git clone to see the repo chip.",
            FriendlyGitErrorKind::OrphanedWorktree => {
                "Try opening the main repo, or remove the orphan with `git worktree prune`."
            }
            FriendlyGitErrorKind::CorruptRepo => {
                "Run `git fsck` to inspect the repo. A fresh clone often clears it up."
            }
            FriendlyGitErrorKind::IndexLocked => {
                "Wait for the running git command to finish, then navigate here again."
            }
            FriendlyGitErrorKind::PermissionDenied => {
                "Open **System Settings > Privacy & Security > Files and Folders** and grant Cmdr access to the folder."
            }
            FriendlyGitErrorKind::BareRepo => "Clone the repo into a working directory to use the git browser.",
            FriendlyGitErrorKind::BlobTooLarge => "Check out the file from a working tree if you want to read it.",
            FriendlyGitErrorKind::ShallowBoundary => {
                "Run `git fetch --unshallow` (or `--depth=N`) to bring more history into the clone."
            }
            FriendlyGitErrorKind::MissingObject => {
                "Try `git fetch` to repopulate the missing object, or `git fsck` to inspect the damage."
            }
            FriendlyGitErrorKind::GitDirPermissionDenied => {
                "Open **System Settings > Privacy & Security > Files and Folders** and grant Cmdr access. \
                 In Terminal, `ls -la .git` shows the current owner and mode."
            }
        }
    }

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

    pub fn title(&self) -> &'static str {
        self.kind.title()
    }

    pub fn explanation(&self) -> &'static str {
        self.kind.explanation()
    }

    #[allow(
        dead_code,
        reason = "Surfaced via to_friendly_error; kept for direct callers / tests"
    )]
    pub fn suggestion(&self) -> &'static str {
        self.kind.suggestion()
    }

    /// Build a fully-shaped `FriendlyError` for `ErrorPane`. The path goes
    /// into `raw_detail` so power users can copy-paste it from the
    /// "Technical details" disclosure.
    pub fn to_friendly_error(&self) -> FriendlyError {
        let raw_detail = match &self.raw {
            Some(raw) if !raw.is_empty() => format!("git: {} ({})", self.kind.token(), raw),
            _ => format!("git: {} (path={})", self.kind.token(), self.path),
        };
        FriendlyError {
            category: self.kind.category(),
            title: self.kind.title().to_string(),
            explanation: self.kind.explanation().to_string(),
            suggestion: self.kind.suggestion().to_string(),
            raw_detail,
            retry_hint: matches!(self.kind.category(), ErrorCategory::Transient),
            action_kind: None,
        }
    }
}

impl fmt::Display for FriendlyGitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.title(), self.explanation())
    }
}

impl StdError for FriendlyGitError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn never_says_error_or_failed(s: &str) {
        let lower = s.to_lowercase();
        // Skip tokens like the kind names themselves (the test inspects
        // user-facing copy strings, not technical detail tokens).
        for word in ["error", "failed"] {
            assert!(!lower.contains(word), "{s:?} contains the forbidden word `{word}`");
        }
    }

    #[test]
    fn every_kind_has_title_explanation_suggestion() {
        for kind in [
            FriendlyGitErrorKind::NotARepo,
            FriendlyGitErrorKind::OrphanedWorktree,
            FriendlyGitErrorKind::CorruptRepo,
            FriendlyGitErrorKind::IndexLocked,
            FriendlyGitErrorKind::PermissionDenied,
            FriendlyGitErrorKind::BareRepo,
            FriendlyGitErrorKind::BlobTooLarge,
            FriendlyGitErrorKind::ShallowBoundary,
            FriendlyGitErrorKind::MissingObject,
            FriendlyGitErrorKind::GitDirPermissionDenied,
        ] {
            assert!(!kind.title().is_empty(), "{kind:?} title");
            assert!(!kind.explanation().is_empty(), "{kind:?} explanation");
            assert!(!kind.suggestion().is_empty(), "{kind:?} suggestion");
            never_says_error_or_failed(kind.title());
            never_says_error_or_failed(kind.explanation());
            never_says_error_or_failed(kind.suggestion());
        }
    }

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
    fn typed_variant_to_friendly_preserves_path_with_colon_chars() {
        use crate::file_system::volume::VolumeError;
        use crate::file_system::volume::friendly_error::friendly_error_from_volume_error;

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
            let friendly = friendly_error_from_volume_error(&err, std::path::Path::new(path));
            assert_eq!(friendly.title, "Beyond the shallow-clone boundary");
            assert_eq!(friendly.category, ErrorCategory::NeedsAction);
            assert!(!friendly.retry_hint, "needs-action variants don't retry");
            // The path lands in `raw_detail` via `to_friendly_error`'s
            // `path=...` branch, so log greps still find it.
            assert!(
                friendly.raw_detail.contains(path),
                "raw_detail {:?} should preserve path {path:?}",
                friendly.raw_detail
            );
        }
    }

    #[test]
    fn to_friendly_error_keeps_messages_clean() {
        for kind in [
            FriendlyGitErrorKind::NotARepo,
            FriendlyGitErrorKind::OrphanedWorktree,
            FriendlyGitErrorKind::CorruptRepo,
            FriendlyGitErrorKind::IndexLocked,
            FriendlyGitErrorKind::PermissionDenied,
            FriendlyGitErrorKind::BareRepo,
            FriendlyGitErrorKind::BlobTooLarge,
            FriendlyGitErrorKind::ShallowBoundary,
            FriendlyGitErrorKind::MissingObject,
            FriendlyGitErrorKind::GitDirPermissionDenied,
        ] {
            let f = FriendlyGitError::new(kind, "/some/path").to_friendly_error();
            never_says_error_or_failed(&f.title);
            never_says_error_or_failed(&f.explanation);
            never_says_error_or_failed(&f.suggestion);
        }
    }
}
