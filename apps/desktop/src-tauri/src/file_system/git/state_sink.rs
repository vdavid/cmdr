//! Where the `.git/*` watcher reports what it saw.
//!
//! The watcher's job is to notice that a repository's mutable state moved and
//! to recompute [`RepoInfo`]. Telling a window about it is the host's job, and
//! [`GitStateSink`] is the one call between the two. That's what lets the
//! watcher hold no `AppHandle`: it reports a repo root and a typed snapshot,
//! and whoever is listening decides what a user sees.

use std::path::Path;
use std::sync::{Arc, LazyLock};

use super::repo::RepoInfo;

/// Reports that a repository's mutable state changed.
///
/// One call per debounced burst of `.git/*` writes, carrying a freshly computed
/// snapshot. The implementation runs on the watcher's own thread, so it must
/// not block for long and must not panic.
pub trait GitStateSink: Send + Sync {
    /// `repo_root`'s state moved, and `info` is what it says now.
    fn repo_changed(&self, repo_root: &Path, info: RepoInfo);
}

/// Nobody is listening: every change goes nowhere.
///
/// The right answer for a test, a bench, or any session with no window open,
/// and it's why the watcher never needs an `Option<AppHandle>`.
pub struct NoGitStateSink;

impl GitStateSink for NoGitStateSink {
    fn repo_changed(&self, _repo_root: &Path, _info: RepoInfo) {}
}

/// The shared detached sink, so a caller with nowhere to report doesn't
/// allocate one per call.
pub fn no_git_state_sink() -> Arc<dyn GitStateSink> {
    static DETACHED: LazyLock<Arc<dyn GitStateSink>> = LazyLock::new(|| Arc::new(NoGitStateSink));
    Arc::clone(&DETACHED)
}

#[cfg(test)]
pub use recording::RecordingGitStateSink;

#[cfg(test)]
mod recording {
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    use super::{GitStateSink, RepoInfo};
    use cmdr_fs::ignore_poison::IgnorePoison;

    /// A [`GitStateSink`] that remembers what it was told, so a test can assert
    /// on what a subscriber would have seen: how many times, for which repo,
    /// and with which snapshot.
    #[derive(Default)]
    pub struct RecordingGitStateSink {
        changes: Mutex<Vec<(PathBuf, RepoInfo)>>,
    }

    impl RecordingGitStateSink {
        /// A recorder with nothing reported yet.
        pub fn new() -> Self {
            Self::default()
        }

        /// Every change reported so far, in order.
        pub fn changes(&self) -> Vec<(PathBuf, RepoInfo)> {
            self.changes.lock_ignore_poison().clone()
        }

        /// How many changes were reported. The instrument for "one report per
        /// debounced burst, not one per `.git/*` write".
        pub fn count(&self) -> usize {
            self.changes.lock_ignore_poison().len()
        }
    }

    impl GitStateSink for RecordingGitStateSink {
        fn repo_changed(&self, repo_root: &Path, info: RepoInfo) {
            self.changes.lock_ignore_poison().push((repo_root.to_path_buf(), info));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::repo::RepoInfo;
    use super::{GitStateSink, RecordingGitStateSink, no_git_state_sink};
    use std::path::Path;

    fn a_snapshot() -> RepoInfo {
        RepoInfo {
            repo_root: "/repo".to_string(),
            branch: Some("main".to_string()),
            detached_sha: None,
            unborn: false,
            upstream: None,
            ahead: None,
            behind: None,
            is_dirty: false,
        }
    }

    /// The detached sink is what a watcher with no window reports into, and it
    /// has to swallow rather than panic: every test binary and every bench runs
    /// that way.
    #[test]
    fn the_detached_sink_swallows_a_change() {
        no_git_state_sink().repo_changed(Path::new("/repo"), a_snapshot());
    }

    #[test]
    fn the_recorder_keeps_the_root_and_the_snapshot() {
        let sink = RecordingGitStateSink::new();
        sink.repo_changed(Path::new("/repo"), a_snapshot());

        assert_eq!(sink.count(), 1);
        let (root, info) = sink.changes().remove(0);
        assert_eq!(root, Path::new("/repo"));
        assert_eq!(info.branch.as_deref(), Some("main"));
    }
}
