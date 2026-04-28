//! Git browser foundation (M1).
//!
//! Repo detection, repo info (branch, ahead/behind, dirty), per-entry status,
//! `.git/*`-mutation watcher, and friendly error mapping.
//!
//! M1 surface only: virtual `.git` listing, blob streaming, log/stash/worktrees
//! land in M2 and M3. The schema additions (icon IDs, `redirectToPath`,
//! `--color-git-portal`) ship here so later milestones don't have to ripple
//! a schema change through every consumer.
//!
//! See `CLAUDE.md` for the module map, decisions, and gotchas.

pub mod friendly;
pub mod repo;
pub mod status;
pub mod watcher;

#[cfg(test)]
mod bench;
#[cfg(test)]
mod tests;

#[allow(
    unused_imports,
    reason = "Public API re-exports consumed by IPC commands and future M2/M3 modules"
)]
pub use friendly::{FriendlyGitError, FriendlyGitErrorKind};
#[allow(
    unused_imports,
    reason = "Public API re-exports consumed by IPC commands and future M2/M3 modules"
)]
pub use repo::{RepoInfo, discover_repo, repo_info};
#[allow(
    unused_imports,
    reason = "Public API re-exports consumed by IPC commands and future M2/M3 modules"
)]
pub use status::{EntryStatus, EntryStatusCode, list_status};
#[allow(
    unused_imports,
    reason = "Public API re-exports consumed by IPC commands and future M2/M3 modules"
)]
pub use watcher::{GitWatcherRegistry, get_watcher_registry};
