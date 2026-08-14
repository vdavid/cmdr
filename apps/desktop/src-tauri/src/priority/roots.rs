//! Which folders matter to this user, ordered best guess first.
//!
//! The index walks a volume in phases, and this is the schedule: the order here
//! is what the user gets first, so `~/Downloads` answers a search while the rest
//! of the drive is still being walked. Answers the index's
//! `HostPolicy::priority_roots` seam through
//! [`AppHostPolicy`](crate::priority::host_policy::AppHostPolicy).
//!
//! **Order is the only payload.** Nothing here is a scope or a promise: a root
//! that drops off the list changes what gets indexed FIRST, never what gets
//! indexed. So a guess that ages badly costs a few minutes of walk order and
//! nothing else.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use cmdr_index::ROOT_VOLUME_ID;

use crate::ignore_poison::IgnorePoison;
use crate::restricted_paths::tcc_paths;

/// Bundle id from `tauri.conf.json`. Mirrored here so the data dir resolves without an
/// `AppHandle` (same reason as `favorites/store.rs` and `install_id.rs`). Keep in sync if
/// it ever changes.
const BUNDLE_ID: &str = "com.veszelovszki.cmdr";

/// Filename inside `{app_data_dir}/`, written by the frontend's pane persistence.
const APP_STATUS_FILE_NAME: &str = "app-status.json";

/// How many roots the index walks before it moves on to the rest of the volume.
///
/// A cap, not a budget: someone with 200 favorites would otherwise turn the first
/// phase into a whole-drive walk with extra bookkeeping, which is the one shape
/// phased indexing exists to avoid.
const MAX_ROOTS: usize = 24;

/// How long a computed answer is reused. The seam is asked at phase boundaries,
/// which can be milliseconds apart, and computing an answer stats a couple of dozen
/// paths. Short enough that an edited favorites list or a new tab lands within a few
/// phases, long enough that the walk never pays for the question.
const CACHE_TTL: Duration = Duration::from_secs(10);

/// The home folders worth walking before the rest of home, in order. Each one is
/// taken only when it exists AND has something in it: an untouched `~/Movies` would
/// otherwise sit ahead of a folder the user actually keeps things in.
const STANDARD_HOME_FOLDERS: &[&str] = &["Downloads", "Documents", "Desktop", "Pictures", "Movies", "Music"];

/// Where macOS mounts every File Provider domain (Dropbox, Google Drive, OneDrive, ...),
/// one directory per domain.
const CLOUD_STORAGE_DIR: &str = "Library/CloudStorage";

/// iCloud Drive, which is its own File Provider domain and has a fixed path.
const ICLOUD_DRIVE_DIR: &str = "Library/Mobile Documents/com~apple~CloudDocs";

/// The legacy Dropbox location, still where a lot of installs keep their files.
const DROPBOX_DIR: &str = "Dropbox";

/// Where the OS mounts everything that isn't the boot volume. Nothing at or below one of
/// these is the boot volume's to schedule, and the check is a pure prefix test on
/// purpose: the volume registry only knows the mounts Cmdr has registered, and a stat on
/// an unregistered wedged share would block this thread for minutes.
#[cfg(target_os = "macos")]
const MOUNT_POINT_PREFIXES: &[&str] = &["/Volumes", "/Network"];
#[cfg(not(target_os = "macos"))]
const MOUNT_POINT_PREFIXES: &[&str] = &["/media", "/mnt", "/run/media", "/net"];

/// `~/Library` is in scope for the index and never a priority root: it is the biggest
/// and churniest subtree in home (caches, mail, containers), so walking it early would
/// spend the phase that is supposed to make the user's own files searchable. Its cloud
/// children are separate candidates and stay.
const LIBRARY_DIR: &str = "Library";

/// The folders on `volume_id` this user cares about, best guess first, ready to walk.
///
/// Deduplicated, with nothing below an earlier root, existence-checked, and capped.
/// Only the boot volume gets an answer: every signal behind the ranking (tabs,
/// favorites, home) describes where the user keeps files on their own machine, and a
/// share must not inherit it. A share's own order is a question for whoever needs it.
pub fn priority_roots(volume_id: &str) -> Vec<PathBuf> {
    if volume_id != ROOT_VOLUME_ID {
        return Vec::new();
    }
    cached_roots()
}

