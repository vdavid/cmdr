//! The presets file: the list's order survives a restart, an edit that changes nothing
//! writes nothing, an unreadable or older file loads, and the seeds go in once per file.
//! The list ops themselves are `cmdr_multi_rename::presets`, tested there.

use super::plan::MultiRenameSpec;
use super::preset_store::PresetStore;
use super::presets::{MultiRenamePreset, PresetSeed, delete_in, rename_in, save_in};
use super::run_test::strip_diacritics;

fn spec(name_mask: &str) -> MultiRenameSpec {
    MultiRenameSpec {
        name_mask: name_mask.to_string(),
        remove_diacritics: false,
        ..strip_diacritics()
    }
}

fn preset(id: &str, name: &str) -> MultiRenamePreset {
    MultiRenamePreset {
        id: id.to_string(),
        name: name.to_string(),
        spec: spec(&format!("[N]-{id}")),
    }
}

fn names(presets: &[MultiRenamePreset]) -> Vec<&str> {
    presets.iter().map(|p| p.name.as_str()).collect()
}

const SEED_NAMES: [&str; 4] = [
    "Date and time",
    "Remove diacritics",
    "Greek to Latin",
    "Normalize Unicode",
];

fn seeds() -> Vec<PresetSeed> {
    SEED_NAMES
        .into_iter()
        .map(|name| PresetSeed {
            name: name.to_string(),
            spec: spec(&format!("[N]-{name}")),
        })
        .collect()
}

#[test]
fn the_order_survives_a_restart() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("multi-rename-presets.json");

    let writer = PresetStore::new();
    writer.edit_at(Some(&path), "a save", |list| {
        save_in(list, preset("a", "Photos"), 10).is_ok()
    });
    writer.edit_at(Some(&path), "a save", |list| {
        save_in(list, preset("b", "Music"), 10).is_ok()
    });
    let answer = writer.edit_at(Some(&path), "a save", |list| {
        save_in(list, preset("c", "Docs"), 10).is_ok()
    });
    assert_eq!(names(&answer), ["Photos", "Music", "Docs"], "an edit answers the list");

    let reader = PresetStore::new();
    reader.load_at(&path);
    assert_eq!(names(&reader.entries()), ["Photos", "Music", "Docs"]);
}

#[test]
fn a_rename_persists_as_one_write() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("multi-rename-presets.json");

    let writer = PresetStore::new();
    writer.edit_at(Some(&path), "a save", |list| {
        save_in(list, preset("a", "Photos"), 10).is_ok()
    });
    writer.edit_at(Some(&path), "a save", |list| {
        save_in(list, preset("b", "Music"), 10).is_ok()
    });
    writer.edit_at(Some(&path), "a rename", |list| rename_in(list, "a", "Music"));

    let reader = PresetStore::new();
    reader.load_at(&path);
    let reloaded = reader.entries();
    assert_eq!(names(&reloaded), ["Music"]);
    assert_eq!(reloaded[0].id, "a");
}

#[test]
fn an_edit_that_changes_nothing_leaves_the_file_untouched() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("multi-rename-presets.json");
    let store = PresetStore::new();
    store.edit_at(Some(&path), "a save", |list| {
        save_in(list, preset("a", "Photos"), 10).is_ok()
    });
    std::fs::remove_file(&path).expect("remove");

    store.edit_at(Some(&path), "a delete", |list| delete_in(list, "gone"));

    assert!(!path.exists(), "a no-op edit shouldn't rewrite the file");
}

#[test]
fn an_unreadable_presets_file_is_set_aside_and_the_list_starts_empty() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("multi-rename-presets.json");
    std::fs::write(&path, "{not json").expect("write garbage");

    let notices = crate::state_recovery::testing::capture_recoveries();
    let store = PresetStore::new();
    store.load_at(&path);
    assert!(store.entries().is_empty());
    assert_eq!(crate::state_recovery::testing::kept_copies(&path).len(), 1);
    assert_eq!(notices.recoveries().len(), 1);
}

#[test]
fn an_older_build_s_presets_file_loads_in_its_stored_order() {
    // Before presets had their own order, the file was a recents list, newest first.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("multi-rename-presets.json");
    let entries = serde_json::to_value([preset("b", "Music"), preset("a", "Photos")]).expect("serializes");
    let older = serde_json::json!({ "_schemaVersion": 1, "entries": entries });
    std::fs::write(&path, older.to_string()).expect("write");

    let store = PresetStore::new();
    store.load_at(&path);
    assert_eq!(names(&store.entries()), ["Music", "Photos"]);
}

