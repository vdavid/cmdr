//! Git-specific friendly errors.
//!
//! Mirrors the conventions of `volume::friendly_error`: warm, active voice,
//! never the words "error" or "failed". Each variant is tested in
//! `tests::friendly_*`.

use std::error::Error as StdError;
use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FriendlyGitErrorKind {
    /// `gix::discover` returned `Discover(_)` — we walked up to the FS root and
    /// found no `.git`.
    NotARepo,
    /// `gix::discover` opened the gitlink but the linked dir is gone, or
    /// `.git` is a file pointing at a worktree that no longer exists.
    OrphanedWorktree,
    /// gix returned an open / parse error that suggests on-disk damage.
    CorruptRepo,
    /// `.git/index.lock` is held — a concurrent git process is mid-write.
    IndexLocked,
    /// We can read the path but the OS denied the gitdir contents.
    PermissionDenied,
    /// The repo is bare. We don't anchor the UX on bare repos.
    BareRepo,
    /// Blob is larger than `MAX_BLOB_BYTES` (256 MB by default). Reading
    /// would allocate the whole thing into RAM (gix limitation), so we
    /// refuse instead of OOM-ing.
    BlobTooLarge,
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
        }
    }

    #[allow(
        dead_code,
        reason = "Used by frontend serialization and friendly_test variants in M4"
    )]
    pub fn suggestion(&self) -> &'static str {
        match self {
            FriendlyGitErrorKind::NotARepo => "Open a folder inside a git clone to see the repo chip.",
            FriendlyGitErrorKind::OrphanedWorktree => {
                "Try opening the main repo, or remove the orphan with `git worktree prune`."
            }
            FriendlyGitErrorKind::CorruptRepo => "Check the repo with `git fsck`. A fresh clone often clears it up.",
            FriendlyGitErrorKind::IndexLocked => "Wait for the running git command to finish, then try again.",
            FriendlyGitErrorKind::PermissionDenied => {
                "Open Disk Access in System Settings and grant Cmdr access to the folder."
            }
            FriendlyGitErrorKind::BareRepo => "Clone the repo into a working directory to use the git browser.",
            FriendlyGitErrorKind::BlobTooLarge => "Check out the file from a working tree if you need it.",
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
        reason = "Surfaced via FriendlyError once UI in M4 wires the suggestion text"
    )]
    pub fn suggestion(&self) -> &'static str {
        self.kind.suggestion()
    }
}

impl fmt::Display for FriendlyGitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.title(), self.explanation())
    }
}

impl StdError for FriendlyGitError {}