/// The ranked answer, recomputed at most once per [`CACHE_TTL`].
///
/// The seam is asked at phase boundaries, which can be milliseconds apart, and an
/// answer costs a couple of dozen stats plus a small file read. The lock is held
/// across the computation so a burst of asks produces one answer rather than a
/// stampede of identical walks of the same paths; nothing else contends for it.
fn cached_roots() -> Vec<PathBuf> {
    /// The last answer and the moment it was computed; `None` until the first ask.
    type LastAnswer = Option<(Instant, Vec<PathBuf>)>;

    static CACHE: OnceLock<Mutex<LastAnswer>> = OnceLock::new();

    let mut cached = CACHE.get_or_init(|| Mutex::new(None)).lock_ignore_poison();
    if let Some((computed_at, roots)) = cached.as_ref()
        && computed_at.elapsed() < CACHE_TTL
    {
        return roots.clone();
    }

    let roots = collect_inputs()
        .map(|inputs| rank_roots(&inputs, &is_on_another_volume))
        .unwrap_or_default();
    *cached = Some((Instant::now(), roots.clone()));
    roots
}

/// Reads the signals the ranking needs. `None` when there is no home directory to
/// reason about, which leaves the volume-root phase to index everything anyway.
///
/// Note `favorites::store::list()` seeds the defaults when the file is absent, exactly
/// as the volume switcher's read does. Asking early only writes the same file the
/// user's first look at the switcher would have.
fn collect_inputs() -> Option<RootInputs> {
    let home = dirs::home_dir()?;
    Some(RootInputs {
        tabs: last_session_tab_paths(&home),
        favorites: crate::favorites::store::list()
            .into_iter()
            .map(|favorite| PathBuf::from(favorite.path))
            .collect(),
        fda_pending: crate::fda_gate::is_fda_pending_runtime(),
        home,
    })
}

/// Whether some mount other than the boot volume covers `path`. An in-memory registry
/// lookup, ❌ never a `statfs`: a probe of a wedged share can block for a minute or two,
/// and this runs on the index's own thread.
fn is_on_another_volume(path: &Path) -> bool {
    crate::file_system::volume::manager::get_volume_manager()
        .mount_id_for_path(&path.to_string_lossy())
        .is_some()
}

/// Where the panes were pointing last time, from the frontend's `app-status.json`.
/// Empty on a true first run.
fn last_session_tab_paths(home: &Path) -> Vec<PathBuf> {
    let Some(path) = app_status_path() else {
        return Vec::new();
    };
    let Ok(contents) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    parse_tab_paths(&contents, home)
}

/// The pane-state file, resolved without an `AppHandle` (the seam is a plain trait
/// method, so there is none to pass): `CMDR_DATA_DIR` when an isolated instance set it,
/// else the OS default for the bundle id. Mirrors `favorites/store.rs`.
fn app_status_path() -> Option<PathBuf> {
    let data_dir = match std::env::var("CMDR_DATA_DIR") {
        Ok(custom) if !custom.is_empty() => PathBuf::from(custom),
        _ => dirs::data_dir()?.join(BUNDLE_ID),
    };
    Some(data_dir.join(APP_STATUS_FILE_NAME))
}

/// What the ranking works from, so the ranking itself can be exercised over a
/// synthetic home instead of the machine's real one.
struct RootInputs {
    /// The user's home directory, and the base every home-relative candidate joins onto.
    home: PathBuf,
    /// Where the panes were pointing, most recently active first.
    tabs: Vec<PathBuf>,
    /// The user's favorites, in their own order.
    favorites: Vec<PathBuf>,
    /// While the Full Disk Access decision is pending, ❌ don't stat TCC-anchored paths:
    /// even `Path::exists()` raises a system popup on top of our onboarding modal.
    fda_pending: bool,
}

/// What a candidate has to be before it earns a place in the walk order.
#[derive(Clone, Copy)]
enum Requirement {
    /// A directory that is there. Enough for a folder the user named themselves.
    Directory,
    /// A directory with at least one entry in it. The bar for a folder we guessed at.
    NonEmptyDirectory,
}

// ---------------------------------------------------------------------------
// The ranking
// ---------------------------------------------------------------------------