#[test]
fn a_legacy_presets_file_loads_migrated() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("multi-rename-presets.json");
    let writer = PresetStore::new();
    writer.edit_at(Some(&path), "a save", |list| {
        save_in(list, preset("a", "Photos"), 10).is_ok()
    });
    // Turn the stored preset into an older Cmdr's: bare `[C]`, sheet digits 3.
    let mut stored: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
    let entry = &mut stored["entries"][0];
    entry["spec"]["nameMask"] = "[N]-[C]".into();
    entry["spec"]["counterStart"] = 1.into();
    entry["spec"]["counterStep"] = 1.into();
    entry["spec"]["counterDigits"] = 3.into();
    std::fs::write(&path, stored.to_string()).expect("write");

    let reader = PresetStore::new();
    reader.load_at(&path);
    assert_eq!(reader.entries()[0].spec.name_mask, "[N]-[C:3]");
}

#[test]
fn a_new_install_starts_with_the_seeds_in_order() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("multi-rename-presets.json");
    let store = PresetStore::new();
    store.load_at(&path);

    let answer = store.seed_at(Some(&path), seeds(), 10);

    assert_eq!(names(&answer), SEED_NAMES);
    assert_eq!(answer[0].spec, spec("[N]-Date and time"));
    let ids: std::collections::HashSet<&str> = answer.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids.len(), 4, "every seed gets its own id");
    assert!(path.exists(), "seeding writes the file, so it happens once");
}

#[test]
fn seeds_go_in_once_so_a_deleted_one_never_comes_back() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("multi-rename-presets.json");
    let store = PresetStore::new();
    store.load_at(&path);
    let seeded = store.seed_at(Some(&path), seeds(), 10);
    store.edit_at(Some(&path), "a delete", |list| delete_in(list, &seeded[1].id));

    let after_restart = PresetStore::new();
    after_restart.load_at(&path);
    let answer = after_restart.seed_at(Some(&path), seeds(), 10);
    assert_eq!(names(&answer), ["Date and time", "Greek to Latin", "Normalize Unicode"]);
}

#[test]
fn an_emptied_list_is_never_reseeded() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("multi-rename-presets.json");
    let store = PresetStore::new();
    store.load_at(&path);
    for preset in store.seed_at(Some(&path), seeds(), 10) {
        store.edit_at(Some(&path), "a delete", |list| delete_in(list, &preset.id));
    }

    let after_restart = PresetStore::new();
    after_restart.load_at(&path);
    assert!(after_restart.seed_at(Some(&path), seeds(), 10).is_empty());
}

#[test]
fn an_older_build_s_presets_get_the_seeds_after_them_once_skipping_names_they_have() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("multi-rename-presets.json");
    let entries = serde_json::to_value([preset("b", "Mine"), preset("a", " remove DIACRITICS")]).expect("serializes");
    let older = serde_json::json!({ "_schemaVersion": 1, "entries": entries });
    std::fs::write(&path, older.to_string()).expect("write");

    let store = PresetStore::new();
    store.load_at(&path);
    let answer = store.seed_at(Some(&path), seeds(), 10);
    assert_eq!(
        names(&answer),
        [
            "Mine",
            " remove DIACRITICS",
            "Date and time",
            "Greek to Latin",
            "Normalize Unicode"
        ],
        "their numbers stay, and a name they have isn't added twice"
    );
    assert_eq!(answer[1].spec, spec("[N]-a"), "their own preset keeps its settings");

    let after_restart = PresetStore::new();
    after_restart.load_at(&path);
    assert_eq!(after_restart.seed_at(Some(&path), seeds(), 10).len(), 5, "once only");
}

#[test]
fn a_set_aside_presets_file_isnt_seeded() {
    // It wasn't a new install: the person's list is in the set-aside copy.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("multi-rename-presets.json");
    std::fs::write(&path, "{not json").expect("write garbage");
    let _notices = crate::state_recovery::testing::capture_recoveries();

    let store = PresetStore::new();
    store.load_at(&path);
    assert!(store.seed_at(Some(&path), seeds(), 10).is_empty());
}
