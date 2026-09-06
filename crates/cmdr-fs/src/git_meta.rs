//! What a virtual git entry puts in the Size column, as data rather than as a
//! sentence.
//!
//! The git portal's rows don't have a byte count to show: a branch has an
//! ahead/behind pair, a commit has a files-changed count, a submodule has the
//! commit it's pinned at. [`GitEntryMeta`] carries that fact on
//! [`FileEntry::git_meta`](crate::entry::FileEntry::git_meta) and the frontend
//! words it from the message catalog, so every variant reads in the user's
//! own language with that language's own plural rules.
//!
//! ❌ Never add a variant that carries a sentence. A variant is the FACT
//! (`three branches`, `pinned at <id>`); the words for it live in
//! `apps/desktop/src/lib/intl/messages/*/fileExplorer.json`, keyed
//! `fileExplorer.git.size.*` for the cell and `fileExplorer.git.tooltip.*` for
//! the tooltip that doubles as the aria-label.

use serde::{Deserialize, Serialize};

/// What a virtual git entry's Size cell states.
///
/// The discriminant is `kind` on the wire, matching every other data-carrying
/// enum that crosses IPC. `Count`'s own sub-kind is therefore `counted`, not
/// `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum GitEntryMeta {
    /// How many of something the row holds. `n` doubles as the row's
    /// within-category Size sort key.
    Count {
        /// What is being counted.
        counted: GitCountKind,
        /// How many.
        n: u64,
    },
    /// How far a branch has diverged from the branch it's compared against.
    AheadBehind {
        /// Commits on this branch that the comparison branch doesn't have.
        ahead: u32,
        /// Commits on the comparison branch that this one doesn't have.
        behind: u32,
        /// The comparison branch's display name: the configured upstream
        /// (`origin/main`), or the `main` / `master` fallback when the branch
        /// tracks nothing.
        vs: String,
    },
    /// The commit a tag points at. Full object id; the cell shows a short form.
    TaggedCommit {
        /// The commit's full object id.
        id: String,
    },
    /// The commit a submodule is pinned at. Full object id.
    PinnedCommit {
        /// The commit's full object id.
        id: String,
    },
    /// The branch a stash entry was created on.
    StashedOnBranch {
        /// The branch's short name.
        branch: String,
    },
    /// The branch a linked worktree has checked out.
    WorktreeOnBranch {
        /// The branch's short name.
        branch: String,
    },
    /// The commit a linked worktree sits at with no branch checked out.
    WorktreeDetachedAt {
        /// The commit's full object id.
        id: String,
    },
}

/// What a [`GitEntryMeta::Count`] is counting.
///
/// One variant per row that shows a count, because each one is worded
/// differently and pluralized on its own noun.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum GitCountKind {
    /// Local branches in the repo (`.git/branches/`).
    Branches,
    /// Tags in the repo (`.git/tags/`).
    Tags,
    /// Commits reachable from HEAD, capped (`.git/commits/`).
    Commits,
    /// Entries on the stash (`.git/stash/`).
    StashEntries,
    /// Linked worktrees (`.git/worktrees/`).
    LinkedWorktrees,
    /// Submodules declared in `.gitmodules` (`.git/submodules/`).
    Submodules,
    /// Files a commit changed against its first parent.
    FilesChanged,
}
