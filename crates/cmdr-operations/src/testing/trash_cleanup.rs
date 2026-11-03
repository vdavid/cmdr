//! Takes back what a test put in the user's real Trash.
//!
//! The trash tests drive the real `trashItemAtURL`, so each success lands in
//! `~/.Trash`, and a test that leaves it there fills the developer's Trash with a
//! fresh batch every run (about 400 fixtures in three days, 2026-10-23). Hold a
//! [`TrashCleanup`] over every source a test may trash: on drop, even a panicking
//! one, it removes what landed.
//!
//! It finds an item by the node it was: a trip into the Trash keeps the node
//! (`journal_capture_tests.rs` pins this), while the name can change (macOS adds a
//! timestamp when the name is taken). So it records each source's identity up
//! front and, on drop, removes only the top-level Trash entry with that identity
//! and the source's name as a prefix. It never touches anything outside the Trash.
//!
//! Where an item landed comes first from `note_trashed`, which a test build's
//! `move_to_trash_sync` calls with `trashItemAtURL`'s resulting URL. Listing the
//! Trash is only the fallback: TCC refuses `readdir` of `~/.Trash` to a process
//! without Full Disk Access (a test run started from an agent or an IDE), while a
//! `stat` and an unlink by path still work, so a guard that only listed left
//! everything behind there (verified on macOS 27, `ls ~/.Trash` answers
//! "Operation not permitted" while `stat` of a fixture in it answers, 2026-11-03).

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Every in-Trash path this process's trashes reported, until a guard claims it.
static TRASHED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Records where a trash landed, so a guard finds it without listing the Trash.
#[cfg(target_os = "macos")]
pub(crate) fn note_trashed(in_trash: &Path) {
    TRASHED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(in_trash.to_path_buf());
}

/// One source the test may trash: where its Trash is, and how to know it there.
struct Tracked {
    trash_dir: PathBuf,
    name: String,
    dev: u64,
    ino: u64,
}

/// Removes, on drop, what the tracked sources became in the Trash.
#[must_use = "the cleanup runs when the guard drops; bind it to a named variable"]
pub struct TrashCleanup {
    tracked: Vec<Tracked>,
}

impl TrashCleanup {
    /// Tracks each of `sources` that exists now. Call it BEFORE the trash, while the
    /// sources still sit where the test made them. A source that doesn't exist (a
    /// refusal a test sets up) is skipped: there's nothing of it to take back.
    pub fn of<P: AsRef<Path>>(sources: &[P]) -> Self {
        let tracked = sources.iter().filter_map(|source| track(source.as_ref())).collect();
        Self { tracked }
    }
}

fn track(source: &Path) -> Option<Tracked> {
    let meta = std::fs::symlink_metadata(source).ok()?;
    Some(Tracked {
        trash_dir: crate::trash_dir_for_path(source)?,
        name: source.file_name()?.to_string_lossy().into_owned(),
        dev: meta.dev(),
        ino: meta.ino(),
    })
}

impl Drop for TrashCleanup {
    fn drop(&mut self) {
        for item in &self.tracked {
            if let Some(path) = claim_noted(item).or_else(|| find_by_listing(item)) {
                remove(&path);
            }
        }
    }
}

impl Tracked {
    /// Whether `path` is this item in its Trash: a top-level entry there, named
    /// after the source, and the node the source was (`symlink_metadata`, so a
    /// trashed symlink is judged as the link itself).
    fn is(&self, path: &Path) -> bool {
        path.parent() == Some(self.trash_dir.as_path())
            && path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(&self.name))
            && std::fs::symlink_metadata(path).is_ok_and(|meta| meta.dev() == self.dev && meta.ino() == self.ino)
    }
}

/// Takes `item`'s noted landing out of [`TRASHED`], if a trash noted one.
fn claim_noted(item: &Tracked) -> Option<PathBuf> {
    let mut noted = TRASHED.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let at = noted.iter().position(|path| item.is(path))?;
    Some(noted.swap_remove(at))
}

/// Finds `item` by listing its Trash, for a trash that noted nothing.
fn find_by_listing(item: &Tracked) -> Option<PathBuf> {
    let entries = std::fs::read_dir(&item.trash_dir).ok()?;
    entries.flatten().map(|entry| entry.path()).find(|path| item.is(path))
}

fn remove(path: &Path) {
    let is_dir = std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_dir());
    let _ = if is_dir {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use cmdr_fs::testing::TestDir;

    #[test]
    fn a_dropped_guard_takes_back_what_landed_in_the_trash() {
        let tmp = TestDir::new("trash_cleanup_takes_back");
        let file = tmp.join("cleanup-probe.txt");
        let dir = tmp.join("cleanup-probe-dir");
        std::fs::write(&file, "content").unwrap();
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("inner.txt"), "data").unwrap();

        let guard = TrashCleanup::of(&[&file, &dir]);
        let file_in_trash = crate::move_to_trash_sync(&file).unwrap().expect("an in-Trash path");
        let dir_in_trash = crate::move_to_trash_sync(&dir).unwrap().expect("an in-Trash path");
        assert!(
            file_in_trash.exists() && dir_in_trash.exists(),
            "both landed in the Trash"
        );

        drop(guard);
        assert!(!file_in_trash.exists(), "the file is gone from the Trash");
        assert!(!dir_in_trash.exists(), "the folder is gone from the Trash");
    }

    #[test]
    fn a_guard_leaves_a_same_named_trash_item_that_isnt_its_own() {
        let tmp = TestDir::new("trash_cleanup_leaves_others");
        let first = tmp.join("a").join("cleanup-same-name.txt");
        let second = tmp.join("b").join("cleanup-same-name.txt");
        for path in [&first, &second] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "content").unwrap();
        }
        // The other test's item, under the same name, already in the Trash.
        let others = TrashCleanup::of(&[&first]);
        let others_in_trash = crate::move_to_trash_sync(&first).unwrap().expect("an in-Trash path");

        let mine = TrashCleanup::of(&[&second]);
        let mine_in_trash = crate::move_to_trash_sync(&second).unwrap().expect("an in-Trash path");
        drop(mine);

        assert!(!mine_in_trash.exists(), "its own item is gone");
        assert!(others_in_trash.exists(), "the same-named item that isn't its own stays");
        drop(others);
    }
}
