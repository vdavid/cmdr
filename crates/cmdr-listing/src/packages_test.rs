//! Packages macOS reports for a listing's dotted folders: `RowFacts::Packages` writes
//! them, re-places the row under the Ext sort, and `carry_forward_deferred`
//! keeps them across a watcher re-stat.

use super::caching_test::{insert_test_listing, make_entry};
use super::testing::{TestEngine, TestListingGuard};
use cmdr_sort::{DirectorySortMode, SortColumn, SortOrder};

fn cached(listing: &TestListingGuard, path: &str) -> (bool, String) {
    listing.with_listing(|cached| {
        let entry = cached
            .entries()
            .iter()
            .find(|e| e.path == path)
            .expect("entry is cached");
        (entry.is_package, entry.icon_id.clone())
    })
}

fn names(listing: &TestListingGuard) -> Vec<String> {
    listing.with_listing(|cached| cached.entries().iter().map(|e| e.name.clone()).collect())
}

#[test]
fn apply_packages_marks_a_dotted_folder_and_gives_it_a_pkg_icon() {
    let engine = TestEngine::new();
    let listing = insert_test_listing(
        &engine,
        "pkg_apply",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("Logo.sketchbundle", true, None)],
    );

    listing
        .engine()
        .inner
        .apply_packages_to_listing(listing.id(), vec![("/test/Logo.sketchbundle".to_string(), true)]);

    assert_eq!(
        cached(&listing, "/test/Logo.sketchbundle"),
        (true, "pkg:/test/Logo.sketchbundle".to_string())
    );
}

#[test]
fn apply_packages_revokes_a_detected_package_but_never_a_listed_one() {
    let engine = TestEngine::new();
    let mut detected = make_entry("Logo.sketchbundle", true, None);
    detected.set_package(true);
    let listing = insert_test_listing(
        &engine,
        "pkg_revoke",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![detected, make_entry("Cmdr.app", true, None)],
    );

    listing.engine().inner.apply_packages_to_listing(
        listing.id(),
        vec![
            ("/test/Logo.sketchbundle".to_string(), false),
            // A failed lookup on a listed bundle mustn't turn it into a folder (macOS).
            ("/test/Cmdr.app".to_string(), false),
        ],
    );

    assert_eq!(cached(&listing, "/test/Logo.sketchbundle"), (false, "dir".to_string()));
    // Only macOS reads a folder's name as a package (`cmdr_fs::file_extension`), so only there is
    // `Cmdr.app` a listed bundle to protect. Elsewhere it's a plain folder before and after.
    let listed_here = if cfg!(any(target_os = "macos", target_family = "wasm")) {
        (true, "pkg:/test/Cmdr.app".to_string())
    } else {
        (false, "dir".to_string())
    };
    assert_eq!(cached(&listing, "/test/Cmdr.app"), listed_here);
}

#[test]
fn apply_packages_skips_unknown_paths_and_files() {
    let engine = TestEngine::new();
    let listing = insert_test_listing(
        &engine,
        "pkg_skip",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![make_entry("notes.sketchbundle", false, Some(1))],
    );

    listing.engine().inner.apply_packages_to_listing(
        listing.id(),
        vec![
            ("/test/gone.bundle2".to_string(), true),
            ("/test/notes.sketchbundle".to_string(), true),
        ],
    );

    assert!(
        !cached(&listing, "/test/notes.sketchbundle").0,
        "a file never becomes a package"
    );
}

#[test]
fn a_detected_package_moves_under_the_ext_sort() {
    let engine = TestEngine::new();
    let listing = insert_test_listing(
        &engine,
        "pkg_resort",
        "/test",
        SortColumn::Extension,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        // Inserted in Ext order: no extension first, so the two folders lead.
        vec![
            make_entry("Logo.sketchbundle", true, None),
            make_entry("Zebra", true, None),
            make_entry("Cmdr.app", true, None),
        ],
    );
    assert_eq!(names(&listing), ["Logo.sketchbundle", "Zebra", "Cmdr.app"]);

    listing
        .engine()
        .inner
        .apply_packages_to_listing(listing.id(), vec![("/test/Logo.sketchbundle".to_string(), true)]);

    assert_eq!(names(&listing), ["Zebra", "Cmdr.app", "Logo.sketchbundle"]);
}

#[test]
fn carry_forward_keeps_a_detected_package_across_a_restat() {
    let engine = TestEngine::new();
    let mut detected = make_entry("Logo.sketchbundle", true, None);
    detected.set_package(true);
    let listing = insert_test_listing(
        &engine,
        "pkg_carry",
        "/test",
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        vec![detected],
    );

    // A watcher re-stat knows only the package list.
    let mut restat = make_entry("Logo.sketchbundle", true, Some(9));
    assert!(!restat.is_package);
    listing.engine().inner.carry_forward_deferred(listing.id(), &mut restat);

    assert!(restat.is_package);
    assert_eq!(restat.icon_id, "pkg:/test/Logo.sketchbundle");
}
