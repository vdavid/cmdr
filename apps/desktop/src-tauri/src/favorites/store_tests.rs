use super::*;

fn store_with(paths: &[(&str, &str)]) -> FavoritesStore {
    FavoritesStore {
        schema_version: CURRENT_SCHEMA_VERSION,
        favorites: paths
            .iter()
            .map(|(path, name)| Favorite {
                id: new_id(),
                path: path.to_string(),
                name: name.to_string(),
                shortcut: None,
                volume: None,
            })
            .collect(),
    }
}

// -- add: dedup, naming, ordering --

#[test]
fn add_appends_new_path_with_derived_name() {
    let mut store = FavoritesStore::default();
    add_to_store(&mut store, "/Users/x/Projects", None, None);
    assert_eq!(store.favorites.len(), 1);
    assert_eq!(store.favorites[0].path, "/Users/x/Projects");
    assert_eq!(
        store.favorites[0].name, "Projects",
        "name defaults to the path's file name"
    );
}

#[test]
fn add_uses_explicit_name_when_given() {
    let mut store = FavoritesStore::default();
    add_to_store(&mut store, "/Users/x/Projects", Some("Work".to_string()), None);
    assert_eq!(store.favorites[0].name, "Work");
}

#[test]
fn add_dedups_by_normalized_path_and_keeps_id() {
    let mut store = FavoritesStore::default();
    let first_id = add_to_store(&mut store, "/Users/x/a", None, None);
    add_to_store(&mut store, "/Users/x/b", None, None);

    // Re-add `/a` with a trailing slash: should dedup (not grow) and keep the original id.
    let dup_id = add_to_store(&mut store, "/Users/x/a/", None, None);
    assert_eq!(store.favorites.len(), 2, "trailing-slash re-add should collapse");
    assert_eq!(dup_id, first_id, "re-add keeps the existing id");
    // The re-added entry moves to the end.
    assert_eq!(store.favorites[0].path, "/Users/x/b");
    assert_eq!(store.favorites[1].path, "/Users/x/a");
}

#[test]
fn shortcuts_are_unique_case_insensitive_and_survive_readd() {
    let mut store = store_with(&[("/a", "A"), ("/b", "B")]);
    let first = store.favorites[0].id.clone();
    let second = store.favorites[1].id.clone();
    assert!(set_shortcut_in_store(&mut store, &first, Some("p")));
    assert_eq!(store.favorites[0].shortcut.as_deref(), Some("P"));
    assert!(set_shortcut_in_store(&mut store, &second, Some("P")));
    assert_eq!(store.favorites[0].shortcut, None);
    assert_eq!(store.favorites[1].shortcut.as_deref(), Some("P"));
    add_to_store(&mut store, "/b", None, None);
    assert_eq!(store.favorites[1].shortcut.as_deref(), Some("P"));
    assert!(set_shortcut_in_store(&mut store, &second, None));
    assert_eq!(store.favorites[1].shortcut, None);
}

#[test]
fn invalid_shortcut_and_unknown_id_do_not_mutate() {
    let mut store = store_with(&[("/a", "A")]);
    let id = store.favorites[0].id.clone();
    assert!(!set_shortcut_in_store(&mut store, &id, Some("1")));
    assert!(!set_shortcut_in_store(&mut store, &id, Some("AB")));
    assert!(!set_shortcut_in_store(&mut store, "missing", Some("A")));
    assert_eq!(store.favorites[0].shortcut, None);
}

#[test]
fn add_re_add_with_name_overrides_existing_label() {
    let mut store = FavoritesStore::default();
    add_to_store(&mut store, "/Users/x/a", None, None);
    add_to_store(&mut store, "/Users/x/a", Some("Renamed".to_string()), None);
    assert_eq!(store.favorites.len(), 1);
    assert_eq!(store.favorites[0].name, "Renamed");
}

