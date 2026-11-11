//! Which rules a spec reports to analytics.

use super::plan::MultiRenameSpec;
use super::rule_kinds::{RuleKind, rule_kinds};
use super::transform::CaseChange;

fn plain() -> MultiRenameSpec {
    MultiRenameSpec {
        name_mask: "[N]".to_string(),
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

#[test]
fn the_no_change_defaults_use_no_rule() {
    assert!(rule_kinds(&plain()).is_empty());
}

#[test]
fn each_rule_reads_off_its_own_field_and_the_masks_placeholders() {
    let spec = MultiRenameSpec {
        name_mask: "[N]_[C]_[Y-M-D]".to_string(),
        extension_mask: "jpg".to_string(),
        search: "IMG".to_string(),
        case: CaseChange::Lower,
        greek_to_latin: true,
        remove_diacritics: true,
        normalize_unicode: true,
        ..plain()
    };
    assert_eq!(
        rule_kinds(&spec),
        vec![
            RuleKind::NameMask,
            RuleKind::ExtMask,
            RuleKind::Counter,
            RuleKind::Date,
            RuleKind::SearchReplace,
            RuleKind::Case,
            RuleKind::GreekToLatin,
            RuleKind::RemoveDiacritics,
            RuleKind::NormalizeUnicode,
        ]
    );
}

#[test]
fn a_media_placeholder_counts_as_media_meta() {
    let spec = MultiRenameSpec {
        name_mask: "[=exif.taken]".to_string(),
        ..plain()
    };
    assert!(rule_kinds(&spec).contains(&RuleKind::MediaMeta));
}

#[test]
fn the_tokens_are_pinned() {
    assert_eq!(RuleKind::NameMask.token(), "name_mask");
    assert_eq!(RuleKind::ExtMask.token(), "ext_mask");
    assert_eq!(RuleKind::SearchReplace.token(), "search_replace");
    assert_eq!(RuleKind::MediaMeta.token(), "media_meta");
}
