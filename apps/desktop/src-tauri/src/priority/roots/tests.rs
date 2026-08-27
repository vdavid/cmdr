//! Tests for the walk order in `../roots.rs`, over a synthetic home so the ranking
//! never depends on what happens to be on the machine running them.

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
        recent: Vec::new(),
        fda_pending: false,
    }
}

/// Recency's whole job is the first run, when there are no tabs and no favorites and
/// the order would otherwise be the same static list for everybody.
#[test]
fn on_a_first_run_recency_outranks_the_standard_home_folders() {
    let (_dir, home) = home_with("roots_recency_first_run", &["Downloads", "Documents", "code"]);
    let mut inputs = inputs(&home);
    inputs.recent = vec![home.join("code")];

    let roots = rank_roots(&inputs, &all_local);

    assert_eq!(
        roots.first(),
        Some(&home.join("code")),
        "the folder they actually work in beats the static list, got {roots:?}"
    );
}

/// ⚠️ Recency is a GUESS from an OS index; a favorite is the user saying it outright.
/// If this ever inverts, a stale Spotlight record outranks a deliberate choice.
#[test]
fn a_stated_favorite_still_beats_a_guessed_recent_folder() {
    let (_dir, home) = home_with("roots_recency_under_favorites", &["Pinned", "Busy"]);
    let mut inputs = inputs(&home);
    inputs.favorites = vec![home.join("Pinned")];
    inputs.recent = vec![home.join("Busy")];

    let roots = rank_roots(&inputs, &all_local);

    assert_eq!(roots.first(), Some(&home.join("Pinned")));
    assert_eq!(roots.get(1), Some(&home.join("Busy")));
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

/// A favorite at a mount point is dropped on path shape alone, before anything
/// stats it. The volume registry only knows the mounts Cmdr registered, so a stat
/// on an unregistered wedged share would block the index's thread for minutes.
#[test]
fn a_folder_at_a_mount_point_is_never_a_root() {
    let (_dir, home) = home_with("roots_mount_point", &["Documents"]);
    let mounted = Path::new(MOUNT_POINT_PREFIXES[0]).join("naspi/media");
    let mut inputs = inputs(&home);
    inputs.tabs = vec![mounted.clone()];
    let registry_untouched = |path: &Path| {
        assert!(
            !path.starts_with(MOUNT_POINT_PREFIXES[0]),
            "path shape alone must settle {path:?}, so nothing later can stat it"
        );
        false
    };

    let roots = rank_roots(&inputs, &registry_untouched);

    assert!(!roots.contains(&mounted), "{roots:?}");
    assert!(roots.contains(&home.join("Documents")), "the local folders still rank");
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