#[test]
fn ids_are_unique_across_adds() {
    let mut store = FavoritesStore::default();
    add_to_store(&mut store, "/a", None, None);
    add_to_store(&mut store, "/b", None, None);
    assert_ne!(store.favorites[0].id, store.favorites[1].id);
}

// -- remove --

#[test]
fn remove_returns_true_when_found_and_false_otherwise() {
    let mut store = store_with(&[("/a", "a")]);
    let id = store.favorites[0].id.clone();
    assert!(remove_from_store(&mut store, &id));
    assert!(store.favorites.is_empty());
    assert!(!remove_from_store(&mut store, &id));
}

// -- rename --

#[test]
fn rename_updates_label_by_id() {
    let mut store = store_with(&[("/a", "a")]);
    let id = store.favorites[0].id.clone();
    assert!(rename_in_store(&mut store, &id, "New name"));
    assert_eq!(store.favorites[0].name, "New name");
    assert!(!rename_in_store(&mut store, "missing-id", "x"));
}

// -- reorder --

#[test]
fn reorder_applies_the_given_order() {
    let mut store = store_with(&[("/a", "a"), ("/b", "b"), ("/c", "c")]);
    let ids: Vec<String> = store.favorites.iter().map(|f| f.id.clone()).collect();
    let new_order = vec![ids[2].clone(), ids[0].clone(), ids[1].clone()];
    reorder_store(&mut store, &new_order);
    assert_eq!(store.favorites[0].path, "/c");
    assert_eq!(store.favorites[1].path, "/a");
    assert_eq!(store.favorites[2].path, "/b");
}

#[test]
fn reorder_appends_unnamed_entries_and_ignores_unknown_ids() {
    let mut store = store_with(&[("/a", "a"), ("/b", "b"), ("/c", "c")]);
    let ids: Vec<String> = store.favorites.iter().map(|f| f.id.clone()).collect();
    // Only name `/c`, plus a stale id. `/a` and `/b` must survive, appended in order.
    let partial = vec![ids[2].clone(), "stale-id".to_string()];
    reorder_store(&mut store, &partial);
    assert_eq!(store.favorites.len(), 3, "reorder must never drop entries");
    assert_eq!(store.favorites[0].path, "/c");
    assert_eq!(store.favorites[1].path, "/a");
    assert_eq!(store.favorites[2].path, "/b");
}

// -- name_from_path --

#[test]
fn name_from_path_uses_last_component_with_root_fallback() {
    assert_eq!(name_from_path("/Users/x/Downloads"), "Downloads");
    assert_eq!(name_from_path("/Applications"), "Applications");
    assert_eq!(name_from_path("/"), "/");
}

// -- default seed --

#[test]
fn default_favorites_are_four_with_unique_ids() {
    let defaults = default_favorites();
    assert_eq!(defaults.len(), 4);
    let names: Vec<&str> = defaults.iter().map(|f| f.name.as_str()).collect();
    #[cfg(target_os = "macos")]
    assert_eq!(names, vec!["Applications", "Desktop", "Documents", "Downloads"]);
    #[cfg(not(target_os = "macos"))]
    assert_eq!(names, vec!["Home", "Desktop", "Documents", "Downloads"]);
    let unique: std::collections::HashSet<&str> = defaults.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(unique.len(), 4, "every seeded favorite gets its own id");
}

// -- serialization round-trip --

#[test]
fn favorite_serialization_round_trip() {
    let f = Favorite {
        id: "abc-123".to_string(),
        path: "/Users/test/Documents".to_string(),
        name: "Documents".to_string(),
        shortcut: None,
        volume: None,
    };
    let json = serde_json::to_string_pretty(&f).unwrap();
    assert!(json.contains("\"path\": \"/Users/test/Documents\""));
    let back: Favorite = serde_json::from_str(&json).unwrap();
    assert_eq!(back, f);
    let legacy = r#"{"id":"old","path":"/a","name":"A"}"#;
    assert_eq!(serde_json::from_str::<Favorite>(legacy).unwrap().shortcut, None);
}

