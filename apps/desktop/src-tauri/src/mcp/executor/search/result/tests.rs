//! Pure shaper tests: a fabricated [`LiveAnswer`] in, a DTO out. No Tauri
//! harness, no running search, no index.

use super::*;
use crate::search::live::{CoverageKind, SearchRunCoverage};

/// A run that covered everything, over `volume`.
fn covered(volume: &str) -> SearchRunCoverage {
    SearchRunCoverage {
        walk: WalkEnding::NothingToWalk,
        kind: CoverageKind::Covered,
        permission_denied: Vec::new(),
        declined: Vec::new(),
        still_covering: Vec::new(),
        unresolved_scopes: Vec::new(),
        abandoned_ground: false,
        abandoned_locations: 0,
        capped: false,
        target_volume_id: volume.to_string(),
        hidden_by_excludes: 0,
    }
}

/// An answer with no rows, ending however the test needs.
fn answer(ending: AnswerEnding, dirs_found: u64) -> LiveAnswer {
    LiveAnswer {
        target_volume_id: "naspi".to_string(),
        entries: Vec::new(),
        match_count: 0,
        dirs_found,
        ending,
    }
}

/// A settled answer over ground with no gaps in it.
fn settled(coverage: SearchRunCoverage) -> AnswerEnding {
    AnswerEnding::Settled(Box::new(coverage))
}

/// `n` plausible rows: a long-ish path and a name, the shape that makes a big
/// page expensive.
fn rows(n: usize) -> Vec<SearchResultEntry> {
    (0..n)
        .map(|i| SearchResultEntry {
            name: format!("2026-03-{i:04} quarterly report and appendix.pdf"),
            path: format!(
                "/Users/dave/Documents/Rymd/invoices/2026/q1/2026-03-{i:04} quarterly report and appendix.pdf"
            ),
            parent_path: "~/Documents/Rymd/invoices/2026/q1".to_string(),
            is_directory: false,
            size: Some(340_000 + i as u64),
            modified_at: Some(1_735_689_600),
            icon_id: "pdf".to_string(),
            entry_id: i as i64,
        })
        .collect()
}

fn joined(notes: &[String]) -> String {
    notes.join("\n")
}

// ── The size contract ────────────────────────────────────────────────────────

#[test]
fn a_search_result_is_cut_to_the_budget_and_reports_the_counts() {
    // The defect the text table had: `limit: 5000` serialized the whole table,
    // six figures of estimated tokens, and pushed the rest of the caller's turn
    // out of the prompt. The row cap can't bound a payload, so the shaper cuts
    // to the budget on top of it — and says it did.
    let mut fabricated = answer(settled(covered("root")), 0);
    fabricated.entries = rows(5_000);
    fabricated.match_count = 5_000;

    let result = shape_answer(fabricated, true);

    assert!(
        result.returned < 5_000,
        "5,000 padded rows can't fit one tool result, {} came back",
        result.returned
    );
    assert_eq!(result.returned, result.entries.len());
    assert!(result.truncated, "a cut is never silent");
    assert_eq!(
        result.match_count, 5_000,
        "the count speaks for every match, including the ones the cut dropped"
    );
    assert!(!result.entries.is_empty(), "a caller always learns something concrete");
}

#[test]
fn a_result_that_fits_is_not_marked_truncated() {
    let mut fabricated = answer(settled(covered("root")), 0);
    fabricated.entries = rows(3);
    fabricated.match_count = 3;

    let result = shape_answer(fabricated, true);

    assert_eq!(result.returned, 3);
    assert!(!result.truncated);
    assert_eq!(result.match_count_human, "3 matches");
}

