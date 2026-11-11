//! Tests for the shared classifiers (`super`).

use super::*;

#[test]
fn denylist_is_folded_set_membership() {
    assert!(is_denylisted("node_modules"));
    assert!(is_denylisted("NODE_MODULES"), "folded, so case doesn't matter");
    assert!(is_denylisted(".git"));
    assert!(!is_denylisted("my_project"));
    // Set-membership, not substring: a name CONTAINING a denylisted word isn't denylisted.
    assert!(!is_denylisted("node_modules_backup"));
}

#[test]
fn path_class_matches_the_fixture_rules() {
    let home = "/Users/test";
    assert_eq!(path_class("/Users/test/Downloads", home), PathClass::UserContent);
    assert_eq!(
        path_class("/Users/test/Documents/invoices", home),
        PathClass::UserContent
    );
    assert_eq!(path_class("/Users/test/Library/Caches", home), PathClass::SystemOrCache);
    assert_eq!(path_class("/Users/test/projects/webapp", home), PathClass::Neutral);
}

#[test]
fn hidden_or_system_covers_dotfiles_and_library() {
    let home = "/Users/test";
    assert!(is_hidden_or_system("/Users/test/.config", ".config", home));
    assert!(is_hidden_or_system("/Users/test/Library/Caches", "Caches", home));
    assert!(!is_hidden_or_system("/Users/test/Downloads", "Downloads", home));
}

#[test]
fn project_markers_are_folded_membership() {
    assert!(is_project_marker(".git"));
    assert!(is_project_marker("cargo.toml"));
    assert!(is_project_marker("package.json"));
    assert!(!is_project_marker("readme.md"));
}

#[test]
fn self_floors_covers_denylist_and_hidden_system() {
    let home = "/Users/test";
    assert!(
        self_floors("/Users/test/proj/node_modules", "node_modules", home),
        "a denylisted folder self-floors"
    );
    assert!(
        self_floors("/Users/test/.config", ".config", home),
        "a dotfile self-floors"
    );
    assert!(
        self_floors("/Users/test/Library/Caches", "Caches", home),
        "a system/cache folder self-floors"
    );
    assert!(
        !self_floors("/Users/test/projects/webapp", "webapp", home),
        "an ordinary folder doesn't self-floor"
    );
}

#[test]
fn under_floored_paths_marks_descendants_of_a_floored_ancestor() {
    let home = "/Users/test";
    let paths = [
        "/Users/test/projects/webapp",
        "/Users/test/projects/webapp/node_modules",
        "/Users/test/projects/webapp/node_modules/react",
        "/Users/test/projects/webapp/node_modules/react/cjs",
        "/Users/test/projects/webapp/.git",
        "/Users/test/projects/webapp/.git/refs/heads",
        "/Users/test/Documents/invoices",
    ];
    let under = under_floored_paths(paths.iter().copied(), home);

    // Descendants of node_modules and .git are under-floored.
    assert!(under.contains("/Users/test/projects/webapp/node_modules/react"));
    assert!(under.contains("/Users/test/projects/webapp/node_modules/react/cjs"));
    assert!(under.contains("/Users/test/projects/webapp/.git/refs/heads"));

    // The self-flooring anchors themselves are NOT in the set (they floor via
    // their own flag, not this one).
    assert!(!under.contains("/Users/test/projects/webapp/node_modules"));
    assert!(!under.contains("/Users/test/projects/webapp/.git"));

    // Folders outside any floored subtree are untouched.
    assert!(!under.contains("/Users/test/projects/webapp"));
    assert!(!under.contains("/Users/test/Documents/invoices"));
}

#[test]
fn temp_roots_classify_as_system_or_cache() {
    let home = "/Users/test";
    for root in TEMP_ROOTS {
        assert_eq!(
            path_class(root, home),
            PathClass::SystemOrCache,
            "{root} is a temp root"
        );
    }
    assert_eq!(
        path_class("/private/tmp/claude-501/session", home),
        PathClass::SystemOrCache,
        "anything under a temp root is system/cache too"
    );
    assert_eq!(
        path_class("/private/var/folders/xx/yy/T", home),
        PathClass::SystemOrCache,
        "the macOS per-user $TMPDIR lives under /private/var/folders"
    );
}

