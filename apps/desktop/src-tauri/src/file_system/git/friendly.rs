//! Git-specific friendly errors.
//!
//! Mirrors the conventions of `volume::friendly_error`: warm, active voice,
//! never the words "error" or "failed". Each variant is tested in
//! `tests::friendly_*`.
//!
//! ## How git errors reach the user (M4)
//!
//! 1. A volume hook (`try_route_listing`, `try_route_metadata`,
//!    `try_open_blob_stream`) returns `Err(FriendlyGitError)` from the
//!    git module.
//! 2. `mod.rs::friendly_to_volume_error` wraps it as a
//!    `VolumeError::IoError` whose `message` carries a sentinel-tagged,
//!    NUL-separated payload (`__GIT_FRIENDLY__\0<kind>\0<path>\0<title>\0<explanation>`).
//! 3. The streaming pipeline (`listing/streaming.rs`) emits a
//!    `listing-error` event. `friendly_error_from_volume_error`
//!    recognizes the sentinel via `try_decode_git_friendly` and
//!    builds a fully-shaped `FriendlyError` so `ErrorPane` renders
//!    title + explanation + suggestion + category.
//!
//! ### Why NUL-separated, not `:`
//!
//! Paths can contain `:` on macOS (resource forks), Windows (drive letters),
//! and from git itself (`stash@{0}` is a valid stash spec). An earlier
//! `split_once(':')` chain mangled any of these. NUL is forbidden in POSIX
//! and Windows paths and never appears inside titles or explanations, so
//! it's the safe field separator. The on-the-wire message also reads
//! naturally in logs because `\0` renders as nothing (or as a visible
//! escape, depending on the viewer); the sentinel itself stays
//! grep-friendly.

use std::error::Error as StdError;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::file_system::volume::friendly_error::{ErrorCategory, FriendlyError};

/// Sentinel prefix on `VolumeError::IoError::message` that flags a
/// git-friendly payload. `friendly_error_from_volume_error` strips it
/// and rebuilds a structured `FriendlyError`.
pub(crate) const GIT_FRIENDLY_SENTINEL: &str = "__GIT_FRIENDLY__";

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

    /// Stable string token used by the sentinel encoding.
    pub(crate) fn token(&self) -> &'static str {
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

    /// Inverse of `token()`. Used by the volume-error decoder.
    pub(crate) fn from_token(s: &str) -> Option<Self> {
        Some(match s {
            "NotARepo" => Self::NotARepo,
            "OrphanedWorktree" => Self::OrphanedWorktree,
            "CorruptRepo" => Self::CorruptRepo,
            "IndexLocked" => Self::IndexLocked,
            "PermissionDenied" => Self::PermissionDenied,
            "BareRepo" => Self::BareRepo,
            "BlobTooLarge" => Self::BlobTooLarge,
            "ShallowBoundary" => Self::ShallowBoundary,
            "MissingObject" => Self::MissingObject,
            "GitDirPermissionDenied" => Self::GitDirPermissionDenied,
            _ => return None,
        })
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
        }
    }

    /// Encode for embedding in a `VolumeError::IoError` message. The
    /// `ListingErrorEvent` decoder strips the prefix and rebuilds the
    /// `FriendlyError` via `try_decode_git_friendly`.
    pub(crate) fn encode_for_volume_error(&self) -> String {
        // Format: `__GIT_FRIENDLY__\0<token>\0<path>\0<title>\0<explanation>`.
        // NUL separators keep paths with `:` (Windows drive letters, macOS
        // resource forks, `stash@{0}` specs) intact through the round-trip.
        // The sentinel prefix stays at the start of the string so log greps
        // for `__GIT_FRIENDLY__` still hit every git failure that bubbled
        // to the user.
        format!(
            "{}\0{}\0{}\0{}\0{}",
            GIT_FRIENDLY_SENTINEL,
            self.kind.token(),
            self.path,
            self.kind.title(),
            self.kind.explanation()
        )
    }
}

impl fmt::Display for FriendlyGitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.title(), self.explanation())
    }
}

impl StdError for FriendlyGitError {}

/// If `message` carries a git-friendly sentinel, returns the decoded
/// `FriendlyError`. The embedded path is forwarded back into the volume
/// layer's friendly-error path.
pub fn try_decode_git_friendly(message: &str) -> Option<FriendlyError> {
    // Format: `__GIT_FRIENDLY__\0<token>\0<path>\0<title>\0<explanation>`.
    // We need only the token and the path to rebuild the `FriendlyError`;
    // title / explanation come from the kind's static copy. Reading them
    // off the wire would let buggy senders corrupt user-facing strings.
    let rest = message.strip_prefix(GIT_FRIENDLY_SENTINEL)?.strip_prefix('\0')?;
    let mut parts = rest.splitn(5, '\0');
    let token = parts.next()?;
    let path = parts.next()?;
    // The remaining title / explanation fields are present but ignored on
    // decode (kept on the wire for log readability and forward-compat).
    let kind = FriendlyGitErrorKind::from_token(token)?;
    Some(FriendlyGitError::new(kind, path).to_friendly_error())
}

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
    fn token_round_trip() {
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
            assert_eq!(FriendlyGitErrorKind::from_token(kind.token()), Some(kind));
        }
    }

    #[test]
    fn encoded_volume_error_decodes_back_to_friendly() {
        let original = FriendlyGitError::new(FriendlyGitErrorKind::ShallowBoundary, "/repo/.git/commits/abc123");
        let encoded = original.encode_for_volume_error();
        let decoded = try_decode_git_friendly(&encoded).expect("sentinel-tagged message decodes");
        assert_eq!(decoded.title, "Beyond the shallow-clone boundary");
        assert_eq!(decoded.category, ErrorCategory::NeedsAction);
        assert!(!decoded.retry_hint, "needs-action errors don't retry");
    }

    #[test]
    fn non_sentinel_message_returns_none() {
        assert!(try_decode_git_friendly("plain old IO problem").is_none());
        assert!(try_decode_git_friendly("__GIT_FRIENDLY__not_well_formed").is_none());
        // Older-format `:`-separated payloads must NOT decode silently :
        // we changed wire formats in the M4 fixup and want a clean miss
        // rather than a half-parsed surprise.
        assert!(try_decode_git_friendly("__GIT_FRIENDLY__:NotARepo:/some/path:Title: Body").is_none());
    }

    #[test]
    fn round_trips_path_with_colon_chars() {
        // macOS resource fork style, Windows drive letter, and a stash spec
        // all contain `:`. The split-by-NUL decoder must keep them intact.
        for path in [
            "/Users/me/repo/file:rsrc",
            "C:/Users/me/repo",
            "stash@{0}",
            "/repo/.git/stash/0/path:with:colons.txt",
        ] {
            let original = FriendlyGitError::new(FriendlyGitErrorKind::ShallowBoundary, path);
            let encoded = original.encode_for_volume_error();
            let decoded = try_decode_git_friendly(&encoded).expect("sentinel decodes");
            assert_eq!(decoded.title, "Beyond the shallow-clone boundary");
            // The path lands in `raw_detail` via `to_friendly_error`'s
            // `path=...` branch, so we can grep for it round-tripped.
            assert!(
                decoded.raw_detail.contains(path),
                "raw_detail {:?} should preserve path {path:?}",
                decoded.raw_detail
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
