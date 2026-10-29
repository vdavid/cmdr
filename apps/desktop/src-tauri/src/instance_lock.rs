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
//!
//! The claim runs BEFORE the logger is installed (`lib.rs`), so a refused instance never opens the
//! shared log files: it used to append to `cmdr.log`, and near 50 MB it could rotate the running
//! app's live file (#812). So nothing here logs during the claim. A refusal goes to stderr and the
//! alert; every other outcome comes back as a [`Claim`] that `lib.rs` logs once the logger is up.

use cmdr_fs::log_private::LogPrivate;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{Seek as _, SeekFrom, Write};
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

/// Catalog key for the alert title shown when another instance owns the data dir.
///
/// Resolved through [`crate::intl::menu_t`] rather than the webview's `t()`,
/// because this alert is the strongest reason the catalog lookup lives in Rust
/// at all: it fires BEFORE a webview exists, so a frontend-supplied string could
/// never reach it.
const ALERT_TITLE_KEY: &str = "main.instanceLock.alertTitle";

/// Catalog key for the alert body. See [`ALERT_TITLE_KEY`].
const ALERT_BODY_KEY: &str = "main.instanceLock.alertBody";

/// How long the refusal alert waits for a click before giving up and letting the process exit.
///
/// Long enough that someone who launched Cmdr and glanced away still gets to read it; short enough
/// that a dialog nobody comes back to doesn't sit on screen (with a refused process behind it) for
/// the rest of the session. There's no decision to make in it, only an "OK" to acknowledge.
#[cfg(target_os = "macos")]
const ALERT_TIMEOUT: Duration = Duration::from_secs(60);

/// Extra slack the watchdog gives the alert to return on its own before ending the process anyway.
#[cfg(target_os = "macos")]
const ALERT_WATCHDOG_GRACE: Duration = Duration::from_secs(10);

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

/// Pure core: the operator-facing line for a refused acquisition. It goes to stderr, never the
/// log (see the module docs), so the dir is plain text: nothing here reaches an error report.
fn refusal_message(data_dir: &Path, holder_pid: Option<u32>) -> impl fmt::Display + '_ {
    fmt::from_fn(move |f| {
        write!(
            f,
            "Another Cmdr instance ({}) already owns the data dir {}. Two processes on one data dir corrupt the index, so this one is stopping.",
            holder_description(holder_pid),
            data_dir.display()
        )
    })
}

/// Best-effort, diagnostics-only: stamp our pid into the lock file. A failure only leaves a
/// refused second instance naming "an unknown process". Nothing to log it to yet (module docs).
fn write_pid(file: &File) {
    let mut file = file;
    // Best-effort: the pid is diagnostics only, and the logger isn't up yet to report a failure.
    let _ = file
        .set_len(0)
        .and_then(|()| file.seek(SeekFrom::Start(0)))
        .and_then(|_| file.write_all(std::process::id().to_string().as_bytes()))
        .and_then(|()| file.flush());
}