#[test]
fn a_temp_root_floors_itself_and_its_whole_subtree() {
    let home = "/Users/test";
    assert!(self_floors("/private/tmp", "tmp", home), "a temp root self-floors");
    assert!(
        under_floored_ancestor("/private/tmp/claude-501/scratchpad", home),
        "a folder under a temp root floors via the ancestor rule"
    );
    assert!(floors_by_path("/private/tmp/claude-501/scratchpad", home));
    assert!(floors_by_path("/private/var/folders/xx/yy/T/build", home));
}

#[test]
fn a_temp_root_prefix_doesnt_capture_a_lookalike_sibling() {
    let home = "/Users/test";
    assert_eq!(
        path_class("/tmpfoo/project", home),
        PathClass::Neutral,
        "/tmpfoo is not /tmp"
    );
    assert!(!floors_by_path("/tmpfoo/project", home));
    assert_eq!(path_class("/tmpfoo", home), PathClass::Neutral);
    assert_eq!(path_class("/private/tmpfoo", home), PathClass::Neutral);
}

#[test]
fn a_marker_doesnt_promote_home_or_a_volume_root() {
    let home = "/Users/test";
    assert_eq!(
        path_class_with_marker(home, home, Some(0)),
        PathClass::Neutral,
        "a .git in $HOME means dotfiles, not a project the user works in"
    );
    assert_eq!(
        path_class_with_marker("/", home, Some(0)),
        PathClass::Neutral,
        "a marker at the boot volume root doesn't promote the whole disk"
    );
    for root in cmdr_fs::mount_layout::user_mount_prefixes() {
        let mount = format!("{root}backup");
        assert_eq!(
            path_class_with_marker(&mount, home, Some(0)),
            PathClass::Neutral,
            "{mount} is a volume root"
        );
    }
}

#[test]
fn home_does_not_floor_even_when_its_promotion_is_suppressed() {
    // Flooring $HOME would propagate through `under_floored_ancestor` to the
    // whole home directory and disable the feature.
    let home = "/Users/test";
    assert!(!self_floors(home, "test", home));
    assert!(!floors_by_path(home, home));
    assert!(!floors_by_path("/Users/test/projects/webapp", home));
}

#[test]
fn a_marker_still_promotes_a_real_project_dir() {
    let home = "/Users/test";
    assert_eq!(
        path_class_with_marker("/Users/test/projects-git/vdavid/cmdr", home, Some(0)),
        PathClass::ProjectRoot,
        "an ordinary project dir keeps the strongest prior"
    );
    assert_eq!(
        path_class_with_marker("/Volumes/backup/repos/thing", home, Some(0)),
        PathClass::ProjectRoot,
        "a project UNDER a volume root still promotes"
    );
}

#[test]
fn volume_roots_are_the_disk_root_and_one_level_under_a_mount_prefix() {
    assert!(is_volume_root("/"));
    for prefix in cmdr_fs::mount_layout::user_mount_prefixes() {
        assert!(is_volume_root(&format!("{prefix}backup")), "{prefix}backup");
        assert!(
            !is_volume_root(&format!("{prefix}backup/photos")),
            "one level under a mount prefix only"
        );
    }
    assert!(!is_volume_root("/Users/test"));
    assert!(!is_volume_root("/Users"));
}

/// The stamp is stable within a build. An unstable one would rescore every
/// volume on every launch, which is the expensive direction of this mechanism
/// failing.
#[test]
fn the_scoring_policy_fingerprint_is_stable() {
    assert_eq!(scoring_policy_fingerprint(), scoring_policy_fingerprint());
    assert_eq!(scoring_policy_fingerprint().len(), 16, "a 64-bit FNV-1a in hex");
}

