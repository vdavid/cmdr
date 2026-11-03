//! When Cmdr can't read one of its own state files (a settings store, a recents list, a
//! database), it sets the file aside, starts that state fresh, and tells the person once: which
//! state reset and where the old copy went. The CRA's Annex I Part I (2)(f) asks to "report on
//! corruptions"; this is that report.
//!
//! Every store that recovers calls [`set_aside`] (or [`report`] when nothing could be kept). That
//! records one typed `StateFileRecovered` in the security log and queues one [`StateRecovery`]
//! for the main window, which drains the queue through `take_state_recoveries` at start and on
//! every [`StateRecoveryQueued`] nudge. The plugin stores are checked before anything loads
//! them: [`recover_plugin_stores`]. Cmdr's own JSON stores read and write through
//! [`read_state_file`] and [`write_state_file`], which never let a file that didn't load be saved
//! over. Design: `DETAILS.md`.

mod plugin_stores;
mod state_file;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use tauri::AppHandle;
use tauri_specta::Event;

use crate::ignore_poison::IgnorePoison;
use crate::security_log::{self, SecurityEvent};
use cmdr_fs::log_private::LogPrivate;

pub use crate::security_log::StateFile;
pub use plugin_stores::{license_file_recovered, recover_plugin_stores, settings_for_consent};
pub use state_file::{StateRead, StateWriteError, lenient_entries, read_state_file, write_state_file};

const LOG_TARGET: &str = "state_recovery";

/// How many set-aside copies of one file stay. A file that keeps breaking can't fill the disk,
/// and the newest copies are the ones worth looking at.
const KEPT_COPIES: usize = 3;

/// One state file Cmdr couldn't read and started fresh, as the person hears about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StateRecovery {
    pub file: StateFile,
    /// Where the unreadable file went, or `None` when it couldn't be kept.
    pub kept_copy: Option<String>,
}

/// Typed `state-recovery-queued` event: a recovery is waiting in the queue. Carries nothing on
/// purpose: the main window drains the queue, so a recovery is shown once however many nudges
/// and window starts race it.
///
/// `Serialize` only beside the derive, like `SecurityNoticeRaised`: nothing listens Rust-side.
#[derive(Debug, Clone, Serialize, specta::Type, Event)]
#[tauri_specta(event_name = "state-recovery-queued")]
pub struct StateRecoveryQueued;

static APP: OnceLock<AppHandle> = OnceLock::new();
static QUEUE: Mutex<Vec<StateRecovery>> = Mutex::new(Vec::new());

/// Lets later recoveries nudge the windows. Recoveries before this wait in the queue, which is
/// how the ones `setup()` finds reach a window that doesn't exist yet.
pub fn init(app: &AppHandle) {
    let _ = APP.set(app.clone());
}

/// Moves the unreadable `path` to a timestamped sibling (`settings.json.broken-2026-10-16T101500`),
/// keeps the newest [`KEPT_COPIES`], and reports the recovery. Returns where the copy went, or
/// `None` when the rename didn't happen, which leaves the file where it was for the caller to
/// deal with.
pub fn set_aside(path: &Path, file: StateFile) -> Option<PathBuf> {
    let kept = match move_aside(path, chrono::Local::now().naive_local()) {
        Ok(kept) => {
            log::warn!(
                target: LOG_TARGET,
                // allowed-log-private: `file` is the `StateFile` variant, not a path
                "Set the unreadable {file:?} file aside at {kept:?}",
                kept = LogPrivate::file_path(&kept)
            );
            Some(kept)
        }
        Err(e) => {
            log::warn!(
                target: LOG_TARGET,
                // allowed-log-private: `file` is the `StateFile` variant, not a path
                "Couldn't set the unreadable {file:?} file at {path:?} aside: {e}",
                path = LogPrivate::file_path(path)
            );
            None
        }
    };
    report(file, kept.as_deref());
    kept
}

/// Copies `path` to a timestamped sibling the way [`set_aside`] names it, keeps the newest
/// [`KEPT_COPIES`], and reports the recovery, while the file itself stays. For a file that read
/// only in part: the parts that read carry on, and the copy keeps the rest. Returns where the copy
/// went, or `None` when it couldn't be made.
///
/// A copy of these exact bytes already there (an earlier launch made it, and nothing has saved over
/// the file since) is the answer as it is: a second one would push an older, different copy out
/// of the newest [`KEPT_COPIES`], and the person already heard about this one.
pub fn keep_copy(path: &Path, file: StateFile) -> Option<PathBuf> {
    if let Some(existing) = identical_copy(path) {
        log::debug!(
            target: LOG_TARGET,
            // allowed-log-private: `file` is the `StateFile` variant, not a path
            "The partly unreadable {file:?} file already has its copy at {existing:?}",
            existing = LogPrivate::file_path(&existing)
        );
        return Some(existing);
    }
    let kept = match copy_aside(path, chrono::Local::now().naive_local()) {
        Ok(kept) => {
            log::warn!(
                target: LOG_TARGET,
                // allowed-log-private: `file` is the `StateFile` variant, not a path
                "Kept a copy of the partly unreadable {file:?} file at {kept:?}",
                kept = LogPrivate::file_path(&kept)
            );
            Some(kept)
        }
        Err(e) => {
            log::warn!(
                target: LOG_TARGET,
                // allowed-log-private: `file` is the `StateFile` variant, not a path
                "Couldn't keep a copy of the partly unreadable {file:?} file at {path:?}: {e}",
                path = LogPrivate::file_path(path)
            );
            return None;
        }
    };
    report(file, kept.as_deref());
    kept
}

