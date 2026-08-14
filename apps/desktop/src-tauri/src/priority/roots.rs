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
        if (self.on_another_volume)(&path) {
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
mod tests {
    use super::*;
    use crate::test_support::TestDir;

    /// Nothing is on another volume unless a test says so.
    fn all_local(_path: &Path) -> bool {
        false
    }

    /// A home directory with `folders` in it, each holding one file so the non-empty
    /// bar is met. Returns the handle (which owns the directory) alongside its path.
    fn home_with(label: &str, folders: &[&str]) -> (TestDir, PathBuf) {
        let dir = TestDir::new(label);
        let home = dir.join("home");
        fs::create_dir_all(&home).expect("create home");
        for folder in folders {
            let path = home.join(folder);
            fs::create_dir_all(&path).expect("create folder");
            fs::write(path.join("a-file.txt"), b"x").expect("write file");
        }
        (dir, home)
    }

    fn inputs(home: &Path) -> RootInputs {
        RootInputs {
            home: home.to_path_buf(),
            tabs: Vec::new(),
            favorites: Vec::new(),
            fda_pending: false,
        }
    }

    /// The strongest signal there is: where the user actually was. It has to outrank
    /// every guess, or the first phase walks a folder nobody opened.
    #[test]
    fn last_session_s_tabs_lead_the_order() {
        let (_dir, home) = home_with("roots_tabs_lead", &["Documents", "Downloads", "Projects"]);
        let mut inputs = inputs(&home);
        inputs.tabs = vec![home.join("Projects")];
        inputs.favorites = vec![home.join("Documents")];

        let roots = rank_roots(&inputs, &all_local);

        assert_eq!(
            roots.first(),
            Some(&home.join("Projects")),
            "the tab beats the favorite and the standard folders"
        );
        assert_eq!(roots.get(1), Some(&home.join("Documents")), "then the favorite");
        assert_eq!(roots.get(2), Some(&home.join("Downloads")), "then the standard folders");
    }

    /// Home is the sweep-up phase, so it comes last: put it first and every later root
    /// would be a descendant of it and get dropped, collapsing the whole schedule into
    /// one undifferentiated walk of home.
    #[test]
    fn home_comes_last_so_the_folders_inside_it_are_walked_first() {
        let (_dir, home) = home_with("roots_home_last", &["Downloads"]);

        let roots = rank_roots(&inputs(&home), &all_local);

        assert_eq!(roots.last(), Some(&home), "home sweeps up what the guesses missed");
        assert!(roots.len() > 1, "and it isn't the only root");
    }

    /// A folder named twice (a tab and a favorite, say) is one root, not two walks of
    /// the same ground. Trailing slashes are the same folder too.
    #[test]
    fn a_folder_named_twice_becomes_one_root() {
        let (_dir, home) = home_with("roots_dedupe", &["Documents"]);
        let mut inputs = inputs(&home);
        inputs.tabs = vec![home.join("Documents")];
        inputs.favorites = vec![PathBuf::from(format!("{}/", home.join("Documents").display()))];

        let roots = rank_roots(&inputs, &all_local);

        assert_eq!(
            roots.iter().filter(|r| *r == &home.join("Documents")).count(),
            1,
            "one entry for one folder: {roots:?}"
        );
    }

    /// A root inside an earlier root is already covered by it. Keeping it would walk
    /// the same ground twice and push a genuinely new folder past the cap.
    #[test]
    fn a_root_inside_an_earlier_root_is_dropped() {
        let (_dir, home) = home_with("roots_descendant", &["Projects"]);
        let nested = home.join("Projects/cmdr");
        fs::create_dir_all(&nested).expect("create nested");
        let mut inputs = inputs(&home);
        inputs.tabs = vec![home.join("Projects"), nested.clone()];

        let roots = rank_roots(&inputs, &all_local);

        assert!(roots.contains(&home.join("Projects")));
        assert!(!roots.contains(&nested), "already covered by its parent: {roots:?}");
    }

    /// A path that isn't there can't be walked, and a stale favorite or a tab pointing
    /// at an ejected drive is common. Dropping it keeps a phase from failing instantly.
    #[test]
    fn a_folder_that_isn_t_there_is_never_a_root() {
        let (_dir, home) = home_with("roots_missing", &["Documents"]);
        let mut inputs = inputs(&home);
        inputs.favorites = vec![home.join("Gone")];

        let roots = rank_roots(&inputs, &all_local);

        assert!(!roots.contains(&home.join("Gone")), "{roots:?}");
        assert!(roots.contains(&home.join("Documents")), "the real folders still rank");
    }

    /// A file isn't a walk root, however the user came to point at one.
    #[test]
    fn a_file_is_never_a_root() {
        let (_dir, home) = home_with("roots_file", &["Documents"]);
        let file = home.join("notes.txt");
        fs::write(&file, b"x").expect("write file");
        let mut inputs = inputs(&home);
        inputs.favorites = vec![file.clone()];

        let roots = rank_roots(&inputs, &all_local);

        assert!(!roots.contains(&file), "{roots:?}");
        assert!(roots.contains(&home.join("Documents")), "the real folders still rank");
    }

    /// The guessed home folders have to earn their slot: an account that never used
    /// `~/Music` shouldn't spend a phase proving it is empty, while `~/Downloads` with
    /// files in it is exactly what the user will search first.
    #[test]
    fn an_empty_standard_home_folder_is_skipped_and_a_used_one_is_taken() {
        let (_dir, home) = home_with("roots_non_empty", &["Downloads"]);
        fs::create_dir_all(home.join("Music")).expect("create empty Music");

        let roots = rank_roots(&inputs(&home), &all_local);

        assert!(roots.contains(&home.join("Downloads")));
        assert!(!roots.contains(&home.join("Music")), "{roots:?}");
    }

    /// A folder the user named themselves is taken as-is, empty or not: they told us it
    /// matters, which is a better signal than its current contents.
    #[test]
    fn an_empty_folder_the_user_named_is_still_a_root() {
        let (_dir, home) = home_with("roots_named_empty", &[]);
        let empty = home.join("Scans");
        fs::create_dir_all(&empty).expect("create empty");
        let mut inputs = inputs(&home);
        inputs.favorites = vec![empty.clone()];

        let roots = rank_roots(&inputs, &all_local);

        assert!(roots.contains(&empty), "{roots:?}");
    }

    /// A true first run: no tabs, no favorites file to speak of. The order still has to
    /// be useful, or the very install that most needs a fast index gets none of it.
    #[test]
    fn a_first_run_still_ranks_the_home_folders() {
        let (_dir, home) = home_with("roots_first_run", &["Downloads", "Documents", "Desktop"]);

        let roots = rank_roots(&inputs(&home), &all_local);

        assert_eq!(
            roots,
            vec![
                home.join("Downloads"),
                home.join("Documents"),
                home.join("Desktop"),
                home.clone(),
            ]
        );
    }

    /// The favorites seed is platform-dependent (`/Applications` on macOS, the home
    /// folder on Linux), so the ranking has to take whatever the store hands it rather
    /// than assume the macOS four. Both seeds land, and the Linux one's home entry
    /// doesn't swallow the folders below it.
    #[test]
    fn both_platform_favorite_seeds_rank() {
        let (_dir, home) = home_with("roots_seeds", &["Desktop", "Documents", "Downloads"]);
        let applications = home.join("Applications");
        fs::create_dir_all(&applications).expect("create Applications");

        let mut macos = inputs(&home);
        macos.favorites = vec![
            applications.clone(),
            home.join("Desktop"),
            home.join("Documents"),
            home.join("Downloads"),
        ];
        let macos_roots = rank_roots(&macos, &all_local);
        assert_eq!(
            macos_roots,
            vec![
                applications,
                home.join("Desktop"),
                home.join("Documents"),
                home.join("Downloads"),
                home.clone(),
            ]
        );

        let mut linux = inputs(&home);
        linux.favorites = vec![
            home.clone(),
            home.join("Desktop"),
            home.join("Documents"),
            home.join("Downloads"),
        ];
        let linux_roots = rank_roots(&linux, &all_local);
        assert_eq!(
            linux_roots,
            vec![home.clone()],
            "a favorited home covers the rest, so it is the whole schedule"
        );
    }

    /// Someone with a long favorites list must not turn the first phase into a whole
    /// drive walk. The cap holds even when every candidate is real.
    #[test]
    fn the_order_stops_at_the_cap() {
        let (_dir, home) = home_with("roots_cap", &[]);
        let mut inputs = inputs(&home);
        inputs.favorites = (0..MAX_ROOTS + 10)
            .map(|i| {
                let path = home.join(format!("folder-{i}"));
                fs::create_dir_all(&path).expect("create folder");
                path
            })
            .collect();

        let roots = rank_roots(&inputs, &all_local);

        assert_eq!(roots.len(), MAX_ROOTS);
        assert!(!roots.contains(&home), "home lost its slot to the favorites");
    }

    /// `~/Library` is in scope for the index and never a root: it is the biggest, most
    /// churn-heavy subtree in home, so walking it early spends the phase that was
    /// supposed to make the user's own files searchable.
    #[test]
    fn the_library_folder_is_never_a_root() {
        let (_dir, home) = home_with("roots_library", &["Library", "Documents"]);
        let mut inputs = inputs(&home);
        inputs.favorites = vec![home.join("Library")];

        let roots = rank_roots(&inputs, &all_local);

        assert!(!roots.contains(&home.join("Library")), "{roots:?}");
        assert!(roots.contains(&home.join("Documents")));
    }

    /// Cloud roots are worth walking, but after the local folders: a File Provider read
    /// can stall for a long time, and a stall must never delay `~/Downloads`.
    #[test]
    fn cloud_roots_come_after_the_local_folders() {
        let (_dir, home) = home_with("roots_cloud", &["Downloads"]);
        let dropbox = home.join(DROPBOX_DIR);
        fs::create_dir_all(&dropbox).expect("create Dropbox");
        let domain = home.join(CLOUD_STORAGE_DIR).join("GoogleDrive-someone@example.com");
        fs::create_dir_all(&domain).expect("create cloud domain");

        let roots = rank_roots(&inputs(&home), &all_local);

        let position = |path: &Path| roots.iter().position(|r| r == path);
        assert!(position(&dropbox).is_some(), "{roots:?}");
        assert!(
            position(&domain).is_some(),
            "each cloud domain is its own root: {roots:?}"
        );
        assert!(
            position(&home.join("Downloads")) < position(&dropbox),
            "local first: {roots:?}"
        );
    }

    /// A favorite on a share is a folder on ANOTHER index. Walking it as part of the
    /// boot volume's schedule would spend a phase on ground this volume doesn't own.
    #[test]
    fn a_folder_on_another_volume_isn_t_a_root_of_this_one() {
        let (_dir, home) = home_with("roots_other_volume", &["Documents"]);
        let elsewhere = home.join("mounted-share");
        fs::create_dir_all(&elsewhere).expect("create share");
        let mut inputs = inputs(&home);
        inputs.favorites = vec![elsewhere.clone()];
        let on_share = |path: &Path| path.starts_with(&elsewhere);

        let roots = rank_roots(&inputs, &on_share);

        assert!(!roots.contains(&elsewhere), "{roots:?}");
        assert!(roots.contains(&home.join("Documents")));
    }

    /// Every signal behind the ranking describes the user's own machine, so a share
    /// gets no order from here rather than inheriting somebody's home folder.
    #[test]
    fn only_the_boot_volume_gets_an_order() {
        assert!(priority_roots("smb-naspi").is_empty());
    }

    /// While the Full Disk Access decision is pending, a stat on a protected folder
    /// raises a system popup on top of our own onboarding modal (several, stacked). So
    /// those paths are taken on trust instead: `~/Downloads` and its siblings exist on
    /// essentially every account, and a walk of one that doesn't simply finds nothing.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_protected_folder_is_taken_on_trust_while_the_fda_gate_is_pending() {
        let real_home = dirs::home_dir().expect("a home directory");
        // Inside `~/Downloads`, so TCC covers it, and absent, so only the pending rule
        // can put it in the list.
        let protected = real_home.join("Downloads/cmdr-priority-roots-absent");
        assert!(
            tcc_paths::is_potentially_tcc_restricted(&protected),
            "the fixture has to be TCC-anchored for this test to mean anything"
        );
        assert!(!protected.exists(), "the fixture must not actually exist");

        let mut pending = inputs(&real_home);
        pending.favorites = vec![protected.clone()];
        pending.fda_pending = true;
        assert!(rank_roots(&pending, &all_local).contains(&protected));

        let mut granted = inputs(&real_home);
        granted.favorites = vec![protected.clone()];
        assert!(
            !rank_roots(&granted, &all_local).contains(&protected),
            "once the gate is open we check for real"
        );
    }

    // -- last session's tabs --

    /// Most recently active first means the focused pane's active tab, then the other
    /// pane's, then the rest. That is the closest thing the store keeps to a recency
    /// order, and the first entry is where the user was looking when they quit.
    #[test]
    fn the_focused_pane_s_active_tab_leads_the_tab_order() {
        let home = PathBuf::from("/Users/david");
        let contents = r#"{
            "focusedPane": "right",
            "leftTabs": {
                "activeTabId": "l2",
                "tabs": [
                    { "id": "l1", "path": "/Users/david/Projects", "volumeId": "root" },
                    { "id": "l2", "path": "/Users/david/Documents", "volumeId": "root" }
                ]
            },
            "rightTabs": {
                "activeTabId": "r1",
                "tabs": [
                    { "id": "r1", "path": "/Users/david/Downloads", "volumeId": "root" },
                    { "id": "r2", "path": "/Users/david/Desktop", "volumeId": "root" }
                ]
            }
        }"#;

        assert_eq!(
            parse_tab_paths(contents, &home),
            vec![
                PathBuf::from("/Users/david/Downloads"),
                PathBuf::from("/Users/david/Documents"),
                PathBuf::from("/Users/david/Desktop"),
                PathBuf::from("/Users/david/Projects"),
            ]
        );
    }

    /// A tab on a share or on the virtual network volume describes another index, and
    /// its path can look exactly like a local one. `volumeId` is what tells them apart.
    #[test]
    fn a_tab_on_another_volume_is_ignored() {
        let home = PathBuf::from("/Users/david");
        let contents = r#"{
            "focusedPane": "left",
            "leftTabs": {
                "activeTabId": "l1",
                "tabs": [
                    { "id": "l1", "path": "/Volumes/naspi/media", "volumeId": "smb-naspi" },
                    { "id": "l2", "path": "/Users/david/Documents", "volumeId": "root" }
                ]
            }
        }"#;

        assert_eq!(
            parse_tab_paths(contents, &home),
            vec![PathBuf::from("/Users/david/Documents")]
        );
    }

    /// `~` is what a pane persists when it is sitting in the home folder, so an
    /// unexpanded one would silently drop the most common tab there is.
    #[test]
    fn a_tilde_tab_path_expands_to_the_home_folder() {
        let home = PathBuf::from("/Users/david");
        let contents = r#"{
            "focusedPane": "left",
            "leftTabs": {
                "activeTabId": "l1",
                "tabs": [{ "id": "l1", "path": "~", "volumeId": "root" }]
            },
            "rightTabs": {
                "activeTabId": "r1",
                "tabs": [{ "id": "r1", "path": "~/Downloads", "volumeId": "root" }]
            }
        }"#;

        assert_eq!(
            parse_tab_paths(contents, &home),
            vec![home.clone(), home.join("Downloads")]
        );
    }

    /// An install that hasn't been touched since before tabs existed still carries its
    /// pane state in the scalar keys, and it is the same signal.
    #[test]
    fn the_pre_tabs_keys_still_answer() {
        let home = PathBuf::from("/Users/david");
        let contents = r#"{
            "focusedPane": "left",
            "leftPath": "/Users/david/Documents",
            "leftVolumeId": "root",
            "rightPath": "/Users/david/Downloads",
            "rightVolumeId": "root"
        }"#;

        assert_eq!(
            parse_tab_paths(contents, &home),
            vec![
                PathBuf::from("/Users/david/Documents"),
                PathBuf::from("/Users/david/Downloads"),
            ]
        );
    }

    /// A first run has no file at all, and a hand-mangled one is no reason to have no
    /// walk order: the later signals still answer.
    #[test]
    fn an_unreadable_store_yields_no_tabs_rather_than_an_opinion() {
        let home = PathBuf::from("/Users/david");
        assert!(parse_tab_paths("", &home).is_empty());
        assert!(parse_tab_paths("{not json", &home).is_empty());
        assert!(parse_tab_paths("{}", &home).is_empty());
    }
}