/// Editing any list the classifiers read moves the fingerprint, which is the
/// whole mechanism: a store scored under the old list re-arms with no version
/// constant for anyone to forget to bump. Simulated over the real `parts`
/// shape, since the constants themselves can't be mutated at runtime.
#[test]
fn adding_a_name_to_any_policy_list_moves_the_fingerprint() {
    let baseline = scoring_policy_fingerprint();
    for label in [
        "temp_roots",
        "mount_prefixes",
        "system_roots",
        "bundle_suffixes",
        "library_package_suffixes",
        "home_cache",
        "home_content",
        "project_markers",
        "denylist",
    ] {
        let mut parts = scoring_policy_parts();
        // Insert a new name right after the perturbed list's own label.
        let at = parts.iter().position(|p| *p == label).expect("label present") + 1;
        parts.insert(at, "/a-newly-listed-name");
        assert_ne!(
            crate::fingerprint::fingerprint_of(&parts),
            baseline,
            "a name added to {label} has to change the stamp"
        );
    }
    // And so does a rules change that no list can see.
    let mut parts = scoring_policy_parts();
    parts[1] = "a-later-rules-version";
    assert_ne!(crate::fingerprint::fingerprint_of(&parts), baseline);
}

// ── Regression anchors from the live `importance-root.db` (2026-10-26) ──
// Before these rules, `~/go/pkg/mod` stored at 0.84, `/opt/homebrew/lib` at 0.85,
// and `/Users` at 0.87, all `projectRoot`: a marker anywhere below promoted every
// ancestor with no depth limit, and half the global top 40 was package stores.

#[test]
fn package_stores_and_system_roots_are_system_or_cache() {
    let home = "/Users/test";
    for path in [
        "/Users/test/go/pkg",
        "/Users/test/go/pkg/mod/github.com/klauspost/compress@v1.20.0",
        "/opt/homebrew",
        "/opt/homebrew/lib",
        "/opt/homebrew/Cellar/python@3.15/3.15.1",
        "/usr/local/Cellar/git",
        "/usr/share/man",
        "/Library/User Template/Non_localized/Downloads",
        "/System/Library/Frameworks",
        "/private/etc/apache2",
    ] {
        assert_eq!(path_class(path, home), PathClass::SystemOrCache, "{path}");
        assert!(floors_by_path(path, home), "{path} should floor");
    }
    assert_eq!(
        path_class("/Users/test/go", home),
        PathClass::Neutral,
        "~/go itself (and ~/go/src) is the user's"
    );
    assert_eq!(path_class("/opt/homebrewer", home), PathClass::Neutral);
    assert_eq!(path_class("/Libraryish", home), PathClass::Neutral);
}

#[test]
fn the_insides_of_an_app_bundle_are_system_or_cache() {
    let home = "/Users/test";
    for path in [
        "/Applications/LibreOffice.app/Contents",
        "/Applications/LibreOffice.app/Contents/Resources/config",
        "/Users/test/Applications/Thing.app/Contents/MacOS",
    ] {
        assert_eq!(path_class(path, home), PathClass::SystemOrCache, "{path}");
    }
    assert_eq!(
        path_class("/Applications/LibreOffice.app", home),
        PathClass::Neutral,
        "the bundle itself isn't its insides"
    );
    assert_eq!(
        path_class(
            "/Users/test/Pictures/Photos Library.photoslibrary/database/search",
            home
        ),
        PathClass::SystemOrCache,
        "a library package's insides are the app's database"
    );
    assert_eq!(
        path_class("/Users/test/Pictures/Photos Library.photoslibrary", home),
        PathClass::Neutral,
        "the package itself is the user's"
    );
    assert_eq!(
        path_class("/Users/test/projects/getcmdr.app/src", home),
        PathClass::Neutral,
        "a project named like a domain isn't a bundle: no Contents"
    );
}