#[test]
fn store_serialization_carries_schema_version() {
    let store = FavoritesStore::default();
    let json = serde_json::to_string_pretty(&store).unwrap();
    assert!(json.contains("\"_schemaVersion\": 1"));
    let back: FavoritesStore = serde_json::from_str(&json).unwrap();
    assert_eq!(back.schema_version, 1);
    assert!(back.favorites.is_empty());
}

// -- disk: seed-once, no-reseed, empty-stays-empty, round-trip, recovery --

#[test]
fn missing_file_reads_as_none_signaling_seed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(FAVORITES_FILE_NAME);
    assert!(read_store_from_path(&path).is_none());
}

#[test]
fn seed_then_load_round_trip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(FAVORITES_FILE_NAME);

    let seeded = FavoritesStore {
        schema_version: CURRENT_SCHEMA_VERSION,
        favorites: default_favorites(),
    };
    write_store_to_path(&path, &seeded).expect("write");

    let loaded = read_store_from_path(&path).expect("present");
    // Assert against the seed itself, not a hardcoded macOS path: `default_favorites()` is
    // platform-specific (macOS leads with `/Applications`, Linux with the home dir, which is
    // `/root` under the root-user CI container), so a literal would fail the Linux test lane.
    assert_eq!(
        loaded.favorites, seeded.favorites,
        "round-trip must preserve the seeded favorites verbatim"
    );
}

#[test]
fn present_empty_file_stays_empty_no_reseed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(FAVORITES_FILE_NAME);
    // A user who removed every favorite: file present, empty list.
    let empty = FavoritesStore {
        schema_version: CURRENT_SCHEMA_VERSION,
        favorites: Vec::new(),
    };
    write_store_to_path(&path, &empty).expect("write");

    let loaded = read_store_from_path(&path).expect("present, not None");
    assert!(loaded.favorites.is_empty(), "an emptied list must not re-seed");
}

#[test]
fn persistence_round_trip_after_mutations() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(FAVORITES_FILE_NAME);

    let mut store = FavoritesStore::default();
    add_to_store(&mut store, "/Users/x/a", None, None);
    let removed_id = add_to_store(&mut store, "/Users/x/b", None, None);
    let shortcut_id = add_to_store(&mut store, "/Users/x/c", Some("See".to_string()), None);
    remove_from_store(&mut store, &removed_id);
    assert!(set_shortcut_in_store(&mut store, &shortcut_id, Some("p")));
    write_store_to_path(&path, &store).expect("write");

    let loaded = read_store_from_path(&path).expect("present");
    let paths: Vec<&str> = loaded.favorites.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["/Users/x/a", "/Users/x/c"]);
    assert_eq!(loaded.favorites[1].name, "See");
    assert_eq!(loaded.favorites[1].shortcut.as_deref(), Some("P"));
}

#[test]
fn corrupted_json_quarantines_and_starts_fresh() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(FAVORITES_FILE_NAME);
    fs::write(&path, "{not valid json").expect("write garbage");

    // A corrupt file is "initialized but broken", so it reads as an empty store (Some), not None
    // (which would re-seed the defaults over the user's lost-but-not-first-launch state).
    let store = read_store_from_path(&path).expect("corrupt reads as Some(empty)");
    assert!(store.favorites.is_empty());
    let broken = path.with_extension("json.broken");
    assert!(broken.exists(), "expected quarantine at {broken:?}");
    assert!(!path.exists());
}

#[test]
fn schema_version_mismatch_quarantines_and_starts_fresh() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(FAVORITES_FILE_NAME);
    fs::write(&path, r#"{"_schemaVersion": 2, "favorites": []}"#).expect("write");

    let store = read_store_from_path(&path).expect("mismatch reads as Some(empty)");
    assert!(store.favorites.is_empty());
    assert!(path.with_extension("json.broken").exists());
}

