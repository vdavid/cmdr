//! `preview_of`: the rows the expanded line shows, and the truncation note's numbers.

use serde_json::{Value, json};

use super::fixtures;
use super::result;
use crate::agent::llm::types::{AgentToolResult, ToolId};
use crate::agent::tools::line::{
    MAX_TEXT_CHARS, PREVIEW_ROWS, ToolPreview, ToolPreviewRow, ToolPreviewRowKind, ToolPreviewRowStatus, preview_of,
};
use crate::agent::tools::read::pane_listing::PaneListingScope;

fn preview(tool: ToolId, content: Value) -> ToolPreview {
    preview_of(&tool, &result(content))
}

fn names(preview: &ToolPreview) -> Vec<&str> {
    preview.rows.iter().map(|row| row.name.as_str()).collect()
}

#[test]
fn a_listing_shows_its_rows_with_paths_joined_onto_the_folder() {
    let shown = preview(ToolId::ListDir, fixtures::list_dir());
    assert_eq!(
        shown.rows[0],
        ToolPreviewRow {
            name: "big.iso".into(),
            path: Some("/Users/u/Downloads/big.iso".into()),
            kind: ToolPreviewRowKind::File,
            size_bytes: Some(9_000_000_000),
            line: None,
            snippet: None,
            status: None,
        }
    );
    assert_eq!(shown.rows[1].kind, ToolPreviewRowKind::Folder);
    assert_eq!(
        (shown.rows_given_to_model, shown.total, shown.lower_bound),
        (Some(2), Some(5), false)
    );
    assert_eq!((shown.problem.as_deref(), shown.set_aside), (None, false));

    let pane = preview(
        ToolId::ListPaneFiles,
        fixtures::pane_listing(PaneListingScope::Folder, &["a.jpg", "Sub/"], 2),
    );
    assert_eq!(pane.rows[0].path.as_deref(), Some("/Users/u/Photos/a.jpg"));
    assert_eq!(pane.rows[1].kind, ToolPreviewRowKind::Folder);

    // A thread stored before results carried `location` named its folder as a bare `path`.
    let mut legacy = fixtures::list_dir();
    legacy["path"] = legacy["location"]["path"].take();
    legacy.as_object_mut().expect("an object").remove("location");
    let old = preview(ToolId::ListDir, legacy);
    assert_eq!(old.rows[0].path.as_deref(), Some("/Users/u/Downloads/big.iso"));
}

#[test]
fn a_location_miss_shows_no_rows_and_no_counts() {
    for tool in [
        ToolId::ListDir,
        ToolId::ImportantFolders,
        ToolId::Search,
        ToolId::InspectFile,
    ] {
        let shown = preview(tool.clone(), fixtures::location_miss());
        assert!(shown.rows.is_empty(), "{tool:?}");
        assert_eq!((shown.rows_given_to_model, shown.total), (None, None), "{tool:?}");
        assert_eq!(shown.problem, None, "a miss is an answer, not a refusal: {tool:?}");
    }
}

