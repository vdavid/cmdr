//! The app's side of the git browser. Everything that talks to a repository is
//! `crates/cmdr-git`; what's here is the two seams that reach it and the
//! decisions only the app can make.
//!
//! ## Two seams, no hooks
//!
//! Everything under `.git/<category>/` is a ROUTE: `VolumeManager::resolve`
//! sends it to the read-only `cmdr_git::GitPortalVolume`, which refuses every
//! mutation by trait default and can't be watched. The `.git/` landing listing
//! is a listing OVERLAY (`overlay.rs`), which reaches a pane and nothing else.
//! `LocalPosixVolume` names git nowhere, so a real file under `.git` is an
//! ordinary local file: editable, renamable, deletable, and walkable when a repo
//! folder is deleted.
//!
//! `wiring.rs` holds the rest: the parked portal, the switch both seams consult,
//! the `git-state-changed` event, and the listing refreshes a repo change drives.

pub mod overlay;
pub mod wiring;

#[cfg(test)]
mod overlay_tests;
#[cfg(test)]
mod walker_exposure_tests;
#[cfg(test)]
mod wiring_tests;

// `FriendlyGitError` lives in `cmdr-fs`: `VolumeError::FriendlyGit` carries it,
// and it maps onto `friendly_error::ErrorCategory`, so the two must live
// together. Aliased so `git::friendly::…` keeps resolving.
pub use cmdr_fs::volume::friendly_error::git as friendly;

#[allow(unused_imports, reason = "Public API re-exports consumed by IPC commands")]
pub use cmdr_git::{EntryStatus, EntryStatusCode, RepoInfo, list_status, repo_info};
#[allow(unused_imports, reason = "Public API re-exports consumed by IPC commands")]
pub use friendly::{FriendlyGitError, FriendlyGitErrorKind};