#[test]
fn present_but_unreadable_reads_as_some_empty_and_never_reseeds() {
    // A present-but-unreadable file (here simulated by a directory at the favorites path, which
    // makes `read_to_string` fail with a non-NotFound error) must NOT read as `None`: `None`
    // means "first launch" and would re-seed the four defaults OVER the user's real list.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(FAVORITES_FILE_NAME);
    fs::create_dir(&path).expect("create dir where the file would be");

    let store = read_store_from_path(&path).expect("present-but-unreadable must read as Some, not None");
    assert!(store.favorites.is_empty());
    // It must NOT have been quarantined or removed: we can't confirm corruption, so leave it be.
    assert!(path.exists(), "the unreadable path is left intact for a later read");
}

#[test]
fn stale_tmp_file_is_cleaned_up_on_read() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(FAVORITES_FILE_NAME);
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, "stale").expect("write tmp");
    let _ = read_store_from_path(&path);
    assert!(!tmp.exists(), "stale tmp should be removed");
}

// -- the volume a favorite lives on --

fn share(root: &str) -> FavoriteVolume {
    FavoriteVolume {
        id: "smb-naspi-0123".to_string(),
        root: root.to_string(),
        name: "naspi on nas.local".to_string(),
    }
}

/// ❗ Downgrade safety: a file written before `volume` existed reads verbatim, and one written by a
/// NEWER build (an extra field this one doesn't know) still reads instead of quarantining the whole
/// list.
#[test]
fn a_file_without_volume_and_one_with_unknown_fields_both_read_verbatim() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(FAVORITES_FILE_NAME);
    fs::write(
        &path,
        r#"{"_schemaVersion": 1, "favorites": [
            {"id": "a", "path": "/Users/x/Docs", "name": "Docs"},
            {"id": "b", "path": "/Volumes/naspi/docs", "name": "docs", "fromTheFuture": 7}
        ]}"#,
    )
    .expect("write");

    let store = read_store_from_path(&path).expect("present");
    assert_eq!(store.favorites.len(), 2);
    assert_eq!(store.favorites[0].volume, None);
    assert!(path.exists(), "nothing was quarantined");
}

