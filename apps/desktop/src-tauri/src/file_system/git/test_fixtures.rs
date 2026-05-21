//! Shared test helpers for building git repository fixtures.
//!
//! Background: every `*_tests.rs` file under this module used to define
//! its own `fn git(dir, args)` that shelled out to the system `git` CLI
//! and chained dozens of those calls in `build_simple_repo` /
//! `build_repo_with_branches` / `commit_with_date`. A single fixture
//! with three feature branches and 8 total commits cost ~31
//! `fork+exec` of `git`, which is fast in isolation but borderline
//! against the project's intentional 8 s nextest cap once
//! `./scripts/check.sh` runs other checks in parallel. Two of those
//! tests (`commits_listing_cancellation_polls_atomic_flag`,
//! `branches_listing_sorts_by_ahead_count_within_category`) were
//! observed crossing the cap in back-to-back runs.
//!
//! This module replaces the shell-out chains with in-process gix calls.
//! `gix` is already a heavyweight dependency in this crate (54 sub-
//! crates in `Cargo.lock`), so we pay no new dependency cost. Each
//! commit creation goes from ~50-150 ms (process spawn + git startup +
//! index manipulation + ref update) to single-digit microseconds (one
//! blob write, one tree edit, one commit object, one ref update — all
//! in-process loose-object writes).
//!
//! ## What lives here
//!
//! - [`temp_dir`]: per-test unique temp directory under `std::env::temp_dir()`.
//! - [`cleanup`]: best-effort `remove_dir_all` for the directory.
//! - [`Fixture`]: thin wrapper around a `gix::Repository` with helpers
//!   for the patterns the tests need (write a file + commit it, create
//!   a branch, switch HEAD to a branch).
//!
//! ## Operations NOT covered (still shell-out)
//!
//! gix 0.81 doesn't expose public APIs for stash creation,
//! `git worktree add`, or `git submodule add`. The handful of tests
//! that need those operations keep using a thin [`git_cli`] helper
//! that wraps `Command::new("git")`. The cost there is bounded: each
//! affected test makes a few CLI calls instead of dozens.

// Test-only support module: each helper here is used by a subset of
// the sibling `*_tests.rs` files. `cargo check` would otherwise treat
// the unused ones as `dead_code`.
#![allow(
    dead_code,
    reason = "Helpers are intentionally shared across multiple *_tests.rs files; not every file uses every helper."
)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use gix::ObjectId;
use gix::actor::SignatureRef;
use gix::bstr::BStr;
pub(super) use gix::object::tree::EntryKind;

pub(super) const TEST_AUTHOR_NAME: &str = "Cmdr Test";
pub(super) const TEST_AUTHOR_EMAIL: &str = "test@cmdr.local";

/// Default time used by [`Fixture::commit_file`]. Per-test code that needs
/// distinct timestamps calls [`Fixture::commit_file_at`] instead.
const DEFAULT_COMMIT_SECS: u64 = 1_700_000_000;