#[test]
fn a_hit_carries_both_the_raw_number_and_the_spoken_one() {
    let mut fabricated = answer(settled(covered("root")), 0);
    fabricated.entries = vec![SearchResultEntry {
        name: "test.pdf".to_string(),
        path: "/Users/dave/Documents/test.pdf".to_string(),
        parent_path: "~/Documents".to_string(),
        is_directory: false,
        size: Some(340_000),
        modified_at: Some(1_735_689_600),
        icon_id: "pdf".to_string(),
        entry_id: 1,
    }];
    fabricated.match_count = 1;

    let result = shape_answer(fabricated, true);
    let hit = &result.entries[0];

    assert_eq!(hit.size_bytes, Some(340_000));
    // ❌ Never a second formatter: these are the dialog's own.
    assert_eq!(hit.size_human.as_deref(), Some(format_size(340_000).as_str()));
    assert_eq!(hit.modified, Some(1_735_689_600));
    assert_eq!(
        hit.modified_human.as_deref(),
        Some(format_timestamp(1_735_689_600).as_str())
    );
    assert_eq!(result.match_count_human, "1 match", "and never \"1 matches\"");

    // A model can't render an icon, so the field isn't on the wire at all.
    let json = serde_json::to_value(&result).expect("the result serializes");
    assert!(json["entries"][0].get("iconId").is_none(), "{json}");
}

#[test]
fn a_size_the_index_has_no_number_for_stays_absent() {
    // A NULL logical size is a hardlink-deduped row, ❌ never a zero-byte file,
    // so it must not arrive as `0`.
    let mut fabricated = answer(settled(covered("root")), 0);
    fabricated.entries = vec![SearchResultEntry {
        name: "Projects".to_string(),
        path: "/Users/dave/Projects".to_string(),
        parent_path: "~".to_string(),
        is_directory: true,
        size: None,
        modified_at: None,
        icon_id: "dir".to_string(),
        entry_id: 2,
    }];

    let json = serde_json::to_value(shape_answer(fabricated, true)).expect("the result serializes");
    let hit = &json["entries"][0];
    assert!(hit.get("sizeBytes").is_none(), "{hit}");
    assert!(hit.get("sizeHuman").is_none(), "{hit}");
    assert!(hit.get("modified").is_none(), "{hit}");
    assert_eq!(hit["isDirectory"], serde_json::json!(true));
}

// ── `complete`, the one field a caller reads before saying "that's all" ──────

#[test]
fn coverage_complete_is_false_when_any_gap_is_set() {
    // A derived boolean that's right five times out of six is worse than no
    // boolean, so every gap gets its own turn.
    let clean = answer(settled(covered("root")), 0);
    assert!(
        shape_answer(clean, true).coverage.complete,
        "an index-served run over whole ground IS complete"
    );

    let gaps: Vec<(&str, SearchRunCoverage)> = vec![
        (
            "an interrupted walk",
            SearchRunCoverage {
                walk: WalkEnding::Interrupted,
                ..covered("root")
            },
        ),
        (
            "a cancelled walk",
            SearchRunCoverage {
                walk: WalkEnding::Cancelled,
                ..covered("root")
            },
        ),
        (
            "a refused folder",
            SearchRunCoverage {
                permission_denied: vec!["/Users/dave/Documents".to_string()],
                ..covered("root")
            },
        ),
        (
            "a declined snapshot tree",
            SearchRunCoverage {
                declined: vec!["/Volumes/naspi/@eaDir".to_string()],
                ..covered("root")
            },
        ),
        (
            "ground another walk holds",
            SearchRunCoverage {
                still_covering: vec!["/Volumes/naspi/photos".to_string()],
                ..covered("root")
            },
        ),
        (
            "a scope nothing could resolve",
            SearchRunCoverage {
                unresolved_scopes: vec!["~/Desktp".to_string()],
                ..covered("root")
            },
        ),
        (
            "abandoned ground",
            SearchRunCoverage {
                abandoned_ground: true,
                abandoned_locations: 1,
                ..covered("root")
            },
        ),
    ];
    for (what, coverage) in gaps {
        let result = shape_answer(answer(settled(coverage), 0), true);
        assert!(!result.coverage.complete, "{what}: the answer isn't complete");
        assert!(
            result.match_count_human.starts_with('≥'),
            "{what}: the count is a floor, got {}",
            result.match_count_human
        );
        assert!(!result.notes.is_empty(), "{what}: the gap gets its own sentence");
    }
}