#[test]
fn a_system_root_never_captures_home_or_its_ancestors() {
    // A Linux home under `/var` (Fedora Silverblue's `/var/home/x`) must not
    // floor: that would switch the whole feature off.
    let home = "/var/home/test";
    assert!(!floors_by_path(home, home));
    assert!(!floors_by_path("/var/home/test/projects/webapp", home));
    assert!(!floors_by_path("/var/home", home));
    assert!(floors_by_path("/var/log", home), "the rest of /var still floors");
}

/// The OS roots are the boot disk's. On any other volume an index path is relative
/// to the volume's root, so a NAS share's top-level `Library` is the user's own
/// folder and must score, not floor.
#[test]
fn system_roots_apply_only_on_the_boot_disk() {
    let home = "/Users/test";
    let share = PathSpace::new(home, Disk::of(IndexVolumeKind::Smb));
    let boot = PathSpace::new(home, Disk::of(IndexVolumeKind::Local));
    for path in ["/Library/x", "/usr/share", "/opt/homebrew/lib", "/tmp/notes"] {
        assert!(!floors_by_path(path, share), "{path} is the user's folder on a share");
        assert_eq!(path_class(path, share), PathClass::Neutral, "{path}");
        assert!(floors_by_path(path, boot), "{path} is the OS's on the boot disk");
    }
    let external = PathSpace::new(home, Disk::of(IndexVolumeKind::LocalExternal));
    assert!(
        !floors_by_path("/Library/x", external),
        "an external drive is volume-relative too"
    );
    // The rules that aren't about the boot disk's own layout still apply anywhere.
    assert!(floors_by_path("/Apps/Thing.app/Contents/Resources", share));
    assert!(floors_by_path("/photos/node_modules", share));
}

#[test]
fn a_marker_never_promotes_an_ancestor_of_home() {
    let home = "/Users/test";
    assert_eq!(path_class_with_marker("/Users", home, Some(0)), PathClass::Neutral);
    assert_eq!(path_class_with_marker("/Users", home, Some(1)), PathClass::Neutral);
}

#[test]
fn marker_reach_is_a_direct_marker_or_one_level_below() {
    let home = "/Users/test";
    assert_eq!(
        path_class_with_marker("/Users/test/projects/cmdr", home, Some(0)),
        PathClass::ProjectRoot,
        "a direct marker is a project"
    );
    assert_eq!(
        path_class_with_marker("/Users/test/projects/cmdr/apps/desktop", home, Some(0)),
        PathClass::ProjectRoot,
        "a monorepo member with its own package.json is a direct marker"
    );
    assert_eq!(
        path_class_with_marker("/Users/test/projects", home, Some(1)),
        PathClass::ProjectRoot,
        "a folder of projects"
    );
    assert_eq!(
        path_class_with_marker("/Users/test/projects-git", home, Some(2)),
        PathClass::Neutral,
        "two levels up is a generic container"
    );
    assert_eq!(marker_depth(false, Some(2)), None, "out of reach reads as no marker");
    assert_eq!(marker_depth(true, Some(1)), Some(0));
    assert_eq!(marker_depth(false, Some(1)), Some(1));
}

#[test]
fn a_project_one_level_down_doesnt_relabel_user_content() {
    let home = "/Users/test";
    assert_eq!(
        path_class_with_marker("/Users/test/Desktop", home, Some(1)),
        PathClass::UserContent,
        "a repo dropped on the Desktop doesn't make the Desktop a project"
    );
    assert_eq!(
        path_class_with_marker("/Users/test/Desktop/vo-test", home, Some(0)),
        PathClass::ProjectRoot
    );
}

#[test]
fn under_floored_detects_an_ancestor_absent_from_the_path_set() {
    // The ancestor `node_modules` isn't in the input list (say the index pruned
    // it), but its descendant still floors — detection walks the path's own
    // components, not the sibling set.
    let home = "/Users/test";
    let paths = ["/Users/test/x/node_modules/pkg/dist"];
    let under = under_floored_paths(paths.iter().copied(), home);
    assert!(under.contains("/Users/test/x/node_modules/pkg/dist"));
}
