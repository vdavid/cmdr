//! Drop folders: one fresh folder per promised drop, under a per-instance root in the
//! app data dir, where the source app writes the files before the transfer copies
//! them to their real destination.
//!
//! Lifecycle, owned here so no dialog route can leak one:
//!
//! - **Created** by `macos.rs` when a promise or image drop lands.
//! - **Claimed** by the copy that reads from it (`commands/file_system/volume_copy.rs`
//!   calls [`claim_for_operation`] with the started operation's id).
//! - **Removed** when that copy completes or is cancelled ([`release_for_operation`],
//!   from `operations_host::terminal_effects`), or when the frontend lets an unclaimed
//!   drop go ([`release_drop`]: the dialog was cancelled, or the drop hit no target).
//!   A FAILED copy keeps its folder, so the error dialog's Retry still finds the files.
//! - **Swept** at startup ([`init_drop_folder_root`]): whatever a crash or a failed
//!   copy left behind. Per instance, so a dev and a prod Cmdr never sweep each other.

use std::collections::{HashMap, VecDeque};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use crate::ignore_poison::IgnorePoison;

/// The registry of live drop folders under one root.
#[derive(Debug)]
pub(crate) struct DropFolders {
    root: PathBuf,
    /// Operation id → the drop folder it reads from.
    claims: HashMap<String, String>,
    /// The last few operations that ended before anything claimed for them. The claim
    /// lands after `start_volume_copy` returns, and a one-image copy can finish first.
    ended_unclaimed: VecDeque<String>,
}

/// How many unclaimed endings to remember: far more than can race one claim.
const ENDED_MEMORY: usize = 64;

/// Makes each drop id unique within the process; the time makes it unique across runs.
static NEXT_DROP: AtomicU64 = AtomicU64::new(0);

const DROP_ID_PREFIX: &str = "drop-";

impl DropFolders {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self {
            root,
            claims: HashMap::new(),
            ended_unclaimed: VecDeque::new(),
        }
    }

    /// Makes a fresh, empty drop folder. Returns its id and path.
    pub(crate) fn create(&mut self) -> io::Result<(String, PathBuf)> {
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or_default();
        let n = NEXT_DROP.fetch_add(1, Ordering::Relaxed);
        let id = format!("{DROP_ID_PREFIX}{millis}-{n}");
        let dir = self.root.join(&id);
        std::fs::create_dir_all(&dir)?;
        Ok((id, dir))
    }

    /// Removes the drop folder `drop_id` unless a copy has claimed it. A malformed id
    /// (anything that could name a path outside the root) is ignored. Returns whether
    /// it removed something.
    pub(crate) fn release(&mut self, drop_id: &str) -> bool {
        if !is_drop_id(drop_id) || self.claims.values().any(|d| d == drop_id) {
            return false;
        }
        remove_dir(&self.root.join(drop_id))
    }

    /// Records that operation `operation_id` reads from a drop folder, when any of
    /// `source_paths` lies inside one. Returns whether it claimed one.
    pub(crate) fn claim(&mut self, operation_id: &str, source_paths: &[PathBuf]) -> bool {
        let Some(drop_id) = source_paths.iter().find_map(|p| self.drop_id_of(p)) else {
            return false;
        };
        self.claims.insert(operation_id.to_string(), drop_id);
        if let Some(i) = self.ended_unclaimed.iter().position(|id| id == operation_id) {
            // It already ended: release it now, the way its ending would have.
            self.ended_unclaimed.remove(i);
            self.release_for_operation(operation_id);
        }
        true
    }

    /// The operation finished: drops its claim, and removes the folder once no other
    /// operation reads from it.
    pub(crate) fn release_for_operation(&mut self, operation_id: &str) {
        let Some(drop_id) = self.claims.remove(operation_id) else {
            if self.ended_unclaimed.len() == ENDED_MEMORY {
                self.ended_unclaimed.pop_front();
            }
            self.ended_unclaimed.push_back(operation_id.to_string());
            return;
        };
        if !self.claims.values().any(|d| *d == drop_id) {
            // allowed-discarded-outcome: already gone is as good as removed; a failure is logged inside.
            remove_dir(&self.root.join(&drop_id));
        }
    }

    /// Removes every drop folder under the root. Startup only: nothing reads them yet.
    pub(crate) fn sweep(&self) {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return;
        };
        for entry in entries.flatten() {
            if entry.file_name().to_str().is_some_and(is_drop_id) {
                // allowed-discarded-outcome: a startup sweep has no one to tell; a failure is logged inside.
                remove_dir(&entry.path());
            }
        }
    }

    /// The id of the drop folder `path` lies in (or is), if any.
    fn drop_id_of(&self, path: &Path) -> Option<String> {
        let first = path.strip_prefix(&self.root).ok()?.components().next()?;
        let id = first.as_os_str().to_str()?;
        is_drop_id(id).then(|| id.to_string())
    }
}

/// A drop id is `drop-<digits>-<digits>`: never a separator, never `..`.
fn is_drop_id(id: &str) -> bool {
    let Some(rest) = id.strip_prefix(DROP_ID_PREFIX) else {
        return false;
    };
    let mut parts = rest.split('-');
    let numeric = |p: Option<&str>| p.is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    numeric(parts.next()) && numeric(parts.next()) && parts.next().is_none()
}

fn remove_dir(dir: &Path) -> bool {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => true,
        Err(e) if e.kind() == io::ErrorKind::NotFound => false,
        Err(e) => {
            log::warn!(target: "drop_in", "couldn't remove a drop folder: {:?}", e.kind());
            false
        }
    }
}

// --- The process-wide registry ---

static FOLDERS: LazyLock<Mutex<DropFolders>> =
    LazyLock::new(|| Mutex::new(DropFolders::new(std::env::temp_dir().join("cmdr-drops"))));

/// Points drop folders at `root` (per instance, under the app data dir) and sweeps
/// what an earlier run left there, on a background thread.
pub fn init_drop_folder_root(root: PathBuf) {
    let mut folders = FOLDERS.lock_ignore_poison();
    *folders = DropFolders::new(root.clone());
    drop(folders);
    std::thread::Builder::new()
        .name("drop-folder-sweep".into())
        .spawn(move || DropFolders::new(root).sweep())
        .map(|_| ())
        .unwrap_or_else(|e| log::warn!(target: "drop_in", "couldn't start the drop-folder sweep: {e}"));
}

/// Makes a fresh drop folder in the process-wide registry.
#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "only the macOS drop receiver creates drop folders")
)]
pub(crate) fn create_drop_folder() -> io::Result<(String, PathBuf)> {
    FOLDERS.lock_ignore_poison().create()
}

/// [`DropFolders::release`] on the process-wide registry.
pub(crate) fn release_drop(drop_id: &str) -> bool {
    FOLDERS.lock_ignore_poison().release(drop_id)
}

/// [`DropFolders::claim`] on the process-wide registry.
pub(crate) fn claim_for_operation(operation_id: &str, source_paths: &[PathBuf]) -> bool {
    FOLDERS.lock_ignore_poison().claim(operation_id, source_paths)
}

/// [`DropFolders::release_for_operation`] on the process-wide registry.
pub(crate) fn release_for_operation(operation_id: &str) {
    FOLDERS.lock_ignore_poison().release_for_operation(operation_id);
}

#[cfg(test)]
#[path = "folders_tests.rs"]
mod tests;
