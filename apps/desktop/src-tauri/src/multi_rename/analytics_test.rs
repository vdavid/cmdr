//! `multi_rename_applied`'s props, pinned word for word: the dashboard maps them.

use std::sync::Arc;

use serde_json::json;

use super::analytics::{APPLIED_FOR_TESTS, applied_props};
use super::app_host::app_multi_rename;
use super::error::MultiRenameError;
use super::plan::MultiRenameSpec;
use super::run::MultiRenameStarted;
use super::run_test::{folder, strip_diacritics};
use super::transform::CaseChange;
use crate::ignore_poison::IgnorePoison;
use cmdr_operations::testing::CollectorEventSink;

/// The engine's apply exit reaches the app's capture, refusals included, carrying
/// the spec's rule tokens. The rule mix is unique to this test, since the recorder
/// is shared by every apply in the binary.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_apply_reports_multi_rename_applied_from_the_engines_exit() {
    let (_dir, listing) = folder("multi-rename-analytics", &["plain.txt"]);
    let session = app_multi_rename()
        .open(listing.id(), true, None, 0)
        .expect("the session opens");
    let spec = MultiRenameSpec {
        remove_diacritics: false,
        greek_to_latin: true,
        normalize_unicode: true,
        case: CaseChange::Lower,
        ..strip_diacritics()
    };
    let preview = app_multi_rename()
        .preview(&session.session_id, &spec)
        .expect("a preview");

    let outcome = app_multi_rename()
        .apply(
            Arc::new(CollectorEventSink::new()),
            session.session_id.clone(),
            preview.preview_id,
        )
        .await;
    assert!(matches!(outcome, Err(MultiRenameError::NothingToRename)));

    let expected = json!({
        "item_count": crate::analytics::item_count_bucket(0),
        "rules": "case,greek_to_latin,normalize_unicode",
        "outcome": "nothing_to_rename",
    });
    assert!(APPLIED_FOR_TESTS.lock_ignore_poison().contains(&expected));
    app_multi_rename().close(&session.session_id);
}

#[test]
fn a_started_rename_reports_its_rules_count_and_outcome() {
    let started = Ok(MultiRenameStarted {
        operation_id: "op".to_string(),
        renaming: 3,
        swaps_left_out: 0,
    });
    assert_eq!(
        applied_props(&["name_mask", "counter", "search_replace"], &started),
        json!({
            "item_count": crate::analytics::item_count_bucket(3),
            "rules": "name_mask,counter,search_replace",
            "outcome": "started",
        })
    );
}

#[test]
fn a_refusal_reports_its_typed_variant_and_no_rows() {
    let refused = Err(MultiRenameError::PreviewOutOfDate);
    let props = applied_props(&[], &refused);
    assert_eq!(props["outcome"], "preview_out_of_date");
    assert_eq!(props["rules"], "none");
    assert_eq!(props["item_count"], crate::analytics::item_count_bucket(0));
}