#[test]
fn a_still_walking_answer_never_reads_as_no_matches() {
    // The wrong conclusion this exists to stop: an agent reading a partial answer
    // as "I searched, nothing matched".
    let mut walking = answer(AnswerEnding::StillWalking, 480);
    walking.match_count = 1_240;

    let result = shape_answer(walking, true);

    assert_eq!(result.match_count_human, "≥ 1,240 matches");
    assert!(result.coverage.still_walking);
    assert!(!result.coverage.complete);
    assert_eq!(result.coverage.folders_found, 480);
    let notes = joined(&result.notes);
    assert!(notes.contains("still walking"), "{notes}");
    assert!(notes.contains("naspi") && notes.contains("480"), "{notes}");
    assert!(notes.contains("again") && notes.contains("maxWaitSeconds"), "{notes}");
}

#[test]
fn a_capped_run_reports_a_floor_even_though_it_covered_its_ground() {
    // The rows stopped, the count didn't. Cmdr covered everything it was asked
    // to, so `complete` stays true — and the number is still a floor.
    let capped = SearchRunCoverage {
        capped: true,
        ..covered("root")
    };
    let mut fabricated = answer(settled(capped), 0);
    fabricated.match_count = 4_200;

    let result = shape_answer(fabricated, true);

    assert!(result.coverage.capped);
    assert!(result.coverage.complete, "a cap is not uncovered ground");
    assert_eq!(result.match_count_human, "≥ 4,200 matches");
}

// ── The filtered count ───────────────────────────────────────────────────────

#[test]
fn hidden_by_excludes_survives_into_the_result() {
    // The failure this prevents: "27 files match" over a machine where 400 more
    // sit in node_modules and Caches. Silently filtering a COUNT is how an agent
    // states a wrong conclusion confidently, and a disk-space question is
    // answered mostly by the folders the defaults hide.
    let coverage = SearchRunCoverage {
        hidden_by_excludes: 400,
        ..covered("root")
    };
    let mut fabricated = answer(settled(coverage), 0);
    fabricated.match_count = 27;

    let result = shape_answer(fabricated, true);

    assert_eq!(result.coverage.hidden_by_excludes, 400, "the number is on the wire");
    let notes = joined(&result.notes);
    assert!(notes.contains("400"), "{notes}");
    assert!(
        notes.contains("excludeSystemDirs"),
        "the way to see them is named: {notes}"
    );
}

#[test]
fn with_the_default_tier_already_off_the_note_stops_advising_it() {
    // Everything hidden then came from the caller's own `!` excludes, and
    // telling them to pass a flag they already passed is noise.
    let coverage = SearchRunCoverage {
        hidden_by_excludes: 3,
        ..covered("root")
    };
    let notes = joined(&shape_answer(answer(settled(coverage), 0), false).notes);
    assert!(notes.contains('3'), "{notes}");
    assert!(!notes.contains("excludeSystemDirs"), "{notes}");
}

// ── The authored sentences ───────────────────────────────────────────────────

#[test]
fn a_run_that_covered_its_scope_from_the_index_says_nothing() {
    // The notes exist to name ground the answer doesn't speak for. A complete
    // answer has none, and a line per search would train an agent to skip
    // them all.
    let result = shape_answer(answer(settled(covered("naspi")), 0), true);
    assert!(result.notes.is_empty());
    assert!(result.coverage.complete);
}

