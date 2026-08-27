//! Where the user has been working lately, as walk-order candidates.
//!
//! [`crate::spotlight`] answers "which folders hold this user's recently-opened
//! files". This decides when to ask, what to ask for, and which answers are worth
//! walking early. The split is deliberate: the query knows nothing about indexing, and
//! this knows nothing about Spotlight's API.
//!
//! **Why it exists.** On a true first run there are no tabs and no favorites, so the
//! ranking falls back to [`super::STANDARD_HOME_FOLDERS`], a static list that is the
//! same for everybody. This is what makes that first run personal: the folder somebody
//! actually lives in gets walked before `~/Movies`, and it gets walked before the
//! index knows anything, which is the one moment nothing else can answer.
//!
//! **The ask is once per process, off-thread, and late is fine.** `priority_roots` is
//! contractually cheap (`HostPolicy::priority_roots`: no I/O on a contended path, no
//! blocking lock) and a synchronous Spotlight query is neither. So the first ask that
//! finds the coast clear ARMS a sampler thread and answers without it; every later ask
//! picks up the result. The phase machine asks at each phase boundary, so the sample
//! joins the ranking within a phase or two of index start. Arriving late costs
//! nothing: `priority_roots` promises order and never scope.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use crate::ignore_poison::IgnorePoison;

use super::{CLOUD_STORAGE_DIR, ICLOUD_DRIVE_DIR, LIBRARY_DIR};

/// How far back "recently" reaches.
///
/// A fresh install often follows a new machine or a migration, where the last week of
/// activity is unpacking rather than working, so a tight window would rank the wrong
/// folders on exactly the run this exists to serve. A month is long enough to see past
/// that and short enough that last spring's project doesn't outrank this week's.
const WINDOW: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// The most Spotlight results to read. Bounds cost, ❌ not the answer: the query itself
/// is unbounded, and when this bites it keeps the most recent files
/// (`spotlight::folders_with_recent_files`). A month of activity on a working machine
/// is thousands of files, and each one read is a framework round-trip.
const MAX_FILES: usize = 2_000;

/// The most folders this signal may contribute.
///
/// ⚠️ A cap is not optional. [`super::MAX_ROOTS`] is 24 for the whole ranking, and
/// recency is ranked ABOVE the standard home folders, so an uncapped tail of
/// barely-used folders would push `~/Downloads` and `~/Documents` off the end
/// entirely. Eight leaves room for every other signal.
const MAX_RECENT_ROOTS: usize = 8;

/// How many recently-opened files a folder needs before it is worth walking early.
///
/// One file is somebody opening an attachment once. Two is a place they went back to.
/// ⚠️ A guess, and the first thing to revisit if this ever ranks badly on a real home.
const MIN_FILES: usize = 2;

/// The folders this user has been working in, busiest first, or empty until the sample
/// lands.
///
/// Cheap on every call: a lock and a clone of at most [`MAX_RECENT_ROOTS`] paths. The
/// first call that finds Full Disk Access settled starts the sampler and still returns
/// empty; ❌ it never waits for it.
pub(super) fn folders(home: &Path, fda_pending: bool) -> Vec<PathBuf> {
    if let Some(sampled) = sample().lock_ignore_poison().clone() {
        return sampled;
    }
    // ⚠️ Don't ask while the Full Disk Access decision is open. The query reaches into
    // TCC-protected folders, and the onboarding modal is the one moment a stack of
    // system popups is unforgivable. A later ask arms it once the choice is made.
    if fda_pending {
        return Vec::new();
    }
    arm(home.to_path_buf());
    Vec::new()
}

/// The sampled answer: `None` until the sampler thread finishes, then the ranked paths.
fn sample() -> &'static Mutex<Option<Vec<PathBuf>>> {
    static SAMPLE: OnceLock<Mutex<Option<Vec<PathBuf>>>> = OnceLock::new();
    SAMPLE.get_or_init(|| Mutex::new(None))
}

