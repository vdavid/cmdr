//! The two things a Rust test needs from the host: somewhere to write, and a way to wait.
//!
//! [`TestDir`] is the scratch directory. [`wait_until`] serves sync `#[test]`s,
//! [`wait_until_async`] serves `#[tokio::test]`s; both poll a condition to a
//! deadline and panic when it never holds, so a wait can't silently pass. Don't
//! hand-roll a poll loop, and don't sleep a fixed span hoping the work landed:
//! the sleep inside those two helpers is the only sanctioned one in Rust test
//! code, and the `test-sleep` check enforces that.
//!
//! Gated behind the `testing` feature, so it exists in dev targets and in no
//! shipped build.

use std::future::Future;
use std::panic::Location;
use std::path::Path;
use std::time::{Duration, Instant};

/// A scratch directory owned by ONE test run, removed when the handle drops.
///
/// ```ignore
/// let dir = TestDir::new("listing_sort");
/// std::fs::write(dir.join("a.txt"), b"x").unwrap();
/// ```
///
/// **Why not `std::env::temp_dir().join("cmdr_something")`.** That path is
/// shared by every process on the machine, which costs three ways:
///
/// 1. Two suite runs at once (parallel worktrees, or CI beside a local run) get
///    the same directory, and whichever calls `remove_dir_all` first deletes the
///    other's live fixture mid-test. Running each test in its own process
///    (nextest) doesn't help: processes share the filesystem.
/// 2. A run that doesn't clean up leaves the next one a pre-populated directory,
///    so "the listing has three entries" can pass on leftovers and go red later
///    for no reason anyone can reproduce.
/// 3. Teardown written as a `remove_dir_all` after the assertions never runs
///    when an assertion fails, which is exactly when the mess is worst.
///
/// A `TestDir` is process-unique (a random suffix), and its `Drop` runs on
/// unwind, so a failing test cleans up after itself. `label` is cosmetic: it
/// names the directory readably while it exists.
///
/// Keep the handle bound for as long as you need the files (`let dir = …`, never
/// `let _ = …`): a `_` binding drops immediately and takes the directory with it.
///
/// ❌ **Both [`Deref`](std::ops::Deref) and [`AsRef<Path>`] below are
/// load-bearing; neither is redundant.** `Deref` is what lets a converted test
/// body keep reading like the `PathBuf` it replaced (`dir.join("a.txt")`,
/// `dir.to_string_lossy()`). `AsRef` is what a generic `impl AsRef<Path>`
/// parameter takes, and deref coercion cannot reach through a type parameter:
/// `LocalPosixVolume::new("Test", &dir)` fails to compile without it.
/// `tempfile::TempDir` ships only the `AsRef` half, which is exactly why this
/// wrapper exists.
#[derive(Debug)]
pub struct TestDir(tempfile::TempDir);

impl TestDir {
    /// Creates an empty scratch directory, named after `label` for readability.
    #[track_caller]
    pub fn new(label: &str) -> Self {
        Self(
            tempfile::Builder::new()
                .prefix(&format!("cmdr_{label}_"))
                .tempdir()
                .expect("failed to create a test scratch directory"),
        )
    }
}

// Both impls, on purpose — see the `TestDir` doc comment before deleting either.
impl std::ops::Deref for TestDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        self.0.path()
    }
}

impl AsRef<Path> for TestDir {
    fn as_ref(&self) -> &Path {
        self.0.path()
    }
}