#[test]
fn the_two_unreadable_lists_get_two_different_sentences() {
    // The typed unreadable cause, end to end: one half is a permission somebody can
    // grant, the other is ground Cmdr declines to read. ❌ Never one list and
    // never one sentence — offering Full Disk Access over a snapshot folder
    // is advice that does nothing.
    let coverage = SearchRunCoverage {
        walk: WalkEnding::Completed,
        kind: CoverageKind::Live,
        permission_denied: vec!["/Users/dave/Documents".to_string()],
        declined: vec!["/Volumes/naspi/@eaDir".to_string()],
        ..covered("naspi")
    };
    let result = shape_answer(answer(settled(coverage), 12), true);

    assert_eq!(result.coverage.permission_denied, vec!["/Users/dave/Documents"]);
    assert_eq!(result.coverage.declined, vec!["/Volumes/naspi/@eaDir"]);
    let notes = joined(&result.notes);
    assert!(notes.contains("/Users/dave/Documents"), "{notes}");
    assert!(notes.contains("/Volumes/naspi/@eaDir"), "{notes}");
    assert!(notes.contains("snapshot folders"), "{notes}");
    assert!(
        result.notes.len() >= 3,
        "each cause gets its own sentence, plus the walk's own line: {notes}"
    );
}

#[test]
fn full_disk_access_is_offered_only_when_granting_it_would_help() {
    let refused = vec!["/Users/dave/Downloads".to_string()];
    let offered = refusal_note(&refused, true);
    assert!(offered.contains("/Users/dave/Downloads"));
    assert!(offered.contains("Full Disk Access"));
    // Cmdr already has it (or this isn't macOS): the folder is still named,
    // and no advice that would do nothing.
    let plain = refusal_note(&refused, false);
    assert!(plain.contains("/Users/dave/Downloads"));
    assert!(!plain.contains("Full Disk Access"));
}

#[test]
fn an_interrupted_walk_says_the_list_is_a_lower_bound() {
    let coverage = SearchRunCoverage {
        walk: WalkEnding::Interrupted,
        kind: CoverageKind::Live,
        ..covered("naspi")
    };
    let notes = joined(&shape_answer(answer(settled(coverage), 3), true).notes);
    assert!(notes.contains("lower bound"), "{notes}");
}

#[test]
fn a_completed_walk_reports_the_ground_it_added_to_the_index() {
    let coverage = SearchRunCoverage {
        walk: WalkEnding::Completed,
        kind: CoverageKind::Live,
        ..covered("naspi")
    };
    let result = shape_answer(answer(settled(coverage), 21_482), true);
    assert!(
        result.coverage.complete,
        "a completed walk over clean ground is complete"
    );
    assert_eq!(result.coverage.folders_found, 21_482);
    // Spoken, so the count carries its thousands separator like every other one.
    assert!(joined(&result.notes).contains("21,482"), "{:?}", result.notes);
}

// ── The serialized shape ─────────────────────────────────────────────────────

#[test]
fn the_result_serializes_camel_case_with_coverage_beside_the_notes() {
    let result = shape_answer(answer(settled(covered("root")), 0), true);
    let json = serde_json::to_value(&result).expect("the result serializes");

    for key in [
        "targetVolumeId",
        "matchCount",
        "matchCountHuman",
        "returned",
        "truncated",
        "entries",
        "coverage",
        "notes",
    ] {
        assert!(json.get(key).is_some(), "{key} is missing from {json}");
    }
    for key in [
        "complete",
        "stillWalking",
        "foldersFound",
        "capped",
        "hiddenByExcludes",
        "permissionDenied",
        "declined",
        "stillCovering",
        "unresolvedScopes",
        "abandonedGround",
        "abandonedLocations",
    ] {
        assert!(
            json["coverage"].get(key).is_some(),
            "coverage.{key} is missing from {json}"
        );
    }
}

#[test]
fn an_ai_search_result_leads_with_what_the_translator_understood() {
    let result = AiSearchResult {
        interpreted_query: "name matches *invoice*, changed after 2026-03-01".to_string(),
        search: shape_answer(answer(settled(covered("root")), 0), true),
    };
    let json = serde_json::to_value(&result).expect("the result serializes");

    assert_eq!(
        json["interpretedQuery"],
        serde_json::json!("name matches *invoice*, changed after 2026-03-01")
    );
    // Flattened, so an ai_search answer reads exactly like a search one below it.
    assert!(json.get("matchCountHuman").is_some(), "{json}");
    assert!(json["coverage"].get("complete").is_some(), "{json}");
}
