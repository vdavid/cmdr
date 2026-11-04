//! The search core's diagnostic summary, as a crash or error report sees it: the host
//! owns the redaction, so this is where the two meet.

use cmdr_search::query::summarize_query_for_diagnostics;
use cmdr_search::types::{PatternType, SearchQuery};

/// The summary's scope roots and exclusions are keyed fields a report tokenizes, so a
/// report keeps their shape (and the Downloads role) and none of their words.
#[test]
fn a_report_tokenizes_the_summarys_scope_and_exclusions() {
    let q = SearchQuery {
        name_pattern: Some("Q\u{1f512}.*private-pattern".to_string()),
        pattern_type: PatternType::Regex,
        min_size: Some(1_024),
        max_size: Some(8_192),
        modified_after: Some(1_735_689_600),
        modified_before: Some(1_772_323_200),
        is_directory: Some(false),
        include_paths: Some(vec![
            "/Users/private-person/Downloads/secret scope a".to_string(),
            "/Volumes/private-drive/secret-scope-b".to_string(),
        ]),
        exclude_dir_names: Some(vec![
            "private-exclusion-a".to_string(),
            "private-exclusion-b".to_string(),
        ]),
        include_path_ids: None,
        count_only: true,
        limit: 30,
        case_sensitive: Some(false),
        exclude_system_dirs: Some(false),
        sort_by: None,
    };
    let summary = summarize_query_for_diagnostics(&q);

    // A report tokenizes every root and exclusion, and keeps the Downloads role.
    let report = crate::redact::RedactionContext::for_test([0x11; 32], "ERR-SCOPE")
        .redact_line(&summary)
        .into_owned();
    for private in [
        "private-person",
        "secret scope",
        "private-drive",
        "secret-scope-b",
        "private-exclusion",
    ] {
        assert!(!report.contains(private), "report leaked {private:?}: {report}");
    }
    assert!(
        report.contains("scope=roots(2) [path=\"$HOME/Downloads/<dir:"),
        "{report}"
    );
    assert!(report.contains("path=\"/Volumes/<volume:"), "{report}");
    assert!(report.contains("exclusions=2 [dir=\"<dir:"), "{report}");
}
