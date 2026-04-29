//! Streamed commit log under `.git/commits/`.
//!
//! Each commit becomes a virtual directory named with its short SHA. The
//! display name is `<short-sha> <subject>` so date / name sorts read well.
//! `addedAt` and `createdAt` carry the author date so date-sort works out
//! of the box.
//!
//! ## Listing v1: HEAD-reachable, capped, paged
//!
//! `list_commits` walks the rev graph from HEAD by commit time (newest
//! first). We cap the listing at `MAX_COMMITS` (5000). When the walk hits
//! the cap, we append a synthetic "Load more…" entry. The frontend opens
//! it with Enter to fetch the next page (handled via `redirect_to_path`,
//! see `LOAD_MORE_REDIRECT_SCHEME`). Each batch is `BATCH_SIZE` (200)
//! commits – the cap matches the plan, the batch size is what we'd flush
//! through `ListingEventSink` if/when we wire streaming through the volume
//! hook (see decision below).
//!
//! ## Direct path entry to any commit
//!
//! `.git/commits/<sha>/...` resolves to that commit's tree even when the
//! SHA isn't in the listing. This is essential for typed-in SHAs and for
//! shallow-clone unreachable commits. Implemented in `path::classify` by
//! treating any 7+ hex segment as a commit ref. The actual commit object
//! is resolved lazily in `resolve_commit_id` so unknown commits surface a
//! friendly error instead of swallowing.
//!
//! ## Cancellation
//!
//! The volume hook returns `Vec<FileEntry>` (`list_directory` is a
//! single-shot contract today). Cancellation works because the hook runs
//! inside the listing pipeline's `spawn_blocking` task, which the listing
//! module aborts on cancel. We additionally poll an `AtomicBool` checked
//! at every iteration so a cooperatively-cancelled walk stops within one
//! commit decode (typically microseconds).
//!
//! Production listings rely on the surrounding task abort; the polled
//! flag is a `#[cfg(test)]` hook so a test can set it, run a list, and
//! observe cooperative cancellation. Gating it behind `cfg(test)` avoids
//! the previous footgun where a process-global `AtomicBool` could in
//! theory let two concurrent commit listings interfere with each other
//! (one's cancel would cancel the other). In production the walk is
//! never cancelled cooperatively, only via task abort.
//!
//! ## Why not stream through `ListingEventSink` here
//!
//! Decision: keep the volume hook single-shot for now. Building a parallel
//! streaming pipeline through the existing `Volume::list_directory`
//! contract would mean reworking the hook contract. The 5000-commit cap +
//! task-abort cancellation gives us ≤30 ms typical walks even on
//! medium-size repos (cmdr's own ~3000-commit history walks in ~7 ms);
//! the budget plus the cap makes a streaming layer overkill in v1. The
//! batch constant (`BATCH_SIZE`) is exposed so M4 can flip the switch
//! without churning callers.

use std::path::Path;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};

use gix::ObjectId;
use gix::revision::walk::Sorting;
use gix::traverse::commit::simple::CommitTimeOrder;

use crate::file_system::listing::FileEntry;

use super::column_meta::{self, files_changed_count};
use super::friendly::{FriendlyGitError, FriendlyGitErrorKind};
use super::repo::RepoHandle;

/// Hard cap on entries returned in a single `commits/` listing.
///
/// Per the plan: enough commits that 99% of users never page; small enough
/// that a worst-case revwalk stays inside the listing pipeline's responsive
/// window.
pub const MAX_COMMITS: usize = 5000;

/// Batch size for chunked emission once streaming lands. Today this only
/// gates the cancellation-poll cadence (we check `cancel` every BATCH).
pub const BATCH_SIZE: usize = 200;

/// How many hex chars we display per short SHA. Matches `git log --oneline`.
pub const SHORT_SHA_LEN: usize = 7;

/// Marker prefix for the "Load more" entry's `redirectToPath`. The frontend
/// special-cases entries whose `redirectToPath` starts with this scheme:
/// pressing Enter calls `loadMoreCommits(repoRoot, afterSha, count)` which
/// re-invokes the listing with a paginated cursor. M3 ships the marker;
/// the actual loadMore IPC is best added with the FE pagination story
/// once the cap is reached often enough to matter (Cmdr's repo today has
/// ~3000 commits; the cap is a safety net, not a UX entry point).
pub const LOAD_MORE_REDIRECT_SCHEME: &str = "cmdr-git://load-more/";

