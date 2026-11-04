//! The desktop source: `realpath` on scope paths, and the index database answering
//! the seam's questions.

use std::sync::atomic::AtomicBool;

use cmdr_index::ReadPool;
use cmdr_index::store::{IndexStore, ROOT_ID};

use super::*;
use crate::search::index::load_search_index;
use crate::search::seam::{ResolvedScope, resolve_scope};
use crate::test_support::TestDir;

// ── canonicalize_scope_path ─────────────────────────────────────

/// A scope path the PANE reports can be tilde-form: a tab sitting in the home
/// folder stores `~`, and since the default scope became "the current folder"
/// that string is what a search's include path is. Unexpanded it resolves to a
/// literal `~` — nothing in the index, and `/~` to a walk — so an ordinary
/// search from the home folder reported "the index doesn't cover this folder"
/// and then walked nothing at all.
#[test]
fn canonicalize_scope_path_expands_the_home_tilde() {
    let home = std::env::var("HOME").expect("HOME is set");
    assert_eq!(canonicalize_scope_path("~"), canonicalize_scope_path(&home));
    assert_eq!(
        canonicalize_scope_path("~/Documents"),
        canonicalize_scope_path(&format!("{home}/Documents"))
    );
    // A tilde INSIDE a path is an ordinary character, and a file may be named it.
    assert_eq!(canonicalize_scope_path("/tmp/~notme"), "/tmp/~notme");
}

#[cfg(unix)]
#[test]
fn canonicalize_scope_path_resolves_symlinks() {
    use std::os::unix::fs::symlink;

    // A real dir with a child, plus a symlink pointing at the real dir — the
    // /tmp → /private/tmp shape the index stores canonically but scopes report
    // symlinked.
    let base = TestDir::new("scope-canon");
    let real = base.join("real");
    std::fs::create_dir_all(real.join("child")).expect("create real dir");
    let link = base.join("link");
    symlink(&real, &link).expect("create symlink");

    // A path THROUGH the symlink canonicalizes to the real path (both fully
    // symlink-resolved), so a scope typed against the symlink now matches the
    // index's stored real path.
    let through_link = link.join("child").to_string_lossy().into_owned();
    let want = std::fs::canonicalize(real.join("child"))
        .expect("canonicalize real")
        .to_string_lossy()
        .into_owned();
    assert_eq!(canonicalize_scope_path(&through_link), want);

    // A non-existent path keeps the literal (best-effort: the offline-index case).
    let missing = link.join("nope").to_string_lossy().into_owned();
    assert_eq!(canonicalize_scope_path(&missing), missing);
}

// ── A mount-rooted volume's scope, through the database ──────────────
//
// Regression tests for the two live-QA failures: a volume-root scope
// (`/Volumes/naspi`) must search the WHOLE volume, and a path that isn't in the
// index must be reported as unresolved rather than silently returning nothing.

const TEST_MOUNT: &str = "/Volumes/cmdr-test-nas";

/// A mount-rooted index (SMB shape: `ROOT_ID` is the mount root) with a single
/// `/photos` folder, loaded. The mount root is a path that can't exist, so
/// `canonicalize_scope_path` keeps the literal deterministically.
fn mount_rooted_volume() -> (tempfile::TempDir, LoadedVolume) {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("index-smb-test.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
    IndexStore::insert_entry_v2(&conn, ROOT_ID, "photos", true, false, None, None, None, None).unwrap();
    let pool = ReadPool::new(db_path).expect("read pool");
    let index = load_search_index(&pool, &AtomicBool::new(false)).expect("load the arena");
    (dir, LoadedVolume::for_test(index, pool, Some(TEST_MOUNT.to_string())))
}

fn resolved(loaded: &LoadedVolume, paths: &[&str]) -> crate::search::seam::ScopeResolution {
    let paths: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
    resolve_scope(&IndexSource(loaded), &scope_pairs(&paths, None)).expect("the scope resolves")
}

#[test]
fn scope_at_volume_root_searches_whole_volume() {
    let (_dir, loaded) = mount_rooted_volume();
    // The mount root itself strips to "/" ⇒ the whole volume, not "nothing" (the live
    // bug).
    let r = resolved(&loaded, &[TEST_MOUNT]);
    assert_eq!(r.scope, ResolvedScope::Whole);
    assert!(r.unresolved.is_empty());
}

#[test]
fn scope_subpath_resolves_to_its_index_id() {
    let (_dir, loaded) = mount_rooted_volume();
    // `/Volumes/cmdr-test-nas/photos` → strip mount → `/photos` → resolves.
    let r = resolved(&loaded, &[&format!("{TEST_MOUNT}/photos")]);
    let photos = (0..loaded.index.len())
        .find(|&i| loaded.index.name(i) == "photos")
        .map(|i| loaded.index.id(i));
    assert_eq!(
        r.scope,
        ResolvedScope::Ids(vec![photos.expect("photos is in the arena")])
    );
    assert!(r.unresolved.is_empty());
}

#[test]
fn scope_path_not_in_index_is_reported_unresolved() {
    let (_dir, loaded) = mount_rooted_volume();
    let scope = format!("{TEST_MOUNT}/does-not-exist");
    let r = resolved(&loaded, &[&scope]);
    // Nothing resolved ⇒ match nothing AND surface the path honestly.
    assert_eq!(r.scope, ResolvedScope::Nothing);
    assert_eq!(r.unresolved, vec![scope]);
}

#[test]
fn scope_outside_mount_root_is_unresolved() {
    let (_dir, loaded) = mount_rooted_volume();
    // A path not under the volume's mount root can't map into its index.
    let r = resolved(&loaded, &["/Volumes/other/x"]);
    assert_eq!(r.scope, ResolvedScope::Nothing);
    assert_eq!(r.unresolved, vec!["/Volumes/other/x".to_string()]);
}

#[test]
fn the_index_reports_folder_sizes_by_id_and_in_a_range() {
    let (_dir, loaded) = mount_rooted_volume();
    let photos = (0..loaded.index.len())
        .find(|&i| loaded.index.name(i) == "photos")
        .map(|i| loaded.index.id(i))
        .expect("photos is in the arena");
    let source = IndexSource(&loaded);
    // No `dir_stats` row yet: unknown, ❌ not zero, and outside any range.
    assert_eq!(source.of(&[photos]).expect("the sizes read"), vec![None]);
    assert!(source.in_range(Some(1), None).expect("the range reads").is_empty());
}
