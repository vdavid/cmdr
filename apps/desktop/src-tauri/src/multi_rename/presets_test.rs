//! Presets: renaming and updating one in place, and that both survive a restart.

use super::plan::MultiRenameSpec;
use super::presets::{MultiRenamePreset, rename_in, update_spec_in};
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
        counter_start: 1,
        counter_step: 1,
        counter_digits: 1,
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
fn a_rename_persists_as_one_write() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("multi-rename-presets.json");

    let writer: RecentsFile<MultiRenamePreset> = RecentsFile::new();
    writer.add_at(Some(&path), preset("a", "Photos"), 10);
    writer.add_at(Some(&path), preset("b", "Music"), 10);
    writer.edit_at(Some(&path), "a rename", |list| rename_in(list, "a", "Music"));

    let reader: RecentsFile<MultiRenamePreset> = RecentsFile::new();
    reader.load_at(&path);
    let reloaded = reader.entries(None);
    assert_eq!(names(&reloaded), ["Music"]);
    assert_eq!(reloaded[0].id, "a");
}