/// Start the one sampler this process runs, unless it is already started.
///
/// A compare-and-set rather than a lock held across the spawn, so a burst of asks
/// (`priority_roots` is asked at every phase boundary, milliseconds apart) produces one
/// thread and not a stampede.
fn arm(home: PathBuf) {
    static ARMED: AtomicBool = AtomicBool::new(false);
    if ARMED.swap(true, Ordering::Relaxed) {
        return;
    }
    // A dedicated OS thread with room for the synchronous framework round-trips,
    // matching `importance/last_used.rs`'s rule: ❌ never rayon, whose 2 MB worker
    // stack can't absorb them. Detached, because nothing waits for the answer.
    let spawned = std::thread::Builder::new()
        .name("priority-recency-sample".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            let folders = crate::spotlight::folders_with_recent_files(&home, WINDOW, MAX_FILES);
            let ranked = worth_walking_early(&home, folders);
            log::info!(
                target: "priority",
                "Spotlight recency ranked {} folder(s) for the first-run walk order",
                ranked.len()
            );
            *sample().lock_ignore_poison() = Some(ranked);
        });
    if let Err(e) = spawned {
        log::warn!(target: "priority", "Spotlight recency sampler didn't start, so the walk order stays static: {e}");
        // Leave `ARMED` set: a thread we couldn't spawn once is not worth retrying at
        // every phase boundary for the rest of the session.
    }
}

/// Filter and cap the sample down to folders a first walk should actually visit early.
///
/// ⚠️ `~/Library` is dropped here, not by the ranking. [`super::WalkOrder::consider`]
/// rejects `~/Library` itself but nothing BELOW it, and a month of recency under a home
/// directory is dominated by application support files: uncapped, they would take every
/// slot this signal has. Its cloud children are the exception and stay, because a file
/// opened in Dropbox or iCloud Drive is the user's own work.
fn worth_walking_early(home: &Path, folders: Vec<crate::spotlight::RecentFolder>) -> Vec<PathBuf> {
    let library = home.join(LIBRARY_DIR);
    let cloud = [home.join(CLOUD_STORAGE_DIR), home.join(ICLOUD_DRIVE_DIR)];
    folders
        .into_iter()
        .filter(|folder| folder.files >= MIN_FILES)
        .map(|folder| folder.path)
        .filter(|path| !path.starts_with(&library) || cloud.iter().any(|dir| path.starts_with(dir)))
        .take(MAX_RECENT_ROOTS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spotlight::RecentFolder;

    fn folder(path: &str, files: usize) -> RecentFolder {
        RecentFolder {
            path: PathBuf::from(path),
            files,
        }
    }

    #[test]
    fn a_folder_visited_once_isnt_worth_a_walk_root() {
        let kept = worth_walking_early(
            Path::new("/Users/x"),
            vec![folder("/Users/x/work", 2), folder("/Users/x/stray", 1)],
        );
        assert_eq!(kept, vec![PathBuf::from("/Users/x/work")]);
    }

    #[test]
    fn library_support_folders_are_dropped_but_cloud_ones_stay() {
        // The ranking only rejects `~/Library` itself, so without this filter a month
        // of application-support churn would take every slot the signal has.
        let kept = worth_walking_early(
            Path::new("/Users/x"),
            vec![
                folder("/Users/x/Library/Application Support/SomeApp", 90),
                folder("/Users/x/Library/CloudStorage/Dropbox/notes", 30),
                folder("/Users/x/Library/Mobile Documents/com~apple~CloudDocs/papers", 20),
                folder("/Users/x/code", 10),
            ],
        );
        assert_eq!(
            kept,
            vec![
                PathBuf::from("/Users/x/Library/CloudStorage/Dropbox/notes"),
                PathBuf::from("/Users/x/Library/Mobile Documents/com~apple~CloudDocs/papers"),
                PathBuf::from("/Users/x/code"),
            ]
        );
    }

    #[test]
    fn the_signal_cant_crowd_out_every_other_one() {
        // `MAX_ROOTS` is 24 for the whole ranking and recency outranks the standard
        // home folders, so an uncapped tail would push `~/Downloads` off the end.
        let many: Vec<RecentFolder> = (0..40).map(|i| folder(&format!("/Users/x/p{i:02}"), 40 - i)).collect();
        assert_eq!(worth_walking_early(Path::new("/Users/x"), many).len(), MAX_RECENT_ROOTS);
    }

    #[test]
    fn nothing_is_sampled_while_the_full_disk_access_choice_is_open() {
        // Arming here would fire a stack of TCC popups over the onboarding modal.
        assert!(folders(Path::new("/Users/x"), true).is_empty());
        assert!(
            sample().lock_ignore_poison().is_none(),
            "the sampler must not have been armed"
        );
    }
}