/// Records that `file` started fresh: one security-log event and one queued notice. Use
/// [`set_aside`] when there's a file to keep; this is for the case where nothing could be kept.
pub fn report(file: StateFile, kept_copy: Option<&Path>) {
    security_log::record(SecurityEvent::StateFileRecovered { file });
    let recovery = StateRecovery {
        file,
        kept_copy: kept_copy.map(|path| path.to_string_lossy().into_owned()),
    };
    #[cfg(test)]
    if testing::capture(&recovery) {
        return;
    }
    QUEUE.lock_ignore_poison().push(recovery);
    if let Some(app) = APP.get()
        && let Err(e) = StateRecoveryQueued.emit(app)
    {
        log::warn!(target: LOG_TARGET, "A recovery notice didn't reach the windows: {e}");
    }
}

/// Everything queued since the last take, oldest first. The main window's to call.
pub fn take_queued() -> Vec<StateRecovery> {
    std::mem::take(&mut *QUEUE.lock_ignore_poison())
}

/// The rename and the pruning behind [`set_aside`], over an injected clock.
fn move_aside(path: &Path, now: chrono::NaiveDateTime) -> std::io::Result<PathBuf> {
    let (kept, prefix) = kept_name(path, now)?;
    fs::rename(path, &kept)?;
    prune(path, &prefix);
    Ok(kept)
}

/// The copy and the pruning behind [`keep_copy`], over an injected clock.
fn copy_aside(path: &Path, now: chrono::NaiveDateTime) -> std::io::Result<PathBuf> {
    let (kept, prefix) = kept_name(path, now)?;
    fs::copy(path, &kept)?;
    prune(path, &prefix);
    Ok(kept)
}

/// A kept copy of `path` whose bytes are exactly the file's, if there is one.
fn identical_copy(path: &Path) -> Option<PathBuf> {
    let current = fs::read(path).ok()?;
    let prefix = format!("{}.broken-", path.file_name()?.to_string_lossy());
    fs::read_dir(path.parent()?)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
        .map(|entry| entry.path())
        .find(|copy| {
            fs::metadata(copy).is_ok_and(|m| m.len() == current.len() as u64)
                && fs::read(copy).is_ok_and(|bytes| bytes == current)
        })
}

/// The first free `<name>.broken-<stamp>[-n]` beside `path`, plus the prefix every copy of it
/// shares.
fn kept_name(path: &Path, now: chrono::NaiveDateTime) -> std::io::Result<(PathBuf, String)> {
    let name = path
        .file_name()
        .ok_or_else(|| std::io::Error::other("a state file path has a file name"))?
        .to_string_lossy()
        .into_owned();
    let prefix = format!("{name}.broken-");
    let stamp = now.format("%Y-%m-%dT%H%M%S").to_string();
    let mut kept = path.with_file_name(format!("{prefix}{stamp}"));
    let mut n = 2;
    while kept.exists() {
        kept = path.with_file_name(format!("{prefix}{stamp}-{n}"));
        n += 1;
    }
    Ok((kept, prefix))
}

/// Deletes all but the newest [`KEPT_COPIES`] set-aside copies. The stamp sorts by time, and a
/// same-second `-2` sorts after its first.
fn prune(path: &Path, prefix: &str) {
    let Some(dir) = path.parent() else {
        return;
    };
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut copies: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(prefix))
        .map(|entry| entry.path())
        .collect();
    copies.sort_unstable();
    let excess = copies.len().saturating_sub(KEPT_COPIES);
    for old in copies.into_iter().take(excess) {
        if let Err(e) = fs::remove_file(&old) {
            log::warn!(target: LOG_TARGET, "Couldn't remove the old set-aside copy {old:?}: {e}");
        }
    }
}

