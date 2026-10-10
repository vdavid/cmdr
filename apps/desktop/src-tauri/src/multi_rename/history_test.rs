//! What a rename adds to the fields' history.

use super::history::{HistoryField, history_entries};
use super::plan::MultiRenameSpec;
use super::transform::CaseChange;

fn spec(name: &str, ext: &str, search: &str, replace: &str) -> MultiRenameSpec {
    MultiRenameSpec {
        name_mask: name.to_string(),
        extension_mask: ext.to_string(),
        search: search.to_string(),
        replace: replace.to_string(),
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

fn fields(s: &MultiRenameSpec) -> Vec<(HistoryField, String)> {
    history_entries(s).into_iter().map(|e| (e.field, e.value)).collect()
}

#[test]
fn every_field_that_did_something_is_kept_with_the_name_mask_last_so_it_lands_on_top() {
    assert_eq!(
        fields(&spec("[N]_[C]", "[E]", "IMG", "foto")),
        vec![
            (HistoryField::Replace, "foto".to_string()),
            (HistoryField::Search, "IMG".to_string()),
            (HistoryField::NameMask, "[N]_[C]".to_string()),
        ]
    );
}

#[test]
fn defaults_and_empty_fields_are_left_out() {
    assert!(fields(&spec("[N]", "[E]", "", "")).is_empty());
    assert_eq!(
        fields(&spec("[N]", "jpg", "", "")),
        vec![(HistoryField::ExtensionMask, "jpg".to_string())]
    );
}

#[test]
fn a_replacement_counts_only_beside_a_search() {
    assert!(fields(&spec("[N]", "[E]", "", "unused")).is_empty());
    // Replacing with nothing (deleting the match) is a real use of the search.
    assert_eq!(
        fields(&spec("[N]", "[E]", "copy", "")),
        vec![(HistoryField::Search, "copy".to_string())]
    );
}

#[test]
fn each_entry_gets_its_own_id() {
    let entries = history_entries(&spec("[N]x", "[E]", "a", "b"));
    let mut ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    ids.dedup();
    assert_eq!(ids.len(), 3);
}
