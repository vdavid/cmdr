//! Single-instance guard: exactly one Cmdr process per data dir.
//!
//! Two processes sharing one data dir means two index writers on one SQLite file. They seed their
//! entry-ID counter independently, hand out the same IDs, and the loser drowns in
//! `UNIQUE constraint failed: entries.id` plus a `SQLITE_BUSY` storm. That's silent index
//! corruption, so it has to be structurally impossible rather than merely unlikely.
//!
//! The mechanism is an advisory whole-file lock (`std::fs::File::try_lock`, `flock` on unix and
//! `LockFileEx` on Windows) on `<data dir>/.instance.lock`. The kernel owns the lock, so it's
//! released when the process dies for ANY reason, `SIGKILL` and a panic included. There is no such
//! thing as a stale lock to clean up, which is exactly why this isn't a PID file.
//!
//! The pid we write into the file is diagnostics only: it names the holder in the log line when
//! acquisition fails. Nothing branches on it, and unreadable or garbage content is treated as
//! "unknown holder".

use std::fs::{File, OpenOptions};
use std::io::{Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Lock file name inside the resolved data dir.
const LOCK_FILE_NAME: &str = ".instance.lock";

/// How long we keep retrying before declaring the data dir taken.
///
/// A bounded retry, not a single attempt: two legitimate flows briefly overlap two processes
/// against one data dir, and both must survive.
///
/// - `scripts/i18n-capture.ts` relaunches the app repeatedly without waiting for the previous exit.
/// - The in-app updater's `relaunch()` spawns the new process before the old one is gone.
///
/// A user quitting and immediately relaunching lands in the same window.
const ACQUIRE_WINDOW: Duration = Duration::from_secs(5);

/// Poll interval inside [`ACQUIRE_WINDOW`].
const RETRY_INTERVAL: Duration = Duration::from_millis(50);

/// The acquired lock file, parked for the process lifetime.
///
/// Dropping the `File` closes the descriptor, and closing it releases the kernel lock. So the
/// handle must outlive everything that touches the data dir, which in practice means "forever":
/// a second Cmdr could otherwise walk in mid-session and start writing the same index.
static HELD_LOCK: OnceLock<File> = OnceLock::new();

/// Alert title shown when another instance owns the data dir.
const ALERT_TITLE: &str = "Cmdr is already running";

/// Alert body shown when another instance owns the data dir.
const ALERT_BODY: &str =
    "Another copy of Cmdr is using this data folder. Switch to the running app, or quit it and try again.";

/// Path of the lock file for a given data dir.
pub fn lock_path(data_dir: &Path) -> PathBuf {
    data_dir.join(LOCK_FILE_NAME)
}

/// Why we couldn't take the lock.
#[derive(Debug)]
pub enum AcquireError {
    /// Another process holds it. `holder_pid` is best-effort diagnostics.
    Busy { holder_pid: Option<u32> },
    /// We couldn't open or lock the file at all (permissions, read-only mount, ...).
    Io(std::io::Error),
}

/// Pure core: read a pid out of the lock file's contents.
///
/// Anything that isn't a plain positive integer is "unknown holder". The file is diagnostics, so a
/// truncated or half-written value must never turn into a wrong claim in the log.
fn parse_holder_pid(contents: &str) -> Option<u32> {
    contents.trim().parse::<u32>().ok().filter(|&pid| pid > 0)
}

/// Pure core: how we name the holder in the log line.
fn holder_description(holder_pid: Option<u32>) -> String {
    match holder_pid {
        Some(pid) => format!("process {pid}"),
        None => "an unknown process".to_string(),
    }
}

/// Pure core: the operator-facing log line for a refused acquisition.
fn busy_log_message(data_dir: &Path, holder_pid: Option<u32>) -> String {
    format!(
        "Another Cmdr instance ({}) already owns the data dir {}. Two processes on one data dir corrupt the index, so this one is stopping.",
        holder_description(holder_pid),
        data_dir.display()
    )
}

/// Best-effort: stamp our pid into the lock file for diagnostics.
fn write_pid(file: &File) {
    let mut file = file;
    let result = file
        .set_len(0)
        .and_then(|()| file.seek(SeekFrom::Start(0)))
        .and_then(|_| file.write_all(std::process::id().to_string().as_bytes()))
        .and_then(|()| file.flush());
    if let Err(e) = result {
        log::debug!(target: "instance_lock", "Couldn't stamp our pid into the lock file: {e}");
    }
}

/// Best-effort: who holds the lock right now, per the file's contents.
fn read_holder_pid(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok().as_deref().and_then(parse_holder_pid)
}

/// Takes the lock, retrying for `window` before giving up.
///
/// `window` is a parameter so tests can prove the refusal path without burning the full production
/// wait.
fn acquire_within(data_dir: &Path, window: Duration) -> Result<File, AcquireError> {
    let path = lock_path(data_dir);
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(AcquireError::Io)?;

    let deadline = Instant::now() + window;
    loop {
        match file.try_lock() {
            Ok(()) => {
                write_pid(&file);
                return Ok(file);
            }
            Err(std::fs::TryLockError::WouldBlock) => {
                if Instant::now() >= deadline {
                    return Err(AcquireError::Busy {
                        holder_pid: read_holder_pid(&path),
                    });
                }
                std::thread::sleep(RETRY_INTERVAL);
            }
            Err(std::fs::TryLockError::Error(e)) => return Err(AcquireError::Io(e)),
        }
    }
}

/// Takes the lock for this data dir, retrying for [`ACQUIRE_WINDOW`].
pub fn acquire(data_dir: &Path) -> Result<File, AcquireError> {
    acquire_within(data_dir, ACQUIRE_WINDOW)
}

/// Claims the data dir for this process, or tells the user and exits.
///
/// Call this once at startup, right after the logger is up and before anything opens a database or
/// shows a window. On refusal the process is gone before it can touch a single index file.
///
/// An I/O problem (unwritable data dir, exotic filesystem with no `flock`) is NOT treated as
/// refusal: that would turn an unrelated filesystem fault into "Cmdr won't start". We log it loudly
/// and continue unlocked, which is exactly the behavior we had before this guard existed.
pub fn claim_data_dir_or_exit(data_dir: &Path) {
    match acquire(data_dir) {
        Ok(file) => {
            if HELD_LOCK.set(file).is_err() {
                log::warn!(target: "instance_lock", "Instance lock claimed twice; keeping the first handle.");
            }
            log::debug!(target: "instance_lock", "Instance lock held for {}", data_dir.display());
        }
        Err(AcquireError::Busy { holder_pid }) => {
            crate::log_error!(target: "instance_lock", "{}", busy_log_message(data_dir, holder_pid));
            // Under E2E nobody is there to click OK, and a modal that waits forever turns a clear
            // "data dir already taken" into an opaque harness timeout. The log line above is what
            // a test run needs; exit straight away.
            if !crate::test_mode::is_e2e_mode() {
                show_already_running_alert();
            }
            std::process::exit(1);
        }
        Err(AcquireError::Io(e)) => {
            log::warn!(
                target: "instance_lock",
                "Couldn't take the instance lock in {}: {e}. Continuing without it; don't run a second Cmdr on this data dir.",
                data_dir.display()
            );
        }
    }
}

/// Native "already running" alert.
///
/// A signed `.app` has no visible stderr, so the dialog is the only thing the user actually sees.
/// We run `NSAlert` synchronously here rather than going through `tauri-plugin-dialog`: its
/// `blocking_show` explicitly must not run on the main thread (it hands the dialog to
/// `run_on_main_thread` and waits), and Tauri's `setup` hook IS the main thread with the event loop
/// not yet running, so it would deadlock. `NSAlert::runModal` spins its own modal loop and needs
/// nothing but an initialized `NSApplication`, which Tauri has already created by `setup` time.
#[cfg(target_os = "macos")]
fn show_already_running_alert() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSAlert;
    use objc2_foundation::NSString;

    let Some(mtm) = MainThreadMarker::new() else {
        log::warn!(target: "instance_lock", "Not on the main thread; skipping the already-running alert.");
        return;
    };
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(ALERT_TITLE));
    alert.setInformativeText(&NSString::from_str(ALERT_BODY));
    alert.runModal();
}