/// The ranked, filtered walk order, best signal first:
///
/// 1. last session's tab paths, most recently active first (literally where the user was),
/// 2. the user's favorites, in their order,
/// 3. the standard home folders that exist and hold something,
/// 4. the cloud roots that exist (after the local ones: a File Provider read can stall,
///    and a stall must not delay `~/Downloads`),
/// 5. `$HOME` itself, which sweeps up everything the guesses missed.
///
/// `on_another_volume` decides whether a candidate belongs to some other mount; it is
/// injected so the ranking stays testable without a volume registry.
fn rank_roots(inputs: &RootInputs, on_another_volume: &dyn Fn(&Path) -> bool) -> Vec<PathBuf> {
    let mut order = WalkOrder {
        roots: Vec::new(),
        library: inputs.home.join(LIBRARY_DIR),
        fda_pending: inputs.fda_pending,
        on_another_volume,
    };

    for tab in &inputs.tabs {
        order.consider(tab, Requirement::Directory);
    }
    for favorite in &inputs.favorites {
        order.consider(favorite, Requirement::Directory);
    }
    for folder in STANDARD_HOME_FOLDERS {
        order.consider(&inputs.home.join(folder), Requirement::NonEmptyDirectory);
    }
    for cloud in cloud_roots(&inputs.home, inputs.fda_pending) {
        order.consider(&cloud, Requirement::Directory);
    }
    order.consider(&inputs.home, Requirement::Directory);

    order.roots
}

/// The order being built, plus everything a candidate is judged against.
struct WalkOrder<'a> {
    /// The roots accepted so far, in walk order.
    roots: Vec<PathBuf>,
    /// `~/Library`, which never becomes a root.
    library: PathBuf,
    /// See [`RootInputs::fda_pending`].
    fda_pending: bool,
    /// Whether a path belongs to some other mount.
    on_another_volume: &'a dyn Fn(&Path) -> bool,
}

impl WalkOrder<'_> {
    /// Takes `candidate` as the next root, unless it is already covered, isn't ours to
    /// walk, or doesn't clear `requirement`. Silent about rejections on purpose: every
    /// signal feeding this is a guess, and a dropped guess costs nothing.
    fn consider(&mut self, candidate: &Path, requirement: Requirement) {
        if self.roots.len() >= MAX_ROOTS {
            return;
        }
        let Some(path) = normalized(candidate) else {
            return;
        };
        if path == self.library {
            return;
        }
        // One test for two rules: a path that starts with an accepted root is either
        // that root again (dedupe) or sits inside it (already covered).
        if self.roots.iter().any(|root| path.starts_with(root)) {
            return;
        }
        // Both volume tests run before anything stats: one catches a registered mount
        // wherever it sits, the other catches an unregistered one at the usual mount
        // points, and neither may be the reason a stat lands on a wedged share.
        if is_under_a_mount_point(&path) || (self.on_another_volume)(&path) {
            return;
        }
        if !self.is_available(&path, requirement) {
            return;
        }
        self.roots.push(path);
    }

    /// Whether `path` clears `requirement` right now.
    ///
    /// ⚠️ While the Full Disk Access gate is pending, a TCC-anchored path is taken on
    /// trust with NO stat at all: `Path::exists()` alone raises a system popup on top
    /// of our onboarding modal, and several of these stack. The protected folders exist
    /// on essentially every account, and a walk of one that doesn't simply finds
    /// nothing. `volumes::get_favorites` follows the same rule.
    fn is_available(&self, path: &Path, requirement: Requirement) -> bool {
        if self.fda_pending && tcc_paths::is_potentially_tcc_restricted(path) {
            return true;
        }
        match requirement {
            Requirement::Directory => path.is_dir(),
            Requirement::NonEmptyDirectory => fs::read_dir(path).is_ok_and(|mut entries| entries.next().is_some()),
        }
    }
}

/// Whether `path` is a mount point or sits below one, by path shape alone. Component-wise,
/// so `/Volumes-of-mine` isn't mistaken for something under `/Volumes`.
fn is_under_a_mount_point(path: &Path) -> bool {
    MOUNT_POINT_PREFIXES.iter().any(|prefix| path.starts_with(prefix))
}

/// The comparable form of a candidate path: absolute, with trailing separators and
/// `.` components gone, so `~/Documents` and `~/Documents/` are one root.
fn normalized(path: &Path) -> Option<PathBuf> {
    path.is_absolute().then(|| path.components().collect())
}

