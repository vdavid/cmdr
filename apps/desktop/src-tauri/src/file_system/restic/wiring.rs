//! The parked hub every restic view shares, and the confirmed "is this inside
//! a restic view?" question for the seams that bypass `VolumeManager::resolve`.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use cmdr_fs::entry::FileEntry;
use cmdr_fs::restic_view::restic_route_candidates;
use cmdr_restic::{ResticRepos, is_restic_repo};

use crate::file_system::volume::DEFAULT_VOLUME_ID;

static REPOS: OnceLock<Arc<ResticRepos>> = OnceLock::new();

/// The app's one restic hub: passwords, opened repos, loaded indexes, and which
/// folders are repos.
///
/// ❗ Parked once, ❌ never rebuilt: a second hub would forget every password and
/// load every index twice. A `ResticVolume` carries the hub it was built over,
/// so the routing LRU dropping a volume loses nothing. Built on first use, which
/// is a listing, so the app's volume host is installed by then.
pub fn repos() -> &'static Arc<ResticRepos> {
    REPOS.get_or_init(|| Arc::new(ResticRepos::new(crate::volume_host::host())))
}

/// The most entries a repo's own folder listing can hold for this to read it.
/// A repo root holds six (`config`, `data`, `index`, `keys`, `snapshots`,
/// `locks`), plus whatever Finder or a sync tool leaves beside them; a folder
/// past this isn't a repo, and reading on would cost a big folder's whole
/// listing.
pub(super) const MAX_REPO_ROOT_ENTRIES: usize = 32;

/// Whether a LOCAL `path` sits at or inside a restic repo's `Browse snapshots`
/// view, CONFIRMED: the guard for the local write fast path and the agent's
/// rename plan, which take paths with no volume id and bypass `resolve`.
///
/// Lexical candidates first (zero I/O for every ordinary path), then the hub's
/// memo for the boot volume, then a bounded `std::fs` read of the candidate
/// folder. ❌ Not the permissive lexical predicate: a real folder named
/// `Browse snapshots` outside a repo takes ordinary writes, and refusing them
/// would be a bug the other way. An unconfirmed miss that wrote, though, would
/// put a real `Browse snapshots` folder inside someone's repo, which is why this
/// does the I/O the routing hot path doesn't.
pub fn local_path_is_in_a_restic_view(path: &Path) -> bool {
    restic_route_candidates(path).any(
        |repo_root| match repos().serves_confirmed(DEFAULT_VOLUME_ID, &repo_root) {
            Some(known) => known,
            None => local_folder_is_a_repo(&repo_root),
        },
    )
}

/// Whether `path` on `volume_id` sits at or inside the view of a repo the hub
/// has CONFIRMED, from its memo alone: zero I/O, for the agent's rename plan,
/// which names a row's own problem before it stats anything. A repo a pane
/// opened is covered (its route confirmed it); a plan may also name a view the
/// hub never met, which answers `false`. That stays safe: the plan's stat goes
/// through the plain volume, finds nothing there (a view isn't on disk), and
/// words a plan where nothing exists by the view
/// (`tools/propose/rename/rows.rs`, `MissingUnderSnapshots`); apply's
/// `is_a_view_row` refuses such a row regardless.
pub fn memo_says_in_a_restic_view(volume_id: &str, path: &Path) -> bool {
    restic_route_candidates(path).any(|repo_root| repos().serves_confirmed(volume_id, &repo_root) == Some(true))
}

/// Reads `folder` with `std::fs` and asks `is_restic_repo`, giving up past
/// [`MAX_REPO_ROOT_ENTRIES`]. Any failure to read is "not a repo".
fn local_folder_is_a_repo(folder: &Path) -> bool {
    let Ok(read) = std::fs::read_dir(folder) else {
        return false;
    };
    let mut entries = Vec::new();
    for entry in read {
        let Ok(entry) = entry else {
            return false;
        };
        if entries.len() == MAX_REPO_ROOT_ENTRIES {
            return false;
        }
        let Ok(kind) = entry.file_type() else {
            return false;
        };
        entries.push(FileEntry::new(
            entry.file_name().to_string_lossy().into_owned(),
            entry.path().to_string_lossy().into_owned(),
            kind.is_dir(),
            kind.is_symlink(),
        ));
    }
    is_restic_repo(&entries)
}