/// Non-macOS: no native alert surface we can rely on at this point in startup, so the log line and
/// the non-zero exit are the whole story.
#[cfg(not(target_os = "macos"))]
fn show_already_running_alert() {
    log::warn!(target: "instance_lock", "{ALERT_TITLE}. {ALERT_BODY}");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two `File` handles opened independently in the SAME process DO conflict under POSIX
    /// `flock`: the lock belongs to the open file description, not the process. That's what makes
    /// the contention path testable in-process, without spawning a second binary.
    #[test]
    fn second_acquisition_is_refused_while_the_first_lives() {
        let dir = tempfile::tempdir().expect("create temp dir");

        let first = acquire_within(dir.path(), Duration::from_millis(0)).expect("first acquire");

        let second = acquire_within(dir.path(), Duration::from_millis(120));
        assert!(
            matches!(second, Err(AcquireError::Busy { .. })),
            "a second acquisition must be refused while the first handle is alive, got {second:?}"
        );

        drop(first);
    }

    #[test]
    fn acquisition_succeeds_again_after_the_first_guard_drops() {
        let dir = tempfile::tempdir().expect("create temp dir");

        let first = acquire_within(dir.path(), Duration::from_millis(0)).expect("first acquire");
        drop(first);

        // The kernel released the lock on close, so this must go through immediately.
        acquire_within(dir.path(), Duration::from_millis(0)).expect("re-acquire after drop");
    }

    #[test]
    fn acquiring_stamps_our_pid_and_names_the_holder() {
        let dir = tempfile::tempdir().expect("create temp dir");

        let held = acquire_within(dir.path(), Duration::from_millis(0)).expect("acquire");
        assert_eq!(read_holder_pid(&lock_path(dir.path())), Some(std::process::id()));

        drop(held);
    }

    #[test]
    fn holder_pid_parses_only_plain_positive_integers() {
        assert_eq!(parse_holder_pid("1234"), Some(1234));
        assert_eq!(parse_holder_pid(" 1234\n"), Some(1234));
        assert_eq!(parse_holder_pid("0"), None);
        assert_eq!(parse_holder_pid(""), None);
        assert_eq!(parse_holder_pid("12 34"), None);
        assert_eq!(parse_holder_pid("pid=1234"), None);
        assert_eq!(parse_holder_pid("-1"), None);
    }

    #[test]
    fn holder_description_falls_back_to_unknown() {
        assert_eq!(holder_description(Some(42)), "process 42");
        assert_eq!(holder_description(None), "an unknown process");
    }

    #[test]
    fn busy_log_message_names_the_dir_and_the_holder() {
        assert_eq!(
            busy_log_message(Path::new("/tmp/cmdr-data"), Some(42)),
            "Another Cmdr instance (process 42) already owns the data dir /tmp/cmdr-data. \
             Two processes on one data dir corrupt the index, so this one is stopping."
        );
    }
}
