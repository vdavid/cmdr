//! "Open with" for a file only a ROUTE serves: an archive entry
//! (`/…/bundle.zip/docs/report.pdf`) or a blob in a repo's virtual `.git` trees. Neither
//! has a file of its own, so no app can open the path. Instead each launch pulls the
//! file into a FRESH temp and hands the app that copy.
//!
//! The sibling of the viewer's `materialize.rs`, whose pull it reuses, with its own
//! lifecycle:
//!
//! - **A fresh, read-only copy per launch.** A launched app has no close event, so no
//!   session or refcount could ever say when a copy is done with; and a copy shared
//!   across launches would cost archive-edit invalidation for nothing. The copy is
//!   `0o444`: the app opens a preview, and nothing it saves goes back into the archive.
//! - **Its own per-instance dir and prefix** (`<app_data_dir>/open-with-extract`,
//!   `.cmdr-open-with-*`). The viewer's reaper runs over a different dir and prefix, so
//!   neither can delete the other's live temps, and side-by-side dev / prod / worktree
//!   instances never share one.
//! - **Reaped at startup only.** The process boundary is what marks a temp abandoned:
//!   an app may hold a copy open for as long as this run lasts. ❌ No TTL reaper.
//! - **Listing apps against a stand-in.** LaunchServices answers no apps for a path
//!   with nothing at it (verified on macOS 27.0 with `URLsForApplicationsToOpenURL:`
//!   via `osascript`, 2026-10-01), and the menu is built before anything is pulled. So
//!   the app list is computed against an empty file with the same extension in this
//!   dir (`listing_path`), which LaunchServices types by extension exactly as it
//!   types the real entry. The UTI-based query that needs no file at all lives in
//!   `UniformTypeIdentifiers.framework`, which dyld refuses on the 10.15 floor.
//!
//! No Full Disk Access guard: the app data dir isn't TCC-protected.
//!
//! The "Open with" menu is macOS-only, so the pull and listing halves compile there
//! alone; the `_in` variants also compile under `test`, which exercises them on every
//! platform. The startup reaper and the event types build everywhere.

use std::path::{Path, PathBuf};
use std::sync::{LazyLock, RwLock};

use serde::{Deserialize, Serialize};

use super::materialize::reap_temps_with_prefix;
#[cfg(any(target_os = "macos", test))]
use super::materialize::{RoutedPullFailure, TempSpot, extract_routed_into};
use super::{ArchiveFailureKind, ViewerError};
#[cfg(any(target_os = "macos", test))]
use crate::file_system::volume::manager::{RoutedKind, get_volume_manager, path_routes_over_its_parent};
use crate::ignore_poison::RwLockIgnorePoison;

/// Prefix on each launch's subdir, and on the stand-in dir: what the startup reaper
/// matches. A different family from the viewer's `.cmdr-viewer-`.
const TEMP_SUBDIR_PREFIX: &str = ".cmdr-open-with-";

/// The stand-ins' dir. Carries [`TEMP_SUBDIR_PREFIX`], so a launch reaps it too and it
/// refills on the next right-click.
#[cfg(any(target_os = "macos", test))]
const STAND_IN_DIRNAME: &str = ".cmdr-open-with-types";

/// The stand-in's file stem. Only its extension matters to LaunchServices.
#[cfg(any(target_os = "macos", test))]
const STAND_IN_STEM: &str = "stand-in";

/// Fallback dir under the OS temp dir before [`init_open_with_extract_dir`] runs (unit
/// tests, a not-yet-initialized process).
#[cfg(target_os = "macos")]
const DEFAULT_DIRNAME: &str = "cmdr-open-with-extract";

/// Max bytes pulled for one launch. Larger than the viewer's 256 MiB preview cap: a
/// whole video or a big PDF is what people open in a real app. The declared size is
/// checked before a byte is written, so this is also the zip-bomb guard, and each copy
/// stays on disk until the next launch, which is what keeps it bounded.
#[cfg(target_os = "macos")]
pub(crate) const OPEN_WITH_CAP_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// The per-instance dir, set at startup by [`init_open_with_extract_dir`].
static OPEN_WITH_DIR: LazyLock<RwLock<Option<PathBuf>>> = LazyLock::new(|| RwLock::new(None));

/// Records the per-instance dir and reaps what an earlier run left in it. Called once
/// at startup from `lib.rs` with `<app_data_dir>/open-with-extract`.
pub fn init_open_with_extract_dir(dir: PathBuf) {
    reap_open_with_temps(&dir);
    *OPEN_WITH_DIR.write_ignore_poison() = Some(dir);
}

