//! The two things a Rust test needs from the host: somewhere to write, and a way to wait.
//!
//! [`TestDir`] is the scratch directory; [`unique_test_id`] names a test-owned entry
//! in a process-wide map. [`wait_until`] serves sync `#[test]`s,
//! [`wait_until_async`] serves `#[tokio::test]`s; both poll a condition to a
//! deadline and panic when it never holds, so a wait can't silently pass.
//! [`wait_until_async_while_moving`] is the one for throughput-bound work: it
//! fails when the work stops moving, not when a stopwatch runs out.
//! [`holds_within`] is [`wait_until`] answering `false` rather than panicking,
//! for a caller (a `Drop`) that must raise its own error. Don't
//! hand-roll a poll loop, and don't sleep a fixed span hoping the work landed:
//! the sleep inside those helpers is the only sanctioned one in Rust test
//! code, and the `test-sleep` check enforces that.
//!
//! Gated behind the `testing` feature, so it exists in dev targets and in no
//! shipped build.
//!
//! On macOS, `disk_images` is the third thing: synthetic APFS and HFS+ disk
//! images for tests that need a real removable volume, created, attached, and torn
//! down through one guarded runner.
//!
//! On macOS, `metadata_fixture` builds files and folders carrying every metadata
//! class (birth, flags, ACL, xattrs, mode) for the copy-keeps-metadata cells.
//!
//! [`scan_recording`] keeps what a copy scan reported to its recorder, for the
//! cells that pin a backend's walk entry by entry.
//!
//! [`tcp_proxy`] is the fourth: a proxy a network backend's test puts between
//! its client and a shared Docker fixture, so it can cut the connection (closed
//! or silent) without touching a container other runs are using.
//!
//! [`WedgedVolume`] is a volume that never answers: the fixture for every bound
//! that exists to survive a wedged mount.
#![allow(
    clippy::disallowed_methods,
    reason = "test scaffolding (the `testing` feature) runs only in a host's test binary, on its own clocks"
)]

#[cfg(target_os = "macos")]
pub mod disk_images;
#[cfg(target_os = "macos")]
pub mod metadata_fixture;
#[cfg(cmdr_host)]
mod platform;
pub mod scan_recording;
pub mod tcp_proxy;
mod wedged_volume;

#[cfg(cmdr_host)]
pub use platform::*;
pub use wedged_volume::WedgedVolume;

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
/// `LocalPosixVolume::local_folder("Test", &dir, host)` fails to compile without it.
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

/// A process-unique key for a test-owned entry in any global map (a listing id, a
/// font id, an operation id, a volume id). The counter alone would collide across
/// concurrently-running test binaries, so the pid goes in too.
pub fn unique_test_id(tag: &str) -> String {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    format!(
        "test-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
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
pub fn wait_until(timeout: Duration, description: &str, condition: impl FnMut() -> bool) {
    assert!(holds_within(timeout, condition), "{}", timed_out(timeout, description));
}

/// [`wait_until`] that answers instead of panicking: `true` once `condition` holds, `false` if it
/// still doesn't after `timeout`. For a caller that must turn the timeout into its own error, like a
/// `Drop` that may already be unwinding, where a second panic aborts the process.
pub fn holds_within(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if condition() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
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

/// [`wait_until_async`], for work whose length is the work's own: `condition` may take as long as
/// `moving` keeps answering something new, and the wait panics once it answers the same for `quiet`
/// (the work stalled) or at `ceiling` overall.
///
/// For a wait whose length is throughput (bytes onto a shared fixture a loaded runner starves), a
/// stopwatch either trips on a healthy slow run or is too long to catch a hang. `moving` is the
/// work's own progress (bytes done, files done); ❌ never a heartbeat that ticks while stalled.
/// Keep `ceiling` under the test's nextest cap, so it stays the authoritative deadline.
#[track_caller]
pub fn wait_until_async_while_moving<'a, M: PartialEq + std::fmt::Debug + 'a>(
    quiet: Duration,
    ceiling: Duration,
    description: &'a str,
    mut moving: impl FnMut() -> M + 'a,
    mut condition: impl FnMut() -> bool + 'a,
) -> impl Future<Output = ()> + 'a {
    let caller = Location::caller();
    async move {
        let deadline = tokio::time::Instant::now() + ceiling;
        let mut seen = moving();
        let mut moved_at = tokio::time::Instant::now();
        loop {
            if condition() {
                return;
            }
            let now = tokio::time::Instant::now();
            let current = moving();
            if current != seen {
                seen = current;
                moved_at = now;
            }
            assert!(
                now.duration_since(moved_at) < quiet,
                "no progress for {quiet:.1?} waiting for {description} (stuck at {seen:?}) (at {caller})"
            );
            assert!(
                now < deadline,
                "{}, still moving ({seen:?}) (at {caller})",
                timed_out(ceiling, description)
            );
            // allowed-test-sleep: the sanctioned poll interval, as in `wait_until_async`
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

    #[test]
    fn holds_within_answers_false_instead_of_panicking_at_the_deadline() {
        assert!(!holds_within(Duration::from_millis(20), || false));
        let mut polls = 0;
        assert!(holds_within(Duration::from_secs(5), || {
            polls += 1;
            polls >= 3
        }));
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

    /// Work that takes ten times `quiet` but moves the whole way is waited out.
    #[tokio::test(start_paused = true)]
    async fn work_that_keeps_moving_may_take_longer_than_the_quiet_budget() {
        let started = tokio::time::Instant::now();
        let ticks = || started.elapsed().as_millis() / 100;
        wait_until_async_while_moving(
            Duration::from_secs(1),
            Duration::from_secs(60),
            "the slow work",
            ticks,
            || started.elapsed() >= Duration::from_secs(10),
        )
        .await;
    }

    #[tokio::test(start_paused = true)]
    #[should_panic(expected = "no progress for 1.0s waiting for the stuck work (stuck at 7)")]
    async fn work_that_stops_moving_panics_after_the_quiet_budget() {
        wait_until_async_while_moving(
            Duration::from_secs(1),
            Duration::from_secs(60),
            "the stuck work",
            || 7,
            || false,
        )
        .await;
    }

    #[tokio::test(start_paused = true)]
    #[should_panic(expected = "timed out after 5.0s waiting for the endless work, still moving")]
    async fn work_that_moves_forever_panics_at_the_ceiling() {
        let started = tokio::time::Instant::now();
        wait_until_async_while_moving(
            Duration::from_secs(1),
            Duration::from_secs(5),
            "the endless work",
            || started.elapsed().as_millis() / 100,
            || false,
        )
        .await;
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