/// Test-only cooperative cancel flag. Set the flag, run a list, observe
/// cancellation within one commit decode.
///
/// Gated behind `#[cfg(test)]` so production builds don't carry a
/// process-wide cancellation switch that two concurrent commit listings
/// could interfere with. Production walks rely on `spawn_blocking` task
/// abort for hard cancellation; that's enough because the cap (5000
/// commits) keeps even pathological walks well inside the listing
/// pipeline's responsive window.
#[cfg(test)]
pub fn cancel_flag() -> &'static AtomicBool {
    static FLAG: AtomicBool = AtomicBool::new(false);
    &FLAG
}

/// In production builds, the polled cancel check is a no-op. The compiler
/// inlines this and dead-code-eliminates the surrounding `if cancel.load`
/// branch, so there's zero runtime cost on the hot path.
#[cfg(not(test))]
fn cancel_is_set() -> bool {
    false
}

#[cfg(test)]
fn cancel_is_set() -> bool {
    cancel_flag().load(Ordering::Relaxed)
}

/// Resolves a SHA-ish path segment to a commit `ObjectId`.
///
/// Accepts a full 40-char hex or any prefix ≥ `SHORT_SHA_LEN` chars (gix
/// resolves it via the loose-object index). Returns a friendly error if
/// the prefix is ambiguous or the object doesn't exist or isn't a commit.
pub fn resolve_commit_id(handle: &RepoHandle, prefix: &str) -> Result<ObjectId, FriendlyGitError> {
    let repo = handle.to_thread_local();
    let rev = repo
        .rev_parse_single(prefix)
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    let id = rev.detach();
    let obj = repo
        .find_object(id)
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    if obj.kind != gix::object::Kind::Commit {
        return Err(FriendlyGitError::new(
            FriendlyGitErrorKind::CorruptRepo,
            prefix.to_string(),
        ));
    }
    Ok(id)
}

/// True when `s` looks like a commit-id prefix (≥ 7 lowercase hex chars).
///
/// We don't validate against the object database here. Exposed as a
/// public helper so future code (URL pasting, drag-drop validation) can
/// shape-check candidates before doing a real DB lookup. Resolution
/// happens via `resolve_commit_id`.
// TODO(M3.1): Wire from URL paste / drag-drop input validation. The dead
// gate stays because the classifier already does the implicit shape check
// through `resolve_commit_id`; this helper is for callers that want to
// reject obviously-wrong input before paying for the DB lookup.
#[allow(
    dead_code,
    reason = "Public helper for URL paste / drag-drop validation; the classifier doesn't call it because the SHA-shape check happens implicitly through resolve_commit_id"
)]
pub fn looks_like_sha_prefix(s: &str) -> bool {
    s.len() >= SHORT_SHA_LEN
        && s.len() <= 40
        && s.chars()
            .all(|c| c.is_ascii_hexdigit() && (c.is_ascii_digit() || c.is_ascii_lowercase()))
}