#[test]
fn the_preview_caps_its_rows_but_reports_what_the_model_got() {
    let names: Vec<String> = (0..200).map(|i| format!("file-{i}")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let shown = preview(
        ToolId::ListPaneFiles,
        fixtures::pane_listing(PaneListingScope::Folder, &refs, 4_812),
    );
    assert_eq!(shown.rows.len(), PREVIEW_ROWS);
    assert_eq!(shown.rows_given_to_model, Some(200), "showed the model 200…");
    assert_eq!(shown.total, Some(4_812), "…of 4,812");
}

#[test]
fn a_search_reports_its_match_count_and_lower_bound() {
    let shown = preview(ToolId::Search, fixtures::search(4_812, 30, false, true));
    assert_eq!(shown.rows.len(), PREVIEW_ROWS);
    assert_eq!(shown.rows[0].path.as_deref(), Some("/Users/u/Docs/report-0.pdf"));
    assert_eq!(shown.rows[0].size_bytes, Some(2_048));
    assert_eq!(
        (shown.rows_given_to_model, shown.total, shown.lower_bound),
        (Some(30), Some(4_812), true)
    );

    let count_only = preview(ToolId::Search, fixtures::search(75, 0, true, false));
    assert!(count_only.rows.is_empty(), "countOnly: no rows, real counts");
    assert_eq!(count_only.total, Some(75));

    let none_yet = preview(ToolId::Search, fixtures::search(0, 0, false, false));
    assert_eq!((none_yet.total, none_yet.lower_bound), (Some(0), true));
}

#[test]
fn inspect_shows_a_row_per_find_hit_and_each_paths_status() {
    let shown = preview(ToolId::InspectFile, fixtures::inspect_with_find());
    assert_eq!(
        names(&shown),
        vec!["notes.txt", "notes.txt", "gone.txt", "locked.txt", "id_rsa", "tax.pdf"]
    );
    assert_eq!(
        (shown.rows[0].line, shown.rows[0].snippet.as_deref()),
        (Some(12), Some("invoice 2026"))
    );
    assert_eq!(shown.rows[1].line, Some(812));
    assert_eq!(shown.rows[0].size_bytes, Some(1_234));
    assert_eq!(shown.rows[2].status, Some(ToolPreviewRowStatus::Missing));
    assert_eq!(shown.rows[2].path.as_deref(), Some("/Users/u/gone.txt"));
    assert_eq!(shown.rows[3].status, Some(ToolPreviewRowStatus::Unreadable));
    assert_eq!(shown.rows[4].status, Some(ToolPreviewRowStatus::Protected));
    assert_eq!(shown.rows[5].status, Some(ToolPreviewRowStatus::NoVolumeAtPath));
    assert_eq!(shown.rows[5].path.as_deref(), Some("/Volumes/naspi/papers/tax.pdf"));
    assert_eq!((shown.rows_given_to_model, shown.total), (Some(5), Some(6)));
}

#[test]
fn photo_tools_show_their_recognized_text_and_not_indexed_paths() {
    let hits = preview(ToolId::SearchPhotos, fixtures::search_photos(40, false));
    assert_eq!(hits.rows[0].name, "IMG_0.jpg");
    assert_eq!(hits.rows[0].snippet.as_deref(), Some("RECEIPT"));
    assert_eq!(hits.total, Some(40));

    let facts = preview(ToolId::ImageFacts, fixtures::image_facts());
    assert_eq!(facts.rows[0].snippet.as_deref(), Some("TOTAL 42.00"));
    assert_eq!(facts.rows[0].status, None);
    assert_eq!(facts.rows[1].status, Some(ToolPreviewRowStatus::NotIndexed));
    assert_eq!(facts.rows[2].status, Some(ToolPreviewRowStatus::Protected));
}

#[test]
fn the_other_lists_show_their_rows() {
    assert_eq!(
        names(&preview(
            ToolId::ImportantFolders,
            fixtures::important_folders(&["/Users/u/Code"], false)
        )),
        vec!["Code"]
    );
    let drives = preview(ToolId::ListVolumes, fixtures::list_volumes());
    assert_eq!(names(&drives), vec!["Macintosh HD", "Pixel"]);
    assert_eq!(drives.rows[0].path.as_deref(), Some("/"));
    assert_eq!(drives.rows[1].path, None, "a phone has no mount path");
    assert_eq!(
        names(&preview(ToolId::ListSuggestions, fixtures::list_suggestions())),
        vec!["Old installers"]
    );
    assert_eq!(
        names(&preview(ToolId::GetSuggestionGroup, fixtures::suggestion_group())),
        vec!["setup.dmg"]
    );
    let operations = preview(ToolId::OperationsList, fixtures::operations_list());
    assert!(operations.rows.is_empty(), "an operation row has no name to show");
    assert_eq!((operations.rows_given_to_model, operations.total), (Some(1), Some(3)));
}

#[test]
fn a_tool_with_no_list_shows_no_rows_and_no_counts() {
    for (tool, content) in [
        (ToolId::AppState, json!({ "focusedPane": "left" })),
        (ToolId::MemoryWrite, fixtures::memory_saved()),
        (ToolId::ProposeRenamePlan, json!({ "readyForReview": true, "count": 3 })),
        (
            ToolId::ConnectVolume,
            fixtures::connect(crate::agent::tools::connect::ConnectOutcome::Connected),
        ),
    ] {
        let shown = preview(tool.clone(), content);
        assert!(shown.rows.is_empty(), "{tool:?}");
        assert_eq!((shown.rows_given_to_model, shown.total), (None, None), "{tool:?}");
    }
}

#[test]
fn a_refusal_shows_its_problem_verbatim() {
    let shown = preview(ToolId::ListDir, fixtures::problem());
    assert_eq!(
        shown.problem.as_deref(),
        Some("list_dir needs path. It takes limit and path.")
    );
    assert!(shown.rows.is_empty());
    let staged_nothing = preview(
        ToolId::ProposeRenamePlan,
        json!({ "readyForReview": false, "problem": "renames needs at least one row." }),
    );
    assert_eq!(
        staged_nothing.problem.as_deref(),
        Some("renames needs at least one row.")
    );
}

#[test]
fn a_set_aside_row_says_so_and_shows_nothing_of_its_stub() {
    let stub = AgentToolResult {
        call_id: "call-1".into(),
        content: json!({ "elided_tool_result": true, "children": [{ "name": "stub" }] }),
        elided: true,
    };
    let shown = preview_of(&ToolId::ListDir, &stub);
    assert!(shown.set_aside);
    assert!(shown.rows.is_empty());
    assert_eq!(shown.rows_given_to_model, None);
}

#[test]
fn rows_are_bounded_and_odd_shapes_never_panic() {
    let long = "n".repeat(MAX_TEXT_CHARS * 3);
    let shown = preview(
        ToolId::Search,
        json!({ "matchCount": 1, "returned": 1, "coverage": { "complete": true },
                "entries": [{ "name": long, "path": 7 }, "not an object", { "path": "/no-name" }] }),
    );
    assert_eq!(shown.rows.len(), 1, "rows missing their name are skipped");
    assert!(shown.rows[0].name.chars().count() <= MAX_TEXT_CHARS + 1);
    assert_eq!(shown.rows[0].path, None);

    for tool in ToolId::KNOWN {
        for content in [json!(null), json!([]), json!({ "files": "x", "children": 3 })] {
            let _ = preview(tool.clone(), content);
        }
    }
}