/// The cloud roots worth walking, in order: each File Provider domain under
/// `~/Library/CloudStorage`, then the legacy Dropbox folder, then iCloud Drive.
///
/// Enumerating the domains reads a TCC-anchored directory, so a pending FDA gate skips
/// that half entirely; a later ask (the answer is recomputed, not frozen) picks the
/// domains up once the decision is in. The domains are sorted because `read_dir` order
/// is arbitrary, and a walk order that reshuffles between asks is one nobody can debug.
fn cloud_roots(home: &Path, fda_pending: bool) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    let cloud_storage = home.join(CLOUD_STORAGE_DIR);
    if !(fda_pending && tcc_paths::is_potentially_tcc_restricted(&cloud_storage))
        && let Ok(entries) = fs::read_dir(&cloud_storage)
    {
        let mut domains: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        domains.sort();
        roots.extend(domains);
    }

    roots.push(home.join(DROPBOX_DIR));
    roots.push(home.join(ICLOUD_DRIVE_DIR));
    roots
}

// ---------------------------------------------------------------------------
// Last session's tabs
// ---------------------------------------------------------------------------

/// The pane paths in `app-status.json`, most recently active first: the focused pane's
/// active tab, the other pane's active tab, then the remaining tabs of each side.
///
/// Only tabs on the boot volume count, which is what `volumeId` is for: a same-looking
/// path on a share is a different folder. A first run has no file and no tabs.
fn parse_tab_paths(contents: &str, home: &Path) -> Vec<PathBuf> {
    let Ok(status) = serde_json::from_str::<serde_json::Value>(contents) else {
        return Vec::new();
    };

    let sides = if status.get("focusedPane").and_then(|v| v.as_str()) == Some("right") {
        ["right", "left"]
    } else {
        ["left", "right"]
    }
    .map(|side| side_tabs(&status, side, home));

    let mut paths: Vec<PathBuf> = sides.iter().filter_map(|side| side.active.clone()).collect();
    for side in &sides {
        paths.extend(side.others.iter().cloned());
    }
    paths
}

/// One pane's persisted tabs, split by what the user was looking at.
#[derive(Default)]
struct SideTabs {
    /// The tab this pane had open.
    active: Option<PathBuf>,
    /// The rest, in tab-bar order.
    others: Vec<PathBuf>,
}

/// One side's tabs, falling back to the pre-tabs scalar keys an install that hasn't
/// been touched in a while may still be the only carrier of.
fn side_tabs(status: &serde_json::Value, side: &str, home: &Path) -> SideTabs {
    let mut tabs = SideTabs::default();

    if let Some(pane) = status.get(format!("{side}Tabs")) {
        let active_id = pane.get("activeTabId").and_then(|v| v.as_str());
        for tab in pane.get("tabs").and_then(|v| v.as_array()).into_iter().flatten() {
            let Some(path) = local_path(tab.get("path"), tab.get("volumeId"), home) else {
                continue;
            };
            let is_active = active_id.is_some() && tab.get("id").and_then(|v| v.as_str()) == active_id;
            if is_active && tabs.active.is_none() {
                tabs.active = Some(path);
            } else {
                tabs.others.push(path);
            }
        }
        if tabs.active.is_some() || !tabs.others.is_empty() {
            return tabs;
        }
    }

    tabs.active = local_path(
        status.get(format!("{side}Path")),
        status.get(format!("{side}VolumeId")),
        home,
    );
    tabs
}

/// A persisted pane path, but only when it names a folder on the boot volume: a path
/// on a share can look exactly like a local one, so `volume_id` is what tells them
/// apart. Absent means the boot volume, matching what the frontend stores by default.
///
/// `~` is what a pane persists while it sits in the home folder, so it expands here;
/// anything else that isn't absolute is a virtual location (search results, MTP) that
/// no walk can take.
fn local_path(path: Option<&serde_json::Value>, volume_id: Option<&serde_json::Value>, home: &Path) -> Option<PathBuf> {
    let volume = volume_id.and_then(|v| v.as_str()).unwrap_or(ROOT_VOLUME_ID);
    if volume != ROOT_VOLUME_ID {
        return None;
    }
    let raw = path.and_then(|v| v.as_str())?;
    if raw == "~" {
        return Some(home.to_path_buf());
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        return Some(home.join(rest));
    }
    let path = PathBuf::from(raw);
    path.is_absolute().then_some(path)
}

#[cfg(test)]
mod tests;
