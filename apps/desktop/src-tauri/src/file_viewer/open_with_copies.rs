//! What becomes of an "Open with" copy after its pull (`open_with_extract.rs`).
//!
//! An app can save into its copy: the file is `0o444`, but its folder stays writable,
//! so an atomic save (vim `:w!`, BBEdit's safe save) replaces it, and "Save a copy"
//! lands beside it. That content exists nowhere else, so a copy is only ever removed
//! once it's provably as pulled:
//!
//! - **Each pull leaves a record** (`record_pull`): a small sidecar in its launch
//!   folder naming the file and its size, mtime, and inode right after the pull.
//! - **A copy is untouched** when its folder holds exactly that file and the record, and
//!   the file's size, mtime, and inode still match. Anything else (an edit, a second
//!   file, no record at all) counts as changed. The inode catches an atomic save that
//!   keeps the size and puts the old mtime back; ❌ not ctime, which any metadata change
//!   (a mode or an xattr) bumps on a copy whose content nobody changed.
//! - **The startup reaper** ([`reap_launch_folders`]), on a background thread, removes an
//!   untouched copy an earlier run left and moves everything in a changed one to the
//!   Trash, under its own name, logging at `warn`. A folder touched since this process
//!   started is this session's and stays.
//!   When the Trash refuses an item, the folder stays for the next launch: ❌ never a
//!   delete in its place.
//! - **The session sweep** (`sweep_launch_folders`), before each launch's pulls,
//!   removes untouched copies past the newest few once they're old or over a size
//!   budget, so a long session can't fill the disk. "No process holds it open" is
//!   deliberately not the test: Preview and TextEdit read a file and close it while
//!   still showing it, so an open-file check answers "free" for a copy on screen, and
//!   removing a file an app does hold open is harmless (its inode lives until closed).
//!   Unchanged, past the newest few, and old or over budget is what makes a copy safe
//!   to take. Each copy is checked again right before it goes, so an app saving while
//!   the sweep sorts can't slip in between the check and the removal.

use std::path::{Path, PathBuf};
#[cfg(any(target_os = "macos", test))]
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use cmdr_fs::file_identity::FileId;
use cmdr_fs::log_private::LogPrivate;
use serde::{Deserialize, Serialize};

/// The record's name inside a launch folder. Dot-led and Cmdr-prefixed, so no pulled
/// file plausibly carries it; `record_pull` refuses one that does.
const RECORD_NAME: &str = ".cmdr-open-with-pull.json";

/// What one pull left in its launch folder.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct PullRecord {
    /// The copy's file name inside the folder.
    file_name: String,
    size: u64,
    modified: FileTime,
    /// The copy's inode: an atomic save lands a new file, whatever size and mtime it
    /// carries.
    inode: u64,
    /// When the pull finished, which orders copies newest-first for the session sweep.
    pulled_at: FileTime,
}

/// A file's mtime to the nanosecond, as seconds and nanoseconds since the Unix epoch.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone, Copy)]
struct FileTime {
    secs: u64,
    nanos: u32,
}

impl FileTime {
    fn of(time: SystemTime) -> Option<Self> {
        let since = time.duration_since(UNIX_EPOCH).ok()?;
        Some(Self {
            secs: since.as_secs(),
            nanos: since.subsec_nanos(),
        })
    }

    #[cfg(any(target_os = "macos", test))]
    fn to_system_time(self) -> SystemTime {
        UNIX_EPOCH + Duration::new(self.secs, self.nanos)
    }
}

/// Records `copy` as pulled, beside it in its launch folder. Best-effort: a copy with
/// no record counts as changed, so a failure here only means it's kept, never lost.
#[cfg(any(target_os = "macos", test))]
pub(super) fn record_pull(copy: &Path) {
    let result = (|| -> Result<(), String> {
        let folder = copy.parent().ok_or("no launch folder")?;
        let file_name = copy.file_name().ok_or("no file name")?.to_string_lossy().into_owned();
        if file_name == RECORD_NAME {
            return Err("the pulled file has the record's own name".to_string());
        }
        let meta = std::fs::symlink_metadata(copy).map_err(|e| e.to_string())?;
        let modified = meta.modified().ok().and_then(FileTime::of).ok_or("no mtime")?;
        let record = PullRecord {
            file_name,
            size: meta.len(),
            modified,
            inode: FileId::of(&meta).and_then(FileId::node_u64).ok_or("no inode")?,
            pulled_at: FileTime::of(SystemTime::now()).ok_or("no clock")?,
        };
        let json = serde_json::to_vec(&record).map_err(|e| e.to_string())?;
        std::fs::write(folder.join(RECORD_NAME), json).map_err(|e| e.to_string())
    })();
    if let Err(e) = result {
        log::debug!(target: "cmdr_lib::file_viewer", "no pull record for {}, so it'll be kept: {e}", LogPrivate::path(&copy));
    }
}

/// The record of the launch folder at `folder` when it still holds exactly what its
/// pull left, else `None`: changed, or never recorded.
fn untouched_record(folder: &Path) -> Option<PullRecord> {
    let record = std::fs::read(folder.join(RECORD_NAME))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<PullRecord>(&bytes).ok())?;
    let only_the_pull = std::fs::read_dir(folder).ok()?.all(|entry| {
        entry.is_ok_and(|entry| {
            let name = entry.file_name();
            name == RECORD_NAME || name.to_string_lossy() == record.file_name
        })
    });
    let meta = std::fs::symlink_metadata(folder.join(&record.file_name)).ok()?;
    let as_pulled = meta.is_file()
        && meta.len() == record.size
        && FileId::of(&meta).and_then(FileId::node_u64) == Some(record.inode)
        && meta.modified().ok().and_then(FileTime::of) == Some(record.modified);
    (only_the_pull && as_pulled).then_some(record)
}