#[cfg(test)]
pub mod testing {
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};

    use super::StateRecovery;

    /// Sets or clears macOS's immutable flag on `path`. An immutable file refuses a rename, which
    /// is how a test reaches the path where a set-aside can't happen.
    #[cfg(target_os = "macos")]
    pub fn set_immutable(path: &Path, on: bool) {
        use std::os::unix::ffi::OsStrExt;
        let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).expect("no NUL in a temp path");
        let flags = if on { libc::UF_IMMUTABLE } else { 0 };
        // SAFETY: (test) `c_path` is a valid NUL-terminated string that outlives the call, and
        // `chflags` only reads it.
        let status = unsafe { libc::chflags(c_path.as_ptr(), flags) };
        assert_eq!(status, 0, "chflags on {path:?}");
    }

    /// The set-aside copies of `path`, oldest first.
    pub fn kept_copies(path: &Path) -> Vec<PathBuf> {
        let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
            return Vec::new();
        };
        let prefix = format!("{}.broken-", name.to_string_lossy());
        let mut copies: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
                    .map(|entry| entry.path())
                    .collect()
            })
            .unwrap_or_default();
        copies.sort_unstable();
        copies
    }

    thread_local! {
        static CAPTURE: RefCell<Option<Vec<StateRecovery>>> = const { RefCell::new(None) };
    }

    /// While alive, [`super::report`] on THIS thread lands here instead of the queue, so parallel
    /// tests can't see each other's recoveries.
    #[must_use = "the capture ends when the guard drops"]
    pub struct CaptureGuard(());

    pub fn capture_recoveries() -> CaptureGuard {
        CAPTURE.with_borrow_mut(|slot| *slot = Some(Vec::new()));
        CaptureGuard(())
    }

    impl CaptureGuard {
        /// Everything reported so far, in order.
        pub fn recoveries(&self) -> Vec<StateRecovery> {
            CAPTURE.with_borrow(|slot| slot.clone().unwrap_or_default())
        }
    }

    impl Drop for CaptureGuard {
        fn drop(&mut self) {
            CAPTURE.with_borrow_mut(|slot| *slot = None);
        }
    }

    pub(super) fn capture(recovery: &StateRecovery) -> bool {
        CAPTURE.with_borrow_mut(|slot| match slot {
            Some(captured) => {
                captured.push(recovery.clone());
                true
            }
            None => false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDir;

    fn at(stamp: &str) -> chrono::NaiveDateTime {
        chrono::NaiveDateTime::parse_from_str(stamp, "%Y-%m-%d %H:%M:%S").expect("a test timestamp")
    }

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("read the dir")
            .map(|entry| entry.expect("an entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort_unstable();
        names
    }

    #[test]
    fn a_set_aside_file_keeps_its_bytes_under_a_timestamped_name() {
        let dir = TestDir::new("state_recovery_stamp");
        let path = dir.join("settings.json");
        fs::write(&path, b"{\"trunc").expect("write");

        let kept = move_aside(&path, at("2026-10-16 10:15:00")).expect("moved");

        assert_eq!(kept, dir.join("settings.json.broken-2026-10-16T101500"));
        assert_eq!(fs::read(&kept).expect("read the copy"), b"{\"trunc");
        assert!(!path.exists());
    }

    #[test]
    fn a_second_recovery_in_the_same_second_gets_its_own_name() {
        let dir = TestDir::new("state_recovery_same_second");
        let path = dir.join("favorites.json");
        for bytes in [b"first".as_slice(), b"second"] {
            fs::write(&path, bytes).expect("write");
            move_aside(&path, at("2026-10-16 10:15:00")).expect("moved");
        }
        assert_eq!(
            names_in(&dir),
            vec![
                "favorites.json.broken-2026-10-16T101500",
                "favorites.json.broken-2026-10-16T101500-2"
            ]
        );
    }

    #[test]
    fn only_the_newest_three_copies_stay() {
        let dir = TestDir::new("state_recovery_prune");
        let path = dir.join("usage.json");
        for minute in 10..15 {
            fs::write(&path, b"garbage").expect("write");
            move_aside(&path, at(&format!("2026-10-16 10:{minute}:00"))).expect("moved");
        }
        fs::write(dir.join("usage.json.tmp"), b"unrelated").expect("write");
        assert_eq!(
            names_in(&dir),
            vec![
                "usage.json.broken-2026-10-16T101200",
                "usage.json.broken-2026-10-16T101300",
                "usage.json.broken-2026-10-16T101400",
                "usage.json.tmp",
            ]
        );
    }

    #[test]
    fn a_recovery_is_one_security_event_and_one_notice() {
        let events = security_log::testing::capture_events();
        let notices = testing::capture_recoveries();
        let dir = TestDir::new("state_recovery_report");
        let path = dir.join("favorites.json");
        fs::write(&path, b"garbage").expect("write");

        let kept = set_aside(&path, StateFile::Favorites).expect("moved");

        assert_eq!(
            events.events(),
            vec![SecurityEvent::StateFileRecovered {
                file: StateFile::Favorites
            }]
        );
        assert_eq!(
            notices.recoveries(),
            vec![StateRecovery {
                file: StateFile::Favorites,
                kept_copy: Some(kept.to_string_lossy().into_owned()),
            }]
        );
    }

    #[test]
    fn a_file_that_cant_move_still_reports_with_no_copy() {
        let notices = testing::capture_recoveries();
        let dir = TestDir::new("state_recovery_missing");

        assert_eq!(set_aside(&dir.join("gone.json"), StateFile::SearchHistory), None);
        assert_eq!(
            notices.recoveries(),
            vec![StateRecovery {
                file: StateFile::SearchHistory,
                kept_copy: None
            }]
        );
    }
}