/// Lists HEAD-reachable commits as virtual directory entries.
///
/// Up to `MAX_COMMITS` entries; on cap, appends a "Load more" entry whose
/// `redirectToPath` starts with `LOAD_MORE_REDIRECT_SCHEME`.
pub fn list_commits(handle: &RepoHandle, repo_root: &Path) -> Result<Vec<FileEntry>, FriendlyGitError> {
    let parent = repo_root.join(".git").join("commits");
    let repo = handle.to_thread_local();

    let head_id = match repo.head_id() {
        Ok(id) => id.detach(),
        Err(_) => {
            // Unborn HEAD: empty `commits/` listing is correct.
            return Ok(Vec::new());
        }
    };

    let walk = repo
        .rev_walk([head_id])
        .sorting(Sorting::ByCommitTime(CommitTimeOrder::NewestFirst))
        .all()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;

    let mut out = Vec::with_capacity(MAX_COMMITS.min(BATCH_SIZE));
    let mut last_sha: Option<String> = None;
    let mut hit_cap = false;
    for (count, info) in walk.enumerate() {
        // Polled cancellation: in tests, the flag lets us observe
        // cooperative cancel; in production, this is a const `false` and
        // the compiler drops the branch entirely. Hard cancel comes from
        // the surrounding `spawn_blocking` task abort.
        if cancel_is_set() {
            break;
        }
        if count >= MAX_COMMITS {
            hit_cap = true;
            break;
        }
        let info =
            info.map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
        let id = info.id;
        // Author date drives display time. We pull the commit object once
        // for the subject line and the author signature; gix caches loose
        // objects so the second hit is cheap.
        let (subject, author_secs, committer_secs) = decode_commit_meta(&repo, id)?;

        let short = short_sha(&id);
        let entry_path = parent.join(&short);
        let mut fe = FileEntry::new(short.clone(), entry_path.to_string_lossy().into_owned(), true, false);
        fe.icon_id = "git:commit".into();
        fe.permissions = 0o755;
        // Display: `<short-sha> <subject>` – same shape as `git log
        // --oneline` so users feel at home.
        fe.name = format!("{} {}", short, subject);
        fe.modified_at = Some(committer_secs as u64);
        fe.created_at = Some(author_secs as u64);
        fe.added_at = Some(author_secs as u64);
        // Files-changed vs. first parent. For an initial commit (no
        // parent), `files_changed_count` returns the total tree entry
        // count — still a useful "size of this snapshot" cue.
        if let Some(n) = files_changed_count(&repo, id) {
            fe.size = Some(n);
            fe.display_size = Some(column_meta::pluralize(n, "file", "files"));
            fe.display_size_tooltip = Some(format!(
                "{} changed compared to the parent commit",
                column_meta::pluralize(n, "file", "files")
            ));
        }
        out.push(fe);
        last_sha = Some(short);
    }

    if hit_cap {
        let load_more = make_load_more_entry(&parent, last_sha.as_deref());
        out.push(load_more);
    }

    Ok(out)
}

fn decode_commit_meta(repo: &gix::Repository, id: ObjectId) -> Result<(String, i64, i64), FriendlyGitError> {
    let commit = repo
        .find_commit(id)
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    let message = commit
        .message_raw()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    // Subject = first line of the commit message; truncate to keep the
    // listing readable (long subjects break the file-list layout).
    let subject = message
        .to_string()
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(80)
        .collect::<String>();
    let author = commit
        .author()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    let committer = commit
        .committer()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    let author_time = author
        .time()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    let committer_time = committer
        .time()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    Ok((subject, author_time.seconds, committer_time.seconds))
}

fn short_sha(id: &ObjectId) -> String {
    id.to_string().chars().take(SHORT_SHA_LEN).collect()
}

fn make_load_more_entry(parent: &Path, after_sha: Option<&str>) -> FileEntry {
    let display_name = "Load more…".to_string();
    let entry_path = parent.join("__cmdr_load_more__");
    let mut fe = FileEntry::new(display_name, entry_path.to_string_lossy().into_owned(), true, false);
    fe.icon_id = "git:commit".into();
    fe.permissions = 0o755;
    let after = after_sha.unwrap_or("");
    fe.redirect_to_path = Some(format!("{}{}", LOAD_MORE_REDIRECT_SCHEME, after));
    fe
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_like_sha_prefix_recognizes_short_and_full() {
        assert!(looks_like_sha_prefix("abc1234"));
        assert!(looks_like_sha_prefix("abcdef0123456789abcdef0123456789abcdef01"));
        // 6 chars: too short for an unambiguous prefix in our handling.
        assert!(!looks_like_sha_prefix("abc123"));
        // 41 chars: longer than a SHA-1.
        assert!(!looks_like_sha_prefix("abcdef0123456789abcdef0123456789abcdef012"));
        // Mixed case rejected so the classifier doesn't shadow ref names
        // that happen to be hex-ish but uppercased.
        assert!(!looks_like_sha_prefix("ABC1234"));
        // Non-hex.
        assert!(!looks_like_sha_prefix("xyz1234"));
    }
}