#[cfg(target_os = "macos")]
fn open_with_dir() -> PathBuf {
    OPEN_WITH_DIR
        .read_ignore_poison()
        .clone()
        .unwrap_or_else(|| std::env::temp_dir().join(DEFAULT_DIRNAME))
}

/// Removes every `.cmdr-open-with-*` entry in `dir`.
pub(super) fn reap_open_with_temps(dir: &Path) {
    reap_temps_with_prefix(dir, TEMP_SUBDIR_PREFIX);
}

/// Whether `name` is one of this family's subdirs (the reaper's match predicate).
#[cfg(test)]
pub(super) fn is_open_with_temp_name(name: &str) -> bool {
    name.starts_with(TEMP_SUBDIR_PREFIX)
}

/// Whether any of `paths` needs a pull before an app can open it. Pure string work, so
/// the menu handler can keep an ordinary launch on its synchronous path.
#[cfg(target_os = "macos")]
pub(crate) fn any_needs_extraction(paths: &[PathBuf]) -> bool {
    paths.iter().any(|path| path_routes_over_its_parent(path))
}

/// `open-with-copy-refused`: an "Open with" click on a file only a route serves
/// couldn't copy it out, so no app was launched. The main window says why in a toast;
/// without it, the click would do nothing at all.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct OpenWithCopyRefused {
    /// The file's own name, as the person sees it in the pane.
    pub file_name: String,
    /// The chosen app's display name (its bundle name without `.app`).
    pub app_name: String,
    pub reason: OpenWithCopyRefusal,
    /// Where the file sits, which the too-big toast names.
    pub source: OpenWithCopySource,
}

#[cfg(any(target_os = "macos", test))]
impl OpenWithCopyRefused {
    /// The notice for `refused`, a launch of the app bundle at `app_path`.
    pub(crate) fn new(refused: &RefusedCopy, app_path: &Path) -> Self {
        let lossy = |name: Option<&std::ffi::OsStr>, fallback: &Path| {
            name.map_or_else(|| fallback.display().to_string(), |n| n.to_string_lossy().into_owned())
        };
        Self {
            file_name: lossy(refused.path.file_name(), &refused.path),
            app_name: lossy(app_path.file_stem(), app_path),
            reason: refused.reason,
            source: refused.source,
        }
    }
}

/// What served the file the copy was pulled from. The archive refusals only ever come
/// from an archive; a repo snapshot's reads that break off are plain `Unreadable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum OpenWithCopySource {
    /// An entry inside a zip, tar, or 7z Cmdr browses like a folder.
    Archive,
    /// A blob in one of a repo's virtual `.git` history trees.
    RepoHistory,
}

#[cfg(any(target_os = "macos", test))]
impl From<RoutedKind> for OpenWithCopySource {
    fn from(routed: RoutedKind) -> Self {
        match routed {
            RoutedKind::Archive => Self::Archive,
            RoutedKind::GitPortal => Self::RepoHistory,
        }
    }
}

/// Why the copy couldn't be made, in the terms the toast words differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum OpenWithCopyRefusal {
    /// Over `OPEN_WITH_CAP_BYTES` (`cap`), refused before a byte was written.
    TooLarge { cap: u64 },
    /// The archive needs a password it hasn't been given. Copying the file out asks
    /// for it, after which "Open with" works too.
    NeedsPassword,
    /// The archive is damaged or uses something this build can't decode.
    ArchiveUnreadable,
    /// Anything else: the source went away or couldn't be read.
    Unreadable,
}

impl From<&ViewerError> for OpenWithCopyRefusal {
    fn from(error: &ViewerError) -> Self {
        match error {
            ViewerError::TooLargeToPreview { cap, .. } => Self::TooLarge { cap: *cap },
            ViewerError::Archive { failure, .. } => match failure {
                ArchiveFailureKind::NeedsPassword => Self::NeedsPassword,
                ArchiveFailureKind::Unsupported | ArchiveFailureKind::Unreadable => Self::ArchiveUnreadable,
            },
            ViewerError::Io { .. }
            | ViewerError::NotFound { .. }
            | ViewerError::IsDirectory
            | ViewerError::SessionNotFound { .. }
            | ViewerError::Cancelled
            | ViewerError::OutOfRange
            | ViewerError::TimedOut
            | ViewerError::StoppedResponding
            | ViewerError::DestinationIsReadOnly
            // An archive in S3 cold storage can't be opened to copy a row out of it.
            | ViewerError::ColdStorage => Self::Unreadable,
        }
    }
}