/// The startup sweep of the open-with dir: every subdir of `dir` that `is_launch_folder`
/// names and an earlier run left goes, an untouched one removed and a changed one's
/// contents handed to `trash`. `trash` is the real Trash in the app and a stand-in under
/// test.
///
/// It runs in the background, beside this session's launches, so it takes only folders
/// last touched before `started` (this process's start, [`left_by_an_earlier_run`]): a
/// pull this session makes is a fresh uuid folder, and an app saving into an old one
/// bumps its mtime, so neither is ever reaped.
pub(super) fn reap_launch_folders(
    dir: &Path,
    is_launch_folder: impl Fn(&str) -> bool,
    started: SystemTime,
    trash: &dyn Fn(&Path) -> Result<(), String>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return; // dir doesn't exist yet (first run) — nothing to reap.
    };
    for entry in entries.flatten() {
        let folder = entry.path();
        if !is_launch_folder(&entry.file_name().to_string_lossy()) || !left_by_an_earlier_run(&folder, started) {
            continue;
        }
        if untouched_record(&folder).is_none() && !trash_contents(&folder, trash) {
            continue;
        }
        if let Err(e) = std::fs::remove_dir_all(&folder) {
            log::debug!(target: "cmdr_lib::file_viewer", "couldn't remove open-with copy {}: {e}", LogPrivate::path(&folder));
        }
    }
}

/// Whether `folder` was last touched before `started`. Its mtime moves when anything is
/// created, renamed, or removed in it, so a folder pulled or saved into since is this
/// session's. An unreadable mtime counts as this session's: kept is the safe answer.
pub(super) fn left_by_an_earlier_run(folder: &Path, started: SystemTime) -> bool {
    std::fs::metadata(folder)
        .and_then(|meta| meta.modified())
        .is_ok_and(|modified| modified < started)
}

/// Moves everything in `folder` but its record to the Trash. Answers whether the folder
/// is now safe to remove, which it isn't while anything the Trash refused is still in it.
fn trash_contents(folder: &Path, trash: &dyn Fn(&Path) -> Result<(), String>) -> bool {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return false;
    };
    let kept: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| entry.file_name() != RECORD_NAME)
        .map(|entry| entry.path())
        .collect();
    let mut all_trashed = true;
    for item in kept {
        match trash(&item) {
            Ok(()) => log::warn!(
                target: "cmdr_lib::file_viewer",
                "an app changed an open-with copy, so it went to the Trash instead of being removed: {}",
                LogPrivate::path(&item)
            ),
            Err(e) => {
                all_trashed = false;
                log::warn!(
                    target: "cmdr_lib::file_viewer",
                    "an app changed an open-with copy and the Trash refused it, so it stays for the next launch: {}: {e}",
                    LogPrivate::path(&item)
                );
            }
        }
    }
    all_trashed
}

/// How the in-session sweep picks what goes.
#[cfg(any(target_os = "macos", test))]
pub(super) struct SweepPolicy {
    /// This many of the newest untouched copies always stay, whatever their age or size.
    pub(super) keep_newest: usize,
    /// An untouched copy past the newest few goes once it's this old.
    pub(super) max_age: Duration,
    /// Past the newest few, the oldest go until the untouched copies fit in this.
    pub(super) budget_bytes: u64,
}

/// The in-session sweep, run before each launch's pulls: removes the untouched copies
/// `policy` lets go, as of `now`. Never touches a changed copy (an app may be editing
/// it; the next startup takes it to the Trash) or a folder with no record yet (a pull
/// still running beside this one).
#[cfg(any(target_os = "macos", test))]
pub(super) fn sweep_launch_folders(
    dir: &Path,
    is_launch_folder: impl Fn(&str) -> bool,
    now: SystemTime,
    policy: &SweepPolicy,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut untouched: Vec<(PathBuf, PullRecord)> = entries
        .flatten()
        .filter(|entry| is_launch_folder(&entry.file_name().to_string_lossy()))
        .filter_map(|entry| {
            let folder = entry.path();
            untouched_record(&folder).map(|record| (folder, record))
        })
        .collect();
    untouched.sort_by_key(|(_, record)| std::cmp::Reverse(record.pulled_at.to_system_time()));
    let mut kept_bytes: u64 = 0;
    for (index, (folder, record)) in untouched.into_iter().enumerate() {
        let age = now
            .duration_since(record.pulled_at.to_system_time())
            .unwrap_or_default();
        let over_budget = kept_bytes.saturating_add(record.size) > policy.budget_bytes;
        if index < policy.keep_newest || (age < policy.max_age && !over_budget) {
            kept_bytes = kept_bytes.saturating_add(record.size);
            continue;
        }
        // The sort above took a while on a big dir; an app may have saved since.
        if untouched_record(&folder).is_none() {
            continue;
        }
        if let Err(e) = std::fs::remove_dir_all(&folder) {
            log::debug!(target: "cmdr_lib::file_viewer", "couldn't sweep open-with copy {}: {e}", LogPrivate::path(&folder));
        }
    }
}
