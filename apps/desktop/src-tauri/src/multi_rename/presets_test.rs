//! Presets: one list in the user's order (saving appends, renaming and updating
//! keep a preset's place), that it survives a restart; the last settings the sheet
//! closed with.

use super::plan::MultiRenameSpec;
use super::preset_store::PresetStore;
use super::presets::{
    LastSpec, MultiRenamePreset, PresetList, PresetMove, PresetSeed, delete_in, move_in, rename_in, save_in, seed_in,
    update_spec_in,
};
use super::transform::CaseChange;
use crate::recents::RecentsFile;

fn spec(name_mask: &str) -> MultiRenameSpec {
    MultiRenameSpec {
        name_mask: name_mask.to_string(),
        extension_mask: "[E]".to_string(),
        search: String::new(),
        replace: String::new(),
        case_sensitive: false,
        first_only: false,
        include_extension: false,
        regex: false,
        substitute: false,
        case: CaseChange::Unchanged,
        remove_diacritics: false,
        greek_to_latin: false,
        normalize_unicode: false,
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

#[test]
fn a_rename_keeps_the_preset_where_it_stands() {
    let mut list = vec![preset("a", "Photos"), preset("b", "Music"), preset("c", "Docs")];
    assert!(rename_in(&mut list, "b", "  Songs "));
    assert_eq!(names(&list), ["Photos", "Songs", "Docs"]);
    assert_eq!(list[1].id, "b");
    assert_eq!(list[1].spec, spec("[N]-b"), "a rename keeps the settings");
}

#[test]
fn a_rename_onto_a_taken_name_replaces_that_preset() {
    let mut list = vec![preset("a", "Photos"), preset("b", "Music"), preset("c", "Docs")];
    assert!(rename_in(&mut list, "c", "photos"));
    assert_eq!(names(&list), ["Music", "photos"]);
    assert_eq!(list[1].id, "c");
}

#[test]
fn a_rename_to_its_own_name_in_another_case_drops_nothing() {
    let mut list = vec![preset("a", "Photos"), preset("b", "Music")];
    assert!(rename_in(&mut list, "a", "PHOTOS"));
    assert_eq!(names(&list), ["PHOTOS", "Music"]);
}

#[test]
fn a_rename_of_an_unknown_preset_or_to_an_empty_name_changes_nothing() {
    let mut list = vec![preset("a", "Photos"), preset("b", "Music")];
    assert!(!rename_in(&mut list, "gone", "Songs"));
    assert!(!rename_in(&mut list, "a", "   "));
    assert!(!rename_in(&mut list, "a", "Photos"), "the same name is no change");
    assert_eq!(names(&list), ["Photos", "Music"]);
}

#[test]
fn an_update_keeps_the_preset_where_it_stands() {
    let mut list = vec![preset("a", "Photos"), preset("b", "Music")];
    assert!(update_spec_in(&mut list, "b", &spec("[C]")));
    assert_eq!(names(&list), ["Photos", "Music"]);
    assert_eq!(list[1].spec, spec("[C]"));
    assert!(
        !update_spec_in(&mut list, "b", &spec("[C]")),
        "the same settings are no change"
    );
    assert!(!update_spec_in(&mut list, "gone", &spec("[C]")));
}

#[test]
fn a_new_preset_goes_at_the_end_so_no_number_moves() {
    let mut list = vec![preset("a", "Photos"), preset("b", "Music")];
    assert!(save_in(&mut list, preset("c", "Docs"), 10));
    assert_eq!(names(&list), ["Photos", "Music", "Docs"]);
}

#[test]
fn saving_over_a_taken_name_replaces_that_preset_where_it_stands() {
    let mut list = vec![preset("a", "Photos"), preset("b", "Music"), preset("c", "Docs")];
    let mut again = preset("b", " music ");
    again.spec = spec("[C]");
    assert!(save_in(&mut list, again, 10));
    assert_eq!(names(&list), ["Photos", " music ", "Docs"]);
    assert_eq!(list[1].spec, spec("[C]"));
    assert_eq!(list[1].id, "b");
}

#[test]
fn saving_over_a_taken_name_under_a_new_id_keeps_the_place_and_takes_the_id() {
    let mut list = vec![preset("a", "Photos"), preset("b", "Music")];
    assert!(save_in(&mut list, preset("new", "PHOTOS"), 10));
    assert_eq!(names(&list), ["PHOTOS", "Music"]);
    assert_eq!(list[0].id, "new");
}

#[test]
fn a_new_preset_past_the_cap_is_refused_and_nothing_moves() {
    let mut list = vec![preset("a", "Photos"), preset("b", "Music")];
    assert!(!save_in(&mut list, preset("c", "Docs"), 2));
    assert_eq!(names(&list), ["Photos", "Music"]);
    // Replacing one at the cap is fine: the list doesn't grow.
    assert!(save_in(&mut list, preset("b", "Music"), 2));
}

#[test]
fn a_new_preset_with_an_id_the_list_holds_gets_a_fresh_one() {
    let mut list = vec![preset("a", "Photos")];
    assert!(save_in(&mut list, preset("a", "Music"), 10));
    assert_eq!(names(&list), ["Photos", "Music"]);
    assert_ne!(list[1].id, "a");
}

#[test]
fn a_delete_drops_one_preset_and_the_rest_keep_their_order() {
    let mut list = vec![preset("a", "Photos"), preset("b", "Music"), preset("c", "Docs")];
    assert!(delete_in(&mut list, "b"));
    assert_eq!(names(&list), ["Photos", "Docs"]);
    assert!(!delete_in(&mut list, "b"), "a second delete finds nothing");
}

#[test]
fn moving_a_preset_swaps_it_with_its_neighbor() {
    let mut list = vec![preset("a", "Photos"), preset("b", "Music"), preset("c", "Docs")];
    assert!(move_in(&mut list, "c", PresetMove::Up));
    assert_eq!(names(&list), ["Photos", "Docs", "Music"]);
    assert!(move_in(&mut list, "a", PresetMove::Down));
    assert_eq!(names(&list), ["Docs", "Photos", "Music"]);
}

#[test]
fn moving_past_either_end_or_an_unknown_preset_changes_nothing() {
    let mut list = vec![preset("a", "Photos"), preset("b", "Music")];
    assert!(!move_in(&mut list, "a", PresetMove::Up));
    assert!(!move_in(&mut list, "b", PresetMove::Down));
    assert!(!move_in(&mut list, "gone", PresetMove::Up));
    assert_eq!(names(&list), ["Photos", "Music"]);
}

#[test]
fn the_order_survives_a_restart() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("multi-rename-presets.json");

    let writer = PresetStore::new();
    writer.edit_at(Some(&path), "a save", |list| save_in(list, preset("a", "Photos"), 10));
    writer.edit_at(Some(&path), "a save", |list| save_in(list, preset("b", "Music"), 10));
    let answer = writer.edit_at(Some(&path), "a save", |list| save_in(list, preset("c", "Docs"), 10));
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
    writer.edit_at(Some(&path), "a save", |list| save_in(list, preset("a", "Photos"), 10));
    writer.edit_at(Some(&path), "a save", |list| save_in(list, preset("b", "Music"), 10));
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
    store.edit_at(Some(&path), "a save", |list| save_in(list, preset("a", "Photos"), 10));
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

/// A preset as an older Cmdr saved it, with the sheet-wide counter fields.
fn legacy(name_mask: &str, extension_mask: &str, start: i64, step: i64, digits: u32) -> MultiRenamePreset {
    serde_json::from_value(serde_json::json!({
        "id": "old",
        "name": "Old",
        "spec": {
            "nameMask": name_mask,
            "extensionMask": extension_mask,
            "search": "",
            "replace": "",
            "caseSensitive": false,
            "firstOnly": false,
            "includeExtension": false,
            "regex": false,
            "substitute": false,
            "case": "unchanged",
            "removeDiacritics": false,
            "counterStart": start,
            "counterStep": step,
            "counterDigits": digits,
        },
    }))
    .expect("a legacy preset loads")
}

fn masks(preset: &MultiRenamePreset) -> (&str, &str) {
    (&preset.spec.name_mask, &preset.spec.extension_mask)
}

#[test]
fn a_legacy_counter_moves_into_every_bare_c_of_both_masks() {
    let p = legacy("[N]_[C] ([C])", "[E][C]", 10, 5, 3);
    assert_eq!(masks(&p), ("[N]_[C10+5:3] ([C10+5:3])", "[E][C10+5:3]"));
}

#[test]
fn a_legacy_counter_carries_only_its_non_default_parts() {
    assert_eq!(masks(&legacy("[C]", "[E]", 1, 1, 3)), ("[C:3]", "[E]"));
    assert_eq!(masks(&legacy("[C]", "[E]", 10, 1, 1)), ("[C10]", "[E]"));
    assert_eq!(masks(&legacy("[C]", "[E]", 1, -2, 1)), ("[C-2]", "[E]"));
    assert_eq!(masks(&legacy("[C]", "[E]", 1, 0, 1)), ("[C+0]", "[E]"));
    assert_eq!(masks(&legacy("[C]", "[E]", 100, -10, 1)), ("[C100-10]", "[E]"));
}

#[test]
fn a_legacy_negative_start_keeps_its_step_explicit() {
    // A lone leading sign is the step (`[C-5]` counts down by five), so a
    // negative start always says its step too.
    assert_eq!(masks(&legacy("[C]", "[E]", -5, 1, 1)), ("[C-5+1]", "[E]"));
    assert_eq!(masks(&legacy("[C]", "[E]", -5, -2, 2)), ("[C-5-2:2]", "[E]"));
}

#[test]
fn a_legacy_default_counter_changes_nothing() {
    let p = legacy("[N]_[C]", "[E]", 1, 1, 1);
    assert_eq!(masks(&p), ("[N]_[C]", "[E]"));
}

#[test]
fn a_legacy_inline_counter_keeps_its_own_parts_and_takes_the_rest() {
    // `[C:4]` used to take the sheet's start: it keeps counting from there.
    let p = legacy("[C2+3:4]-[C:4]-[C20]", "[E]", 10, 1, 3);
    assert_eq!(masks(&p), ("[C2+3:4]-[C10:4]-[C20:3]", "[E]"));
}

#[test]
fn a_legacy_migration_leaves_literals_and_other_placeholders_alone() {
    let p = legacy("[[C]-[N]-[N2-5]-[CX", "[E]", 10, 1, 1);
    assert_eq!(masks(&p), ("[[C]-[N]-[N2-5]-[CX", "[E]"));
}

#[test]
fn a_legacy_digits_width_over_the_cap_renders_the_same() {
    // The old sheet clamped digits to 1..=64; zero meant one.
    assert_eq!(masks(&legacy("[C]", "[E]", 1, 1, 0)), ("[C]", "[E]"));
    assert_eq!(masks(&legacy("[C]", "[E]", 1, 1, 4_000_000_000)), ("[C:64]", "[E]"));
}

#[test]
fn a_saved_preset_writes_no_legacy_counter_fields() {
    let p = legacy("[C]", "[E]", 10, 1, 1);
    let json = serde_json::to_value(&p).expect("serializes");
    let spec = json["spec"].as_object().expect("a spec object");
    assert!(!spec.contains_key("counterStart"));
    assert!(!spec.contains_key("counterStep"));
    assert!(!spec.contains_key("counterDigits"));
    let again: MultiRenamePreset = serde_json::from_value(json).expect("reloads");
    assert_eq!(again, p, "a migrated preset reloads unchanged");
}

#[test]
fn a_legacy_presets_file_loads_migrated() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("multi-rename-presets.json");
    let writer = PresetStore::new();
    writer.edit_at(Some(&path), "a save", |list| save_in(list, preset("a", "Photos"), 10));
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
fn a_preset_saved_before_greek_to_latin_and_normalize_loads_with_both_off() {
    let p = legacy("[N]", "[E]", 1, 1, 1);
    assert!(!p.spec.greek_to_latin);
    assert!(!p.spec.normalize_unicode);
}

#[test]
fn the_last_settings_keep_one_entry_and_survive_a_restart() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("multi-rename-last.json");

    let writer: RecentsFile<LastSpec> = RecentsFile::new();
    writer.add_at(Some(&path), LastSpec::new(spec("[N]-one"), None), 1);
    let loaded = Some("p1".to_string());
    writer.add_at(Some(&path), LastSpec::new(spec("[N]-two"), loaded.clone()), 1);

    let reader: RecentsFile<LastSpec> = RecentsFile::new();
    reader.load_at(&path);
    let entries = reader.entries(None);
    assert_eq!(entries.len(), 1, "saving replaces the one entry");
    assert_eq!(entries[0].settings.spec, spec("[N]-two"));
    assert_eq!(entries[0].settings.preset_id, loaded);
}

#[test]
fn last_settings_saved_without_a_preset_or_the_newer_options_load() {
    let last: LastSpec = serde_json::from_value(serde_json::json!({
        "id": "last",
        "spec": {
            "nameMask": "[N]-old",
            "extensionMask": "[E]",
            "search": "",
            "replace": "",
            "caseSensitive": false,
            "firstOnly": false,
            "includeExtension": false,
            "regex": false,
            "substitute": false,
            "case": "unchanged",
            "removeDiacritics": false,
        },
    }))
    .expect("an older save loads");
    assert_eq!(last.settings.spec.name_mask, "[N]-old");
    assert!(!last.settings.spec.normalize_unicode);
    assert_eq!(last.settings.preset_id, None);
}

/// Last settings as a build saved them while built-in presets were their own kind.
fn last_with_preset(preset: serde_json::Value) -> LastSpec {
    let mut stored = serde_json::to_value(LastSpec::new(spec("[N]-old"), None)).expect("serializes");
    stored["preset"] = preset;
    serde_json::from_value(stored).expect("an older save loads")
}

#[test]
fn older_last_settings_keep_a_saved_preset_and_drop_a_built_in_one() {
    let saved = last_with_preset(serde_json::json!({ "kind": "saved", "id": "p1" }));
    assert_eq!(saved.settings.preset_id.as_deref(), Some("p1"));

    // The built-ins are seeded presets now, with ids of their own: the fields stay, no preset is named.
    let built_in = last_with_preset(serde_json::json!({ "kind": "builtIn", "id": "builtin:remove-diacritics" }));
    assert_eq!(built_in.settings.preset_id, None);
    assert_eq!(built_in.settings.spec, spec("[N]-old"));
}

// -- Seeds --

fn seeds() -> Vec<PresetSeed> {
    [
        "Date and time",
        "Remove diacritics",
        "Greek to Latin",
        "Normalize Unicode",
    ]
    .into_iter()
    .map(|name| PresetSeed {
        name: name.to_string(),
        spec: spec(&format!("[N]-{name}")),
    })
    .collect()
}

const SEED_NAMES: [&str; 4] = [
    "Date and time",
    "Remove diacritics",
    "Greek to Latin",
    "Normalize Unicode",
];

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
fn a_preset_saved_before_the_seeds_arrive_keeps_number_one() {
    let mut list = PresetList::default();
    assert!(save_in(&mut list.entries, preset("a", "Mine"), 10));
    assert!(seed_in(&mut list, seeds(), 10));
    assert_eq!(names(&list.entries)[..2], ["Mine", "Date and time"]);
}

#[test]
fn seeding_stops_at_the_cap() {
    let mut list = PresetList {
        entries: vec![preset("a", "Mine")],
        seeded: false,
    };
    assert!(seed_in(&mut list, seeds(), 2));
    assert_eq!(names(&list.entries), ["Mine", "Date and time"]);
    assert!(list.seeded);
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