/// One line to stderr, for the stretch of startup before the logger exists. Best-effort: a closed
/// stderr has nobody to tell.
fn to_stderr(line: impl fmt::Display) {
    // Best-effort: stderr is the last resort; there's nowhere to report its failure.
    let _ = writeln!(std::io::stderr(), "{line}");
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

/// What claiming the data dir found, held until the logger is up. `lib.rs` calls [`Claim::log`]
/// right after `logging::startup::init`.
#[must_use = "log the claim once the logger is up"]
#[derive(Debug)]
pub enum Claim {
    /// We hold the lock. `twice` when one was already parked (a second call keeps the first).
    Held { data_dir: PathBuf, twice: bool },
    /// We couldn't open or lock the file at all, so we run unlocked.
    Unlocked { data_dir: PathBuf, error: std::io::Error },
    /// We couldn't resolve the data dir, so there was nothing to lock.
    Unresolved { error: String },
}

impl Claim {
    /// Records the claim in the log. Call once, after the logger is installed.
    pub fn log(self) {
        match self {
            Claim::Held { data_dir, twice } => {
                if twice {
                    log::warn!(target: "instance_lock", "Instance lock claimed twice; keeping the first handle.");
                }
                log::debug!(target: "instance_lock", "Instance lock held for {}", LogPrivate::dir_path(&data_dir));
            }
            Claim::Unlocked { data_dir, error } => log::warn!(
                target: "instance_lock",
                "Couldn't take the instance lock in {}: {error}. Continuing without it; don't run a second Cmdr on this data dir.",
                LogPrivate::dir_path(&data_dir)
            ),
            Claim::Unresolved { error } => log::warn!(
                target: "instance_lock",
                "Couldn't resolve the data dir for the instance lock: {error}. Continuing without it."
            ),
        }
    }
}

/// Claims the data dir for this process, or tells the user and exits.
///
/// Call this once at startup, BEFORE the logger and before anything opens a database: a refused
/// instance must touch neither the shared log files nor a single index file. It logs nothing
/// itself; log the returned [`Claim`] once the logger is up.
///
/// Tauri has already created the config-declared main window by the time `setup` runs, so this is
/// not "before a window exists". It is before one is ever shown: that window is `"visible": false`
/// and only gets shown later in startup, which the refusal path never reaches.
///
/// An I/O problem (unwritable data dir, exotic filesystem with no `flock`) is NOT treated as
/// refusal: that would turn an unrelated filesystem fault into "Cmdr won't start". It comes back as
/// [`Claim::Unlocked`], logged loudly, and we continue unlocked, which is exactly the behavior we
/// had before this guard existed.
pub fn claim_data_dir_or_exit(data_dir: &Path) -> Claim {
    match acquire(data_dir) {
        Ok(file) => Claim::Held {
            data_dir: data_dir.to_path_buf(),
            twice: HELD_LOCK.set(file).is_err(),
        },
        Err(AcquireError::Busy { holder_pid }) => {
            write_refusal(&mut std::io::stderr(), data_dir, holder_pid);
            // Under E2E nobody is there to click OK, and a modal that waits forever turns a clear
            // "data dir already taken" into an opaque harness timeout. The stderr line above is
            // what a test run needs; exit straight away.
            if !crate::test_mode::is_e2e_mode() {
                show_already_running_alert();
            }
            std::process::exit(1);
        }
        Err(AcquireError::Io(error)) => Claim::Unlocked {
            data_dir: data_dir.to_path_buf(),
            error,
        },
    }
}

/// The refusal, for a dev's terminal or a test harness's captured stderr. Not the log: a refused
/// instance never installs one (module docs).
fn write_refusal(out: &mut impl Write, data_dir: &Path, holder_pid: Option<u32>) {
    // Best-effort: stderr is the last resort; there's nowhere to report its failure.
    let _ = writeln!(out, "{}", refusal_message(data_dir, holder_pid));
}

/// Native "already running" alert, shown without ever pumping our own run loop.
///
/// A signed `.app` has no visible stderr, so the dialog is the only thing the user actually sees.
///
/// `CFUserNotificationDisplayAlert` is the API that fits: the dialog is drawn by the system's
/// `UserNotificationCenter` agent, so our call is a plain blocking wait on this thread, and it
/// carries its own timeout.
///
/// ❌ Don't swap in `NSAlert::runModal` or `tauri-plugin-dialog` here. `runModal` pumps OUR main run
/// loop, so a refused process kept booting behind the unanswered alert: webview and frontend up,
/// updater check fired, files written into the data dir the other instance owns, then an abort when
/// the frontend called a menu command while `setup` was still parked in the modal. And
/// `tauri-plugin-dialog`'s `blocking_show` must not run on the main thread (it hands the dialog to
/// `run_on_main_thread` and waits), which is exactly where `setup` lives, so it would deadlock.
#[cfg(target_os = "macos")]
fn show_already_running_alert() {
    use objc2_core_foundation::{CFOptionFlags, CFString, CFUserNotification, kCFUserNotificationCautionAlertLevel};

    spawn_exit_watchdog(ALERT_TIMEOUT + ALERT_WATCHDOG_GRACE);

    // The catalog lookup is safe here: it reads a compiled-in table plus the OS
    // preference, with no Tauri app, no webview, and no settings store needed.
    let header = CFString::from_str(&crate::intl::menu_t(ALERT_TITLE_KEY));
    let message = CFString::from_str(&crate::intl::menu_t(ALERT_BODY_KEY));
    let mut response: CFOptionFlags = 0;
    // SAFETY: every pointer argument is either `None` (the icon, sound, localization, and button
    // titles are documented as nullable, meaning "system defaults", which is the single "OK" button
    // we want) or a live `CFString` we own for the whole call, and `response_flags` points at a
    // stack local that outlives the call. The call blocks this thread, so both borrows stay valid.
    let err = unsafe {
        CFUserNotification::display_alert(
            ALERT_TIMEOUT.as_secs_f64(),
            kCFUserNotificationCautionAlertLevel,
            None,
            None,
            None,
            Some(&header),
            Some(&message),
            None,
            None,
            None,
            &mut response,
        )
    };
    if err != 0 {
        to_stderr(format_args!(
            "Couldn't show the already-running alert (CFUserNotification error {err})."
        ));
    }
}

/// Ends the process after `after`, whatever the alert is doing.
///
/// `CFUserNotificationDisplayAlert` honors its own timeout, but it depends on a separate system
/// agent; if that agent wedges, the call could outlive it. A process that has been refused the data
/// dir must never be able to linger, so this thread ends it unconditionally.
#[cfg(target_os = "macos")]
fn spawn_exit_watchdog(after: Duration) {
    std::thread::spawn(move || {
        std::thread::sleep(after);
        to_stderr("The already-running alert never returned; exiting anyway.");
        std::process::exit(1);
    });
}

/// Non-macOS: no native alert surface we can rely on at this point in startup, so the stderr lines
/// and the non-zero exit are the whole story.
#[cfg(not(target_os = "macos"))]
fn show_already_running_alert() {
    to_stderr(format_args!(
        "{}. {}",
        crate::intl::menu_t(ALERT_TITLE_KEY),
        crate::intl::menu_t(ALERT_BODY_KEY)
    ));
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

    /// The refusal is a plain stderr line, with no log markers: a refused instance never installs
    /// the logger, so nothing would ever read them (#812).
    #[test]
    fn the_refusal_is_one_plain_line_naming_the_dir_and_the_holder() {
        let mut out = Vec::new();
        write_refusal(&mut out, Path::new("/tmp/cmdr-data"), Some(42));
        assert_eq!(
            String::from_utf8(out).expect("utf-8"),
            "Another Cmdr instance (process 42) already owns the data dir /tmp/cmdr-data. \
             Two processes on one data dir corrupt the index, so this one is stopping.\n"
        );
    }
}
