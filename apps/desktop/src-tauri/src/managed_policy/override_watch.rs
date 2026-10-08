//! Debug and E2E builds only: re-read the policy when the `CMDR_MANAGED_PREFS_FILE` plist changes,
//! so a dev run or an E2E spec can push and remove a policy while the app runs, the way an MDM does.
//! It watches the file's FOLDER (the file may not exist yet, and an atomic save replaces it), and
//! reacts only to events naming the file. Any platform: the override works on Linux E2E too.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::ignore_poison::IgnorePoison;

static WATCHER: OnceLock<Mutex<Option<notify::RecommendedWatcher>>> = OnceLock::new();

/// Starts the watch on `file`'s folder. A failure only costs the live refresh, so it warns.
pub(super) fn start(file: PathBuf) {
    let Some(dir) = file.parent().map(Path::to_path_buf) else {
        return;
    };
    let watched = file.clone();
    let watcher = notify::recommended_watcher(move |result: Result<notify::Event, notify::Error>| match result {
        Ok(event) if names_file(&event.paths, &watched) => {
            // The read is a small file, but keep it off the watcher's own thread all the same.
            tauri::async_runtime::spawn_blocking(|| {
                if super::refresh() {
                    log::debug!(target: "managed_policy", "Re-read the managed policy after the override file changed");
                }
            });
        }
        Ok(_) => {}
        Err(e) => log::warn!(target: "managed_policy", "Override-file watch error: {e}"),
    });
    let mut watcher = match watcher {
        Ok(watcher) => watcher,
        Err(e) => {
            log::warn!(target: "managed_policy", "Couldn't create the override-file watcher: {e}");
            return;
        }
    };
    if let Err(e) = notify::Watcher::watch(&mut watcher, &dir, notify::RecursiveMode::NonRecursive) {
        log::warn!(target: "managed_policy", "Couldn't watch {}: {e}", dir.display());
        return;
    }
    *WATCHER.get_or_init(|| Mutex::new(None)).lock_ignore_poison() = Some(watcher);
    log::debug!(target: "managed_policy", "Watching {} for policy changes", file.display());
}

/// Whether an event in the folder concerns the override file. Compared by file name: the folder is
/// watched non-recursively, and FSEvents may report it through a different spelling of the same
/// folder (`/tmp` vs `/private/tmp`).
fn names_file(paths: &[PathBuf], file: &Path) -> bool {
    let Some(name) = file.file_name() else {
        return false;
    };
    paths.iter().any(|path| path.file_name() == Some(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_event_naming_the_file_counts_even_through_another_spelling_of_its_folder() {
        let file = Path::new("/tmp/cmdr-e2e-data/managed-prefs.plist");
        assert!(names_file(
            &[PathBuf::from("/tmp/cmdr-e2e-data/managed-prefs.plist")],
            file
        ));
        assert!(names_file(
            &[
                PathBuf::from("/private/tmp/cmdr-e2e-data/settings.json"),
                PathBuf::from("/private/tmp/cmdr-e2e-data/managed-prefs.plist"),
            ],
            file
        ));
    }

    #[test]
    fn an_event_for_another_file_in_the_folder_doesnt() {
        let file = Path::new("/tmp/cmdr-e2e-data/managed-prefs.plist");
        assert!(!names_file(&[PathBuf::from("/tmp/cmdr-e2e-data/settings.json")], file));
        assert!(!names_file(
            &[PathBuf::from("/tmp/cmdr-e2e-data/managed-prefs.plist.tmp")],
            file
        ));
        assert!(!names_file(&[], file));
    }
}