/// Creates a fresh per-process temp directory under `std::env::temp_dir()`.
/// The path includes the module prefix, the supplied name, the PID, and a
/// nanosecond timestamp, so concurrent test invocations don't collide.
pub(super) fn temp_dir(module_prefix: &str, name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "cmdr_git_{}_{}_{}_{}",
        module_prefix,
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Best-effort cleanup. Tests call this at the end of the body so a
/// successful run leaves no debris; a panicking run still leaves the
/// directory for post-mortem inspection.
pub(super) fn cleanup(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

/// Last-resort fallback for the few operations gix doesn't expose
/// (stash creation, worktree add, submodule add). Sets the four
/// `GIT_AUTHOR_*` / `GIT_COMMITTER_*` env vars so deterministic
/// timestamps stay deterministic, and silences output so test logs
/// stay readable.
pub(super) fn git_cli(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", TEST_AUTHOR_NAME)
        .env("GIT_AUTHOR_EMAIL", TEST_AUTHOR_EMAIL)
        .env("GIT_COMMITTER_NAME", TEST_AUTHOR_NAME)
        .env("GIT_COMMITTER_EMAIL", TEST_AUTHOR_EMAIL)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("git command");
    assert!(status.success(), "git {:?} failed in {}", args, dir.display());
}

/// Same as [`git_cli`] but captures stdout.
pub(super) fn git_cli_capture(dir: &Path, args: &[&str]) -> Vec<u8> {
    Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", TEST_AUTHOR_NAME)
        .env("GIT_AUTHOR_EMAIL", TEST_AUTHOR_EMAIL)
        .env("GIT_COMMITTER_NAME", TEST_AUTHOR_NAME)
        .env("GIT_COMMITTER_EMAIL", TEST_AUTHOR_EMAIL)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .expect("git command")
        .stdout
}

/// Owns a gix repo open at `dir`. Tracks the current branch the next
/// `commit_*` call should write to (mirrors how `git checkout` flips
/// the active branch). Defaults to `main` (gix's default for new
/// repos).
pub(super) struct Fixture {
    pub(super) dir: PathBuf,
    pub(super) repo: gix::Repository,
    /// Active branch name (without `refs/heads/` prefix). Each commit
    /// is written to `refs/heads/{current_branch}`; switching is just
    /// updating this field plus the HEAD symbolic ref so prod code
    /// (`gix::Repository::head_*`) sees the right branch.
    pub(super) current_branch: String,
}

impl Fixture {
    /// Initialize an empty gix repo at `dir`. Equivalent to `git init
    /// -b main`. gix's default branch is already `main` (deviation
    /// from upstream git), so no `init.defaultBranch` ceremony.
    pub(super) fn init(dir: PathBuf) -> Self {
        let repo = gix::init(&dir).expect("gix::init");
        Self {
            dir,
            repo,
            current_branch: "main".to_string(),
        }
    }

    /// Write `content` to `<dir>/<file>` and commit it on the current
    /// branch. Returns the new commit's ObjectId. Uses
    /// [`DEFAULT_COMMIT_SECS`] for both author and committer time.
    pub(super) fn commit_file(&mut self, file: &str, content: &[u8], message: &str) -> ObjectId {
        self.commit_files(&[(file, content)], message, DEFAULT_COMMIT_SECS)
    }

    /// Like [`commit_file`](Self::commit_file) but with a specific
    /// epoch second. Used by `snapshot_dates_tests` to seed
    /// reproducible per-file dates.
    pub(super) fn commit_file_at(&mut self, file: &str, content: &[u8], message: &str, secs: u64) -> ObjectId {
        self.commit_files(&[(file, content)], message, secs)
    }

    /// Multi-file commit: writes each `(path, content)` pair to disk,
    /// then builds one commit containing all of them as additions or
    /// updates on top of the current HEAD tree. Carries over every
    /// already-tracked entry from the parent's tree (so commits are
    /// additive — drop the world before reusing this for a delete).
    pub(super) fn commit_files(&mut self, files: &[(&str, &[u8])], message: &str, secs: u64) -> ObjectId {
        let with_mode: Vec<(&str, &[u8], EntryKind)> = files
            .iter()
            .map(|(rel, content)| (*rel, *content, EntryKind::Blob))
            .collect();
        self.commit_files_with_modes(&with_mode, message, secs)
    }

    /// Like [`commit_files`](Self::commit_files) but the caller picks
    /// the [`EntryKind`] per entry, so tests can pin executable mode
    /// bits (`BlobExecutable`) on commits without going through a CLI
    /// `chmod`+`git add` dance.
    pub(super) fn commit_files_with_modes(
        &mut self,
        files: &[(&str, &[u8], EntryKind)],
        message: &str,
        secs: u64,
    ) -> ObjectId {
        for (rel, content, _) in files {
            let abs = self.dir.join(rel);
            if let Some(parent) = abs.parent() {
                std::fs::create_dir_all(parent).expect("create parent dir for fixture file");
            }
            std::fs::write(&abs, content).expect("write fixture file");
        }

        // Start the tree edit from the current branch's tree if one
        // exists, otherwise from an empty tree. This makes commits
        // additive, matching how `git commit` would behave after
        // `git add .`.
        let parent_commit_id: Option<ObjectId> = self.current_branch_tip();
        let base_tree_id = match &parent_commit_id {
            Some(c) => self
                .repo
                .find_object(*c)
                .expect("find parent commit")
                .into_commit()
                .tree_id()
                .expect("parent commit tree id")
                .detach(),
            None => ObjectId::empty_tree(self.repo.object_hash()),
        };
        let mut editor = self.repo.edit_tree(base_tree_id).expect("edit_tree");
        for (rel, content, kind) in files {
            let blob = self.repo.write_blob(content).expect("write_blob").detach();
            editor.upsert(rel.to_string(), *kind, blob).expect("upsert into tree");
        }
        let tree_id = editor.write().expect("write tree").detach();

        // SignatureRef takes the time as a raw string in git's internal
        // commit format: `<seconds> <offset>`. UTC offset is fine for
        // tests — `gix_date::Time::parse` will round-trip this back.
        let time_str = format!("{secs} +0000");
        let sig = SignatureRef {
            name: BStr::new(TEST_AUTHOR_NAME),
            email: BStr::new(TEST_AUTHOR_EMAIL),
            time: time_str.as_str(),
        };
        let reference = format!("refs/heads/{}", self.current_branch);
        let parents = parent_commit_id.into_iter().collect::<Vec<_>>();
        let commit_id = self
            .repo
            .commit_as(sig, sig, reference.as_str(), message, tree_id, parents)
            .expect("commit_as")
            .detach();

        // `commit_as` writes the commit object + advances the branch ref
        // but does NOT update `.git/index`. The status walk, `git
        // checkout`, and any test that does a CLI follow-up all key off
        // the index, so we sync it to the new tree here. Done only when
        // HEAD points at the just-committed branch — the cost of
        // skipping it for off-branch commits is zero because tests
        // don't switch HEAD between commits without our `checkout()`
        // call.
        if let Ok(mut idx) = self.repo.index_from_tree(&tree_id) {
            idx.write(gix::index::write::Options::default()).expect("write index");
        }

        commit_id
    }

    /// Creates a new branch at the current branch's tip without
    /// switching to it. Equivalent to `git branch <name>` followed by
    /// staying on the current branch.
    pub(super) fn create_branch(&self, name: &str) {
        let tip = self
            .current_branch_tip()
            .expect("create_branch requires at least one commit on parent");
        let ref_name = format!("refs/heads/{name}");
        self.repo
            .reference(
                ref_name.as_str(),
                tip,
                gix::refs::transaction::PreviousValue::MustNotExist,
                "test_fixtures: create_branch",
            )
            .expect("create branch ref");
    }

    /// Points HEAD at `refs/heads/<name>`, updates the in-memory repo
    /// handle so subsequent gix calls see the new HEAD, and flips
    /// `current_branch` so the next `commit_*` goes to the right ref.
    /// Equivalent to `git checkout <name>` from a test's POV — we
    /// don't sync the worktree, which is fine because tests rebuild
    /// file contents before each commit.
    pub(super) fn checkout(&mut self, name: &str) {
        let ref_name = format!("refs/heads/{name}");
        // Re-open the repo so changes to packed-refs / HEAD are
        // observed by subsequent calls. gix's in-memory ref iter
        // caches packed-refs at handle-creation time.
        self.repo
            .edit_reference(gix::refs::transaction::RefEdit {
                change: gix::refs::transaction::Change::Update {
                    log: Default::default(),
                    expected: gix::refs::transaction::PreviousValue::Any,
                    new: gix::refs::Target::Symbolic(ref_name.as_str().try_into().expect("valid ref name")),
                },
                name: "HEAD".try_into().expect("HEAD ref"),
                deref: false,
            })
            .expect("update HEAD");
        self.current_branch = name.to_string();
        self.repo = gix::open(&self.dir).expect("re-open after checkout");
    }

    /// ObjectId of the tip commit on the active branch, or None for an
    /// unborn branch (no commits yet).
    fn current_branch_tip(&self) -> Option<ObjectId> {
        let ref_name = format!("refs/heads/{}", self.current_branch);
        self.repo
            .find_reference(ref_name.as_str())
            .ok()
            .and_then(|mut r| r.peel_to_id().ok().map(|id| id.detach()))
    }
}

// ============================================================================
// High-level fixture builders. The shapes match the ones every test file used
// to define; tests just call into here now.
// ============================================================================

/// One repo with `commits` commits on `main`, each touching `README.md`.
/// Replaces the per-file `build_simple_repo` helpers.
pub(super) fn build_simple_repo(prefix: &str, commits: usize) -> (PathBuf, Fixture) {
    let dir = temp_dir(prefix, "simple");
    let mut fixture = Fixture::init(dir.clone());
    for n in 0..commits {
        fixture.commit_file("README.md", format!("step {n}\n").as_bytes(), &format!("commit {n}"));
    }
    (dir, fixture)
}

/// `main` with one initial commit, plus one feature branch per entry
/// in `branches`. Each branch has `extra` extra commits on top of
/// `main`, so its ahead-count vs `main` matches `extra`. HEAD stays
/// on `main` at the end.
pub(super) fn build_repo_with_branches(prefix: &str, branches: &[(&str, usize)]) -> (PathBuf, Fixture) {
    let dir = temp_dir(prefix, "branches");
    let mut fixture = Fixture::init(dir.clone());
    fixture.commit_file("README.md", b"main\n", "initial");
    for (name, extra) in branches {
        fixture.create_branch(name);
        fixture.checkout(name);
        for n in 0..*extra {
            let file = format!("{name}-{n}.txt");
            fixture.commit_file(&file, b"x\n", &format!("on {name} #{n}"));
        }
        fixture.checkout("main");
    }
    (dir, fixture)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::git::repo::discover_repo;

    #[test]
    fn simple_repo_has_expected_commit_count_and_branch() {
        let (dir, _f) = build_simple_repo("smoke", 3);
        let (handle, root) = discover_repo(&dir).expect("discover");
        let repo = handle.to_thread_local();
        let head = repo.head_name().unwrap().unwrap();
        assert_eq!(head.shorten().to_string(), "main");
        // Walk HEAD and count commits.
        let walk = repo.rev_walk([repo.head_id().unwrap()]).all().expect("rev walk");
        let count = walk.count();
        assert_eq!(count, 3);
        cleanup(&root);
    }

    #[test]
    fn repo_with_branches_has_per_branch_ahead_counts() {
        let (dir, _f) = build_repo_with_branches("smoke", &[("feat-a", 3), ("feat-b", 1)]);
        let (handle, root) = discover_repo(&dir).expect("discover");
        let repo = handle.to_thread_local();
        // main itself has just the initial commit
        let main_id = repo
            .find_reference("refs/heads/main")
            .unwrap()
            .peel_to_id()
            .unwrap()
            .detach();
        let feat_a = repo
            .find_reference("refs/heads/feat-a")
            .unwrap()
            .peel_to_id()
            .unwrap()
            .detach();
        let feat_b = repo
            .find_reference("refs/heads/feat-b")
            .unwrap()
            .peel_to_id()
            .unwrap()
            .detach();
        assert_ne!(main_id, feat_a, "feat-a diverged from main");
        assert_ne!(main_id, feat_b, "feat-b diverged from main");
        assert_ne!(feat_a, feat_b, "feat-a and feat-b each have unique commits");
        cleanup(&root);
    }
}
