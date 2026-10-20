use super::*;
use crate::agent::store::proposals::{get_group, page_ops};
use crate::agent::store::{MIGRATIONS, run_migrations};
use crate::agent::types::{ProposalStatus, ProposalVerb};

const DIR: &str = "/trips/lisbon";

fn file(name: &str) -> FileEntry {
    FileEntry {
        size: Some(100),
        modified_at: Some(1_781_557_809),
        inode: Some(7),
        ..FileEntry::new(name.to_string(), format!("{DIR}/{name}"), false, false)
    }
}

fn preview_of(params: Value, folder: &[FileEntry], batch: &[&str]) -> Vec<PreviewRow> {
    let request = parse_request(&params).expect("valid params");
    let compiled = Compiled::new(&request.spec).expect("spec runs");
    let names: Vec<String> = batch.iter().map(|s| (*s).to_string()).collect();
    preview_names(&compiled, Path::new(DIR), &names, folder, &NameEdits::default())
}

// ── Params ────────────────────────────────────────────────────────────────────

/// A bare call is a dry run of the sheet's blank settings, which renames nothing.
#[test]
fn a_bare_call_is_a_dry_run_of_the_blank_settings() {
    let request = parse_request(&json!({})).expect("valid");
    assert!(request.dry_run);
    assert_eq!(request.spec.name_mask, "[N]");
    assert_eq!(request.spec.extension_mask, "[E]");
    assert_eq!(
        (request.start, request.limit, request.filter),
        (0, DEFAULT_LIMIT, RowFilter::All)
    );
    let rows = preview_of(json!({}), &[file("a.jpg")], &["a.jpg"]);
    assert_eq!(rows[0].status, RowStatus::Unchanged);
}

/// Every knob the sheet has reaches the spec under its own name.
#[test]
fn every_sheet_knob_is_a_param() {
    let request = parse_request(&json!({
        "nameMask": "[N]_[C]",
        "extensionMask": "[E]",
        "search": "a|b",
        "replace": "x|y",
        "caseSensitive": true,
        "firstOnly": true,
        "includeExtension": true,
        "regex": false,
        "substitute": true,
        "case": "upper",
        "removeDiacritics": true,
        "greekToLatin": true,
        "normalizeUnicode": true,
    }))
    .expect("valid");
    let spec = request.spec;
    assert_eq!(
        (spec.name_mask.as_str(), spec.search.as_str(), spec.replace.as_str()),
        ("[N]_[C]", "a|b", "x|y")
    );
    assert!(spec.case_sensitive && spec.first_only && spec.include_extension && spec.substitute);
    assert!(spec.remove_diacritics && spec.greek_to_latin && spec.normalize_unicode && !spec.regex);
    assert_eq!(spec.case, crate::multi_rename::transform::CaseChange::Upper);
}

#[test]
fn bad_params_are_refused_with_the_field_named() {
    let err = parse_request(&json!({ "case": "sideways" })).expect_err("unknown case");
    assert!(err.message.contains("wrong type"), "{}", err.message);
    parse_request(&json!({ "limit": 0 })).expect_err("zero limit");
    parse_request(&json!({ "limit": MAX_LIMIT + 1 })).expect_err("limit cap");
    parse_request(&json!({ "filter": "ready" })).expect_err("unknown filter");
    parse_request(&json!({ "names": "a.jpg" })).expect_err("names must be an array");
}

/// Paging a proposal would silently propose a slice, so it's refused.
#[test]
fn paging_params_beside_a_proposal_are_refused() {
    parse_request(&json!({ "dryRun": false, "limit": 10 })).expect_err("limit is dry-run only");
    parse_request(&json!({ "dryRun": false, "start": 10 })).expect_err("start is dry-run only");
    assert!(!parse_request(&json!({ "dryRun": false })).expect("valid").dry_run);
}

/// The schema has to declare exactly the knobs the parser reads, or a knob is unreachable
/// (the schema gate refuses undeclared keys).
#[test]
fn the_schema_declares_every_spec_knob() {
    let tool = crate::mcp::tool_registry::get_all_tools()
        .into_iter()
        .find(|t| t.name == "multi_rename")
        .expect("multi_rename is in the ai-client view");
    let props = tool.input_schema["properties"].as_object().expect("properties").clone();
    for knob in spec_knob_names() {
        assert!(props.contains_key(&knob), "schema is missing '{knob}'");
    }
}

// ── Dry run ───────────────────────────────────────────────────────────────────