/// A launch that couldn't copy one of its rows out: which one, and why.
#[cfg(any(target_os = "macos", test))]
#[derive(Debug)]
pub(crate) struct RefusedCopy {
    pub(crate) path: PathBuf,
    pub(crate) reason: OpenWithCopyRefusal,
    pub(crate) source: OpenWithCopySource,
    /// The pull's own failure, for the log.
    pub(crate) error: ViewerError,
}

/// What to hand the app: each path as it is, or a fresh read-only copy of one only a
/// route serves. Blocking (it streams the file out): run it off the main thread.
#[cfg(target_os = "macos")]
pub(crate) fn launch_paths(paths: &[PathBuf]) -> Result<Vec<PathBuf>, RefusedCopy> {
    launch_paths_in(paths, &open_with_dir(), OPEN_WITH_CAP_BYTES)
}

/// [`launch_paths`] into an explicit dir under an explicit cap, for tests.
#[cfg(any(target_os = "macos", test))]
pub(super) fn launch_paths_in(paths: &[PathBuf], dir: &Path, cap: u64) -> Result<Vec<PathBuf>, RefusedCopy> {
    let spot = TempSpot {
        dir,
        prefix: TEMP_SUBDIR_PREFIX,
    };
    paths
        .iter()
        .map(|path| {
            if !path_routes_over_its_parent(path) {
                return Ok(path.clone());
            }
            // `None`: the route didn't confirm (a real folder named `foo.zip`), so the
            // path is a real file and opens as it is.
            let copy = extract_routed_into(path, &parent_volume_id(path), spot, cap).map_err(
                |RoutedPullFailure { routed, error }| RefusedCopy {
                    path: path.clone(),
                    reason: OpenWithCopyRefusal::from(&error),
                    source: OpenWithCopySource::from(routed),
                    error,
                },
            )?;
            let Some(copy) = copy else {
                return Ok(path.clone());
            };
            make_read_only(&copy.temp_file);
            Ok(copy.temp_file)
        })
        .collect()
}

/// The volume physically holding `path`: the longest registered mount under it, else
/// the default (boot) volume. The menu carries paths only, and a route rides on its
/// parent drive's volume (`VolumeManager::mount_id_for_path` skips routed volumes).
#[cfg(any(target_os = "macos", test))]
fn parent_volume_id(path: &Path) -> String {
    let manager = get_volume_manager();
    manager
        .mount_id_for_path(&path.to_string_lossy())
        .or_else(|| manager.default_volume_id())
        .unwrap_or_else(|| "root".to_string())
}

/// Best-effort: a copy that stays writable still opens, it just doesn't say "locked".
#[cfg(any(target_os = "macos", test))]
fn make_read_only(file: &Path) {
    let result = std::fs::metadata(file).and_then(|meta| {
        let mut permissions = meta.permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(file, permissions)
    });
    if let Err(e) = result {
        log::debug!(target: "cmdr_lib::file_viewer", "open-with copy stays writable, {}: {e}", file.display());
    }
}

/// The path to ask LaunchServices about for `path`'s "Open with" apps: `path` itself, or
/// for a path only a route serves, a real empty file with the same extension.
#[cfg(target_os = "macos")]
pub(crate) fn listing_path(path: &Path) -> PathBuf {
    listing_path_in(path, &open_with_dir())
}

/// `listing_path` in an explicit dir, for tests.
#[cfg(any(target_os = "macos", test))]
pub(super) fn listing_path_in(path: &Path, dir: &Path) -> PathBuf {
    if !path_routes_over_its_parent(path) {
        return path.to_path_buf();
    }
    let stand_ins = dir.join(STAND_IN_DIRNAME);
    let stand_in = match path.extension() {
        Some(extension) => stand_ins.join(STAND_IN_STEM).with_extension(extension),
        None => stand_ins.join(STAND_IN_STEM),
    };
    if stand_in.is_file() {
        return stand_in;
    }
    match std::fs::create_dir_all(&stand_ins).and_then(|()| std::fs::File::create(&stand_in)) {
        Ok(_) => stand_in,
        Err(e) => {
            // Asking about the path itself answers no apps, which is what the menu said
            // before this existed; "Other…" still works.
            log::debug!(target: "cmdr_lib::file_viewer", "no open-with stand-in at {}: {e}", stand_in.display());
            path.to_path_buf()
        }
    }
}