/// How often we re-check the condition: short enough that a satisfied wait returns promptly, long
/// enough that a cheap predicate doesn't spin a core.
const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// Polls `condition` until it holds, panicking after `timeout` if it never does.
///
/// `description` finishes the sentence "timed out after 2s waiting for …", so phrase it as a noun
/// phrase: `"the ByteSeek to LineIndex upgrade to finish"`.
///
/// ❌ Don't call this from an `async` test: `std::thread::sleep` blocks the runtime worker and
/// deadlocks a current-thread scheduler. Use [`wait_until_async`] there.
#[track_caller]
pub fn wait_until(timeout: Duration, description: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    loop {
        if condition() {
            return;
        }
        assert!(Instant::now() < deadline, "{}", timed_out(timeout, description));
        // allowed-test-sleep: the sanctioned poll interval; every sync test wait routes through here
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// The async twin of [`wait_until`], for `#[tokio::test]`s.
///
/// Deadline and poll both run on tokio's clock, so a `start_paused` runtime auto-advances through
/// the waiting instead of burning wall-clock.
///
/// This is a plain `fn` returning a future rather than an `async fn` on purpose: `#[track_caller]`
/// doesn't reach through the future an `async fn` generates, so we capture the call site eagerly
/// and put it in the panic message instead.
#[track_caller]
pub fn wait_until_async<'a>(
    timeout: Duration,
    description: &'a str,
    mut condition: impl FnMut() -> bool + 'a,
) -> impl Future<Output = ()> + 'a {
    let caller = Location::caller();
    async move {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if condition() {
                return;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "{} (at {caller})",
                timed_out(timeout, description)
            );
            // allowed-test-sleep: the sanctioned poll interval; every async test wait routes through here
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }
}

fn timed_out(timeout: Duration, description: &str) -> String {
    format!("timed out after {timeout:.1?} waiting for {description}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_already_true_condition_returns_without_waiting() {
        let started = Instant::now();
        wait_until(Duration::from_secs(30), "an always-true condition", || true);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn a_condition_that_turns_true_later_is_picked_up() {
        let mut polls = 0;
        wait_until(Duration::from_secs(5), "the third poll", || {
            polls += 1;
            polls >= 3
        });
        assert_eq!(polls, 3);
    }

    #[test]
    #[should_panic(expected = "timed out after 20.0ms waiting for a condition that never holds")]
    fn a_condition_that_never_holds_panics_with_the_description() {
        wait_until(Duration::from_millis(20), "a condition that never holds", || false);
    }

    #[tokio::test]
    async fn an_already_true_condition_returns_without_waiting_async() {
        let started = Instant::now();
        wait_until_async(Duration::from_secs(30), "an always-true condition", || true).await;
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[tokio::test]
    async fn a_condition_that_turns_true_later_is_picked_up_async() {
        let mut polls = 0;
        wait_until_async(Duration::from_secs(5), "the third poll", || {
            polls += 1;
            polls >= 3
        })
        .await;
        assert_eq!(polls, 3);
    }

    #[tokio::test]
    #[should_panic(expected = "timed out after 20.0ms waiting for a condition that never holds")]
    async fn a_condition_that_never_holds_panics_with_the_description_async() {
        wait_until_async(Duration::from_millis(20), "a condition that never holds", || false).await;
    }

    #[test]
    fn two_dirs_with_the_same_label_do_not_share_a_path() {
        let a = TestDir::new("same_label");
        let b = TestDir::new("same_label");
        assert_ne!(*a, *b, "a shared path is the collision this type exists to prevent");
        assert!(a.exists() && b.exists());
    }

    #[test]
    fn dropping_the_handle_removes_the_directory_and_its_contents() {
        let dir = TestDir::new("drop_cleanup");
        let path = dir.to_path_buf();
        std::fs::write(dir.join("leftover.txt"), b"x").expect("write");
        drop(dir);
        assert!(!path.exists(), "a dropped TestDir must leave nothing behind");
    }

    #[test]
    fn a_fresh_dir_starts_empty() {
        // Cross-run contamination is the second failure mode: a test that asserts
        // on a directory's contents has to start from a known-empty one.
        let dir = TestDir::new("fresh");
        assert_eq!(std::fs::read_dir(&dir).expect("read_dir").count(), 0);
    }

    #[test]
    fn the_timeout_message_names_the_budget_and_the_condition() {
        assert_eq!(
            timed_out(Duration::from_secs(2), "the upgrade to finish"),
            "timed out after 2.0s waiting for the upgrade to finish"
        );
    }
}