/// `start` / `limit` page the preview while the counts cover every row.
#[test]
fn a_dry_run_pages_the_preview_and_counts_every_row() {
    let folder: Vec<FileEntry> = (1..=5).map(|i| file(&format!("img{i}.jpg"))).collect();
    let batch: Vec<String> = folder.iter().map(|f| f.name.clone()).collect();
    let batch: Vec<&str> = batch.iter().map(String::as_str).collect();
    let rows = preview_of(json!({ "nameMask": "trip_[C]" }), &folder, &batch);

    let page = dry_run_page(Path::new(DIR), &rows, 1, 2, RowFilter::All);
    assert_eq!(page.counts.ready, 5);
    assert_eq!((page.total, page.start, page.returned, page.truncated), (5, 1, 2, true));
    let names: Vec<_> = page
        .rows
        .iter()
        .map(|r| (r.old_name.as_str(), r.new_name.as_str()))
        .collect();
    assert_eq!(names, vec![("img2.jpg", "trip_2.jpg"), ("img3.jpg", "trip_3.jpg")]);

    let last = dry_run_page(Path::new(DIR), &rows, 4, 2, RowFilter::All);
    assert_eq!((last.returned, last.truncated), (1, false));
}

/// The problems filter lists the rows the sheet would flag, and nothing else.
#[test]
fn the_problems_filter_keeps_only_flagged_rows() {
    let folder = vec![file("a.jpg"), file("b.jpg"), file("taken.jpg")];
    // `a` and `b` both become `taken.jpg`, which a file that stays already holds.
    let rows = preview_of(json!({ "nameMask": "taken" }), &folder, &["a.jpg", "b.jpg"]);
    let page = dry_run_page(Path::new(DIR), &rows, 0, 50, RowFilter::Problems);
    assert_eq!(page.counts.problems, 2);
    assert_eq!(page.total, 2);
    assert!(page.rows.iter().all(|r| r.status.is_problem()));
}

// ── Proposal ──────────────────────────────────────────────────────────────────

fn migrated_conn() -> rusqlite::Connection {
    let conn = crate::sqlite_util::open_in_memory().expect("in-memory db");
    conn.execute_batch("PRAGMA foreign_keys = ON;").expect("pragma");
    run_migrations(&conn, MIGRATIONS).expect("migrate");
    conn
}

/// The non-dry run stages ONLY the ready rows as one pending rename group, each carrying what
/// its file looked like, and renames nothing.
#[test]
fn a_proposal_stages_the_ready_rows_as_one_pending_rename_group() {
    let folder = vec![file("a.jpg"), file("b.jpg"), file("keep.txt")];
    let rows = preview_of(
        json!({ "nameMask": "[N]_lisbon" }),
        &folder,
        &["a.jpg", "b.jpg", "gone.jpg"],
    );
    assert_eq!(Counts::of(&rows).problems, 1, "gone.jpg is Missing");

    let group = rename_group("root", Path::new(DIR), &rows, &folder, Some("Tag the trip".into())).expect("ready rows");
    let conn = migrated_conn();
    let sweep = NewSweep {
        conversation_id: None,
        created_by_model: None,
        rationale: None,
    };
    let proposed = crate::agent::suggested_ops::propose(&conn, &sweep, std::slice::from_ref(&group), 1_800_000_000)
        .expect("proposed");
    let group_id = proposed.group_ids[0];

    let stored = get_group(&conn, group_id).expect("read").expect("group exists");
    assert_eq!(stored.verb, ProposalVerb::Rename);
    assert_eq!(stored.status, ProposalStatus::Pending);
    assert_eq!(stored.destination.as_deref(), Some(DIR));
    assert_eq!(stored.display_name, "Rename two files in lisbon");

    let ops = page_ops(&conn, group_id, 100, 0).expect("ops");
    let pairs: Vec<_> = ops
        .iter()
        .map(|op| (op.source_path.as_str(), op.destination.as_deref()))
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("/trips/lisbon/a.jpg", Some("a_lisbon.jpg")),
            ("/trips/lisbon/b.jpg", Some("b_lisbon.jpg"))
        ]
    );
    assert_eq!(
        (ops[0].snapshot_size, ops[0].snapshot_mtime, ops[0].snapshot_inode),
        (Some(100), Some(1_781_557_809), Some(7))
    );
}

#[test]
fn a_batch_with_nothing_ready_is_refused_rather_than_staged_empty() {
    let folder = vec![file("a.jpg")];
    let rows = preview_of(json!({}), &folder, &["a.jpg"]);
    assert_eq!(
        rename_group("root", Path::new(DIR), &rows, &folder, None).expect_err("nothing ready"),
        ProposalRefusal::NothingReady
    );
}