#[test]
fn a_favorite_with_a_volume_round_trips() {
    let favorite = Favorite {
        id: "abc".to_string(),
        path: "/Volumes/naspi/docs".to_string(),
        name: "docs".to_string(),
        shortcut: Some("D".to_string()),
        volume: Some(share("/Volumes/naspi")),
    };
    let json = serde_json::to_string(&favorite).expect("serialize");
    assert!(json.contains(r#""volume":{"id":"smb-naspi-0123","root":"/Volumes/naspi","name":"naspi on nas.local"}"#));
    assert_eq!(serde_json::from_str::<Favorite>(&json).expect("parse"), favorite);
}

/// A legacy entry stays the shape it was on disk: no `"volume": null` written back.
#[test]
fn a_favorite_without_a_volume_writes_no_volume_key() {
    let mut store = FavoritesStore::default();
    add_to_store(&mut store, "/Users/x/a", None, None);
    let json = serde_json::to_string(&store).expect("serialize");
    assert!(!json.contains("volume"), "{json}");
}

/// ❗ One share mounted at two paths over time (`/Volumes/naspi`, then `/Volumes/naspi-1` while a
/// stale dir held the first name) is one favorite, and the re-add moves it onto where the share is
/// now.
#[test]
fn the_same_folder_on_one_volume_at_two_roots_is_one_favorite() {
    let mut store = FavoritesStore::default();
    let first = add_to_store(&mut store, "/Volumes/naspi/docs", None, Some(share("/Volumes/naspi")));
    let again = add_to_store(
        &mut store,
        "/Volumes/naspi-1/docs",
        None,
        Some(share("/Volumes/naspi-1")),
    );
    assert_eq!(first, again);
    assert_eq!(store.favorites.len(), 1);
    assert_eq!(store.favorites[0].path, "/Volumes/naspi-1/docs");
    assert_eq!(
        store.favorites[0].volume.as_ref().map(|v| v.root.as_str()),
        Some("/Volumes/naspi-1")
    );
}

#[test]
fn different_folders_or_volumes_stay_separate_favorites() {
    let mut store = FavoritesStore::default();
    add_to_store(&mut store, "/Volumes/naspi/docs", None, Some(share("/Volumes/naspi")));
    add_to_store(&mut store, "/Volumes/naspi/photos", None, Some(share("/Volumes/naspi")));
    let other = FavoriteVolume {
        id: "smb-other-4567".to_string(),
        ..share("/Volumes/naspi")
    };
    add_to_store(&mut store, "/Volumes/naspi/docs", None, Some(other));
    assert_eq!(store.favorites.len(), 3);
}

/// A legacy entry (no volume yet) and a fresh add of the same path are one favorite, and the add
/// teaches it its volume.
#[test]
fn a_re_add_of_a_legacy_path_fills_its_volume_in() {
    let mut store = store_with(&[("/Volumes/naspi/docs", "docs")]);
    let id = store.favorites[0].id.clone();
    let again = add_to_store(&mut store, "/Volumes/naspi/docs/", None, Some(share("/Volumes/naspi")));
    assert_eq!(again, id);
    assert_eq!(store.favorites.len(), 1);
    assert!(store.favorites[0].volume.is_some());
}

/// Scheme paths dedup on the same trailing-slash rule, and stay segment-wise: `…/data-1` is a
/// different folder from `…/data`.
#[test]
fn scheme_paths_collapse_a_trailing_slash_and_nothing_else() {
    let server = FavoriteVolume {
        id: "sftp-nas-89ab".to_string(),
        root: "sftp://u@h:22/".to_string(),
        name: "u@h".to_string(),
    };
    let mut store = FavoritesStore::default();
    add_to_store(&mut store, "sftp://u@h:22/", None, Some(server.clone()));
    add_to_store(&mut store, "sftp://u@h:22", None, Some(server.clone()));
    assert_eq!(store.favorites.len(), 1);
    add_to_store(&mut store, "sftp://u@h:22/srv/data", None, Some(server.clone()));
    add_to_store(&mut store, "sftp://u@h:22/srv/data-1", None, Some(server));
    assert_eq!(store.favorites.len(), 3);
    assert_eq!(normalize_for_dedup("sftp://u@h:22/"), "sftp://u@h:22");
}

// -- claiming legacy entries --

#[test]
fn a_claim_fills_an_empty_volume() {
    let mut store = store_with(&[("/Volumes/naspi/docs", "docs")]);
    let id = store.favorites[0].id.clone();
    assert!(claim_in_store(&mut store, &[(id, share("/Volumes/naspi"))]));
    assert_eq!(store.favorites[0].volume, Some(share("/Volumes/naspi")));
}

/// ❗ A claim is the listing's guess; a volume already there came from the user's own add (or an
/// earlier claim), so it always wins.
#[test]
fn a_claim_never_overwrites_a_known_volume_and_reports_no_change() {
    let mut store = FavoritesStore::default();
    let id = add_to_store(&mut store, "/Volumes/naspi/docs", None, Some(share("/Volumes/naspi")));
    let boot = FavoriteVolume {
        id: "root".to_string(),
        root: "/".to_string(),
        name: "Macintosh HD".to_string(),
    };
    assert!(!claim_in_store(
        &mut store,
        &[(id, boot), ("gone".to_string(), share("/x"))]
    ));
    assert_eq!(store.favorites[0].volume, Some(share("/Volumes/naspi")));
}

/// A claim is bookkeeping, ❌ not a user gesture: it reports nothing to analytics.
#[test]
fn a_claim_is_not_an_analytics_event() {
    assert_eq!(StoreChange::Claimed.analytics_action(), None);
    assert_eq!(
        StoreChange::Gesture(FavoriteAction::Added).analytics_action(),
        Some(FavoriteAction::Added)
    );
}
