use super::*;
use crate::test_support::TestDir;
use cmdr_index::media_index::testing::seed_ocr_row;

fn semantic(path: &str, score: f32) -> SemanticHit {
    SemanticHit {
        path: path.to_string(),
        score,
    }
}
fn ocr(path: &str, snippet: &str) -> OcrHit {
    OcrHit {
        path: path.to_string(),
        snippet: snippet.to_string(),
    }
}
fn tag(path: &str, score: f32) -> TagHit {
    TagHit {
        path: path.to_string(),
        score,
    }
}

#[test]
fn parse_requested_mode_maps_known_and_rejects_unknown() {
    assert_eq!(parse_requested_mode(None).unwrap(), RequestedMode::Auto);
    assert_eq!(parse_requested_mode(Some("semantic")).unwrap(), RequestedMode::Semantic);
    assert_eq!(parse_requested_mode(Some("ocr")).unwrap(), RequestedMode::Ocr);
    assert_eq!(parse_requested_mode(Some("tag")).unwrap(), RequestedMode::Tag);
    assert!(parse_requested_mode(Some("faces")).is_err());
}

#[test]
fn resolve_effective_mode_mirrors_ui_and_is_honest_about_the_model() {
    // Auto with a model composes semantic + OCR; without a model it degrades to OCR.
    assert_eq!(
        resolve_effective_mode(RequestedMode::Auto, true),
        ModeResolution::Search {
            mode: EffectiveMode::SemanticThenOcr,
            degraded_to_ocr: false
        }
    );
    assert_eq!(
        resolve_effective_mode(RequestedMode::Auto, false),
        ModeResolution::Search {
            mode: EffectiveMode::OcrOnly,
            degraded_to_ocr: true
        }
    );
    // An EXPLICIT semantic ask with no model is an honest status, not a silent OCR swap.
    assert_eq!(
        resolve_effective_mode(RequestedMode::Semantic, false),
        ModeResolution::SemanticModelMissing
    );
    assert_eq!(
        resolve_effective_mode(RequestedMode::Semantic, true),
        ModeResolution::Search {
            mode: EffectiveMode::SemanticOnly,
            degraded_to_ocr: false
        }
    );
    // OCR and tag never depend on the model.
    assert_eq!(
        resolve_effective_mode(RequestedMode::Ocr, false),
        ModeResolution::Search {
            mode: EffectiveMode::OcrOnly,
            degraded_to_ocr: false
        }
    );
    assert_eq!(
        resolve_effective_mode(RequestedMode::Tag, false),
        ModeResolution::Search {
            mode: EffectiveMode::Tag,
            degraded_to_ocr: false
        }
    );
}

#[test]
fn merge_hits_leads_with_semantic_then_dedups_ocr_by_path() {
    // A path matched by BOTH semantic and OCR appears once, as the description hit.
    let hits = merge_hits(
        "root",
        EffectiveMode::SemanticThenOcr,
        vec![semantic("/a.jpg", 0.9), semantic("/b.jpg", 0.8)],
        vec![ocr("/a.jpg", "[a]"), ocr("/c.jpg", "[c]")],
        vec![],
    );
    let paths: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
    assert_eq!(paths, vec!["/a.jpg", "/b.jpg", "/c.jpg"]);
    assert_eq!(hits[0].match_kind, "description");
    assert_eq!(hits[0].score, Some(0.9));
    assert_eq!(hits[0].match_text, None);
    // The deduped /a.jpg stays a description hit (semantic leads), not re-added as text.
    assert_eq!(hits.iter().filter(|h| h.path == "/a.jpg").count(), 1);
    // /c.jpg is an OCR text hit carrying its snippet.
    let c = hits.iter().find(|h| h.path == "/c.jpg").unwrap();
    assert_eq!(c.match_kind, "text");
    assert_eq!(c.match_text.as_deref(), Some("[c]"));
    assert_eq!(c.score, None);
}

#[test]
fn merge_hits_ocr_only_and_tag_only_carry_the_right_kind() {
    let ocr_hits = merge_hits(
        "root",
        EffectiveMode::OcrOnly,
        vec![],
        vec![ocr("/x.jpg", "[x]")],
        vec![],
    );
    assert_eq!(ocr_hits.len(), 1);
    assert_eq!(ocr_hits[0].match_kind, "text");
    assert_eq!(ocr_hits[0].match_text.as_deref(), Some("[x]"));

    let tag_hits = merge_hits("root", EffectiveMode::Tag, vec![], vec![], vec![tag("/dog.jpg", 0.77)]);
    assert_eq!(tag_hits.len(), 1);
    assert_eq!(tag_hits[0].match_kind, "tag");
    assert_eq!(tag_hits[0].score, Some(0.77));
    assert_eq!(tag_hits[0].match_text, None);

    // OcrOnly never emits semantic hits even when some are passed.
    let no_semantic = merge_hits(
        "root",
        EffectiveMode::OcrOnly,
        vec![semantic("/s.jpg", 0.5)],
        vec![],
        vec![],
    );
    assert!(no_semantic.is_empty());
}

#[test]
fn sort_and_cap_orders_by_score_then_caps() {
    let hits = vec![
        PhotoHit {
            path: "/low.jpg".into(),
            volume: "root".into(),
            match_kind: "description".into(),
            score: Some(0.3),
            match_text: None,
        },
        PhotoHit {
            path: "/text.jpg".into(),
            volume: "root".into(),
            match_kind: "text".into(),
            score: None,
            match_text: Some("[t]".into()),
        },
        PhotoHit {
            path: "/high.jpg".into(),
            volume: "root".into(),
            match_kind: "description".into(),
            score: Some(0.95),
            match_text: None,
        },
    ];
    let capped = sort_and_cap(hits, 2);
    assert_eq!(capped.len(), 2);
    // Highest score first; the scoreless text hit sinks below the scored ones.
    assert_eq!(capped[0].path, "/high.jpg");
    assert_eq!(capped[1].path, "/low.jpg");
}

fn state(indexing: bool, enriched: u64, qualifying: Option<u64>) -> MediaIndexVolumeState {
    MediaIndexVolumeState {
        enabled: true,
        indexing,
        enriched_count: enriched,
        qualifying_count: qualifying,
        network_opt_in: false,
        always_indexed: false,
        paused: false,
        waiting_for_importance: false,
        covered_qualifying_count: None,
        kept_count: None,
    }
}

#[test]
fn derive_coverage_flags_incomplete_when_indexing_or_short() {
    // A running pass ⇒ incomplete regardless of counts.
    assert!(derive_coverage("root", &state(true, 10, Some(10))).incomplete);
    // Fewer enriched than qualify ⇒ incomplete.
    assert!(derive_coverage("root", &state(false, 5, Some(20))).incomplete);
    // Done and idle ⇒ complete.
    assert!(!derive_coverage("root", &state(false, 20, Some(20))).incomplete);
    // Unknown denominator, idle ⇒ not flagged incomplete off a missing count.
    assert!(!derive_coverage("root", &state(false, 20, None)).incomplete);
}

/// The scope-aware denominator wins over the volume total. In the "only folders I
/// choose" scope, fully-indexed chosen folders are a handful of images against the
/// whole drive's qualifying count, so comparing against the volume total reports
/// incomplete forever and the agent tells the user their indexed folder isn't
/// indexed. Regression anchor for exactly that.
#[test]
fn derive_coverage_prefers_the_scope_aware_denominator() {
    let mut covered = state(false, 4, Some(229_945));
    covered.covered_qualifying_count = Some(4);
    let coverage = derive_coverage("root", &covered);
    assert!(
        !coverage.incomplete,
        "4 of 4 covered images is complete, not incomplete"
    );
    assert_eq!(coverage.qualifying_count, Some(4), "reports the covered denominator");

    // Still short within the covered set ⇒ genuinely incomplete.
    let mut partial = state(false, 3, Some(229_945));
    partial.covered_qualifying_count = Some(10);
    assert!(derive_coverage("root", &partial).incomplete);

    // No scope-aware count ⇒ fall back to the volume total, unchanged behaviour.
    let mut fallback = state(false, 5, Some(20));
    fallback.covered_qualifying_count = None;
    assert!(derive_coverage("root", &fallback).incomplete);
}

#[test]
fn build_note_leads_with_degradation_then_indexing() {
    let complete = vec![VolumeCoverage {
        volume: "root".into(),
        indexing: false,
        enriched_count: 10,
        qualifying_count: Some(10),
        incomplete: false,
    }];
    assert!(build_note(false, &complete).is_none());

    let incomplete = vec![VolumeCoverage {
        volume: "root".into(),
        indexing: true,
        enriched_count: 3,
        qualifying_count: Some(10),
        incomplete: true,
    }];
    assert!(build_note(false, &incomplete).unwrap().contains("still indexing"));
    // Degradation wins the note slot even when a volume is also incomplete.
    assert!(build_note(true, &incomplete).unwrap().contains("in-image text only"));
}

#[test]
fn honest_statuses_serialize_with_a_typed_status_tag() {
    let off = serde_json::to_value(SearchPhotosResult::ImageIndexingOff { note: "n".into() }).unwrap();
    assert_eq!(off["status"], "imageIndexingOff");

    let missing = serde_json::to_value(SearchPhotosResult::SemanticModelNotInstalled { note: "n".into() }).unwrap();
    assert_eq!(missing["status"], "semanticModelNotInstalled");

    let ok = serde_json::to_value(shape_ok("semantic+ocr".into(), vec![], vec![], None, 0)).unwrap();
    assert_eq!(ok["status"], "ok");
    // An absent note doesn't clutter the payload.
    assert!(ok.get("note").is_none());
    assert!(ok.get("protectedHidden").is_none());

    let hid = serde_json::to_value(shape_ok("ocr".into(), vec![], vec![], None, 2)).unwrap();
    assert_eq!(hid["protectedHidden"], 2);
}

/// A hidden hit is dropped before the cap, so it can't eat the limit, and the count says so.
#[test]
fn protected_hits_are_hidden_before_the_cap() {
    let dir = TestDir::new("photos-protected");
    seed_ocr_row(&dir, "root", "/Volumes/naspi/home/.ssh/a.png", "invoice one");
    seed_ocr_row(&dir, "root", "/Users/someone/Pictures/b.png", "invoice two");

    let (hits, hidden) = run_search(&dir, &["root".to_string()], "invoice", EffectiveMode::OcrOnly, 1);

    assert_eq!(hits.len(), 1, "the limit of one still finds the readable hit");
    assert_eq!(hits[0].path, "/Users/someone/Pictures/b.png");
    assert_eq!(hidden, vec![Protected::Credentials]);
}

/// The query matches the key's body, so the snippet shows no armor: the check has to read
/// the whole stored text.
#[test]
fn a_snippet_of_a_key_s_body_is_hidden_too() {
    let dir = TestDir::new("photos-ocr-key");
    let text = format!(
        "-----BEGIN OPENSSH PRIVATE KEY----- {} b3BlbnNzaC1rZXk {} -----END OPENSSH PRIVATE KEY-----",
        "filler ".repeat(200),
        "filler ".repeat(200),
    );
    seed_ocr_row(&dir, "root", "/Users/someone/Pictures/key.png", &text);
    seed_ocr_row(
        &dir,
        "root",
        "/Users/someone/Pictures/doc.png",
        "b3BlbnNzaC1rZXk in a note",
    );

    let (hits, hidden) = run_search(
        &dir,
        &["root".to_string()],
        "b3BlbnNzaC1rZXk",
        EffectiveMode::OcrOnly,
        30,
    );

    let paths: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
    assert_eq!(paths, vec!["/Users/someone/Pictures/doc.png"]);
    assert_eq!(hidden, vec![Protected::PrivateKey]);
}

#[test]
fn a_wall_of_ocr_hits_pages_instead_of_blowing_the_context() {
    use crate::agent::chat::budget::{MAX_TOOL_RESULT_TOKENS, estimate_serialized_tokens};

    // `limit: 200` OCR hits, each carrying a snippet, is far more than one tool result
    // may hold. The answer keeps the best-ranked hits and says how many it left.
    let hits: Vec<PhotoHit> = (0..MAX_LIMIT)
        .map(|i| PhotoHit {
            path: format!("/photos/some/quite/deep/folder/receipt-{i}.jpg"),
            volume: "root".into(),
            match_kind: "text".into(),
            score: None,
            match_text: Some(format!(
                "[total] 42.00 EUR — line {i} of a dense receipt {}",
                "x".repeat(300)
            )),
        })
        .collect();

    let SearchPhotosResult::Ok {
        hits,
        total,
        returned,
        truncated,
        ..
    } = shape_ok("ocr".into(), hits, vec![], None, 0)
    else {
        panic!("expected an ok result");
    };
    assert_eq!(total, MAX_LIMIT);
    assert_eq!(returned, hits.len());
    assert!(truncated, "the cut must be visible to the model");
    assert!(returned < MAX_LIMIT);
    assert!(hits[0].path.ends_with("receipt-0.jpg"), "rank order survives");
    let spent: usize = hits.iter().map(estimate_serialized_tokens).sum();
    assert!(
        spent <= MAX_TOOL_RESULT_TOKENS,
        "the hits must fit the tool-result ceiling (spent {spent})"
    );
}

/// The load-bearing privacy property: a hit is TEXT-ONLY, so the tool structurally
/// can't hand a provider image bytes or a thumbnail. Every serialized value is a
/// string or number (never a JSON array, which could smuggle a byte vector), and the
/// keys are exactly the allowed text/number fields.
#[test]
fn photo_hit_is_text_only_no_byte_fields() {
    let hit = PhotoHit {
        path: "/passport.jpg".into(),
        volume: "root".into(),
        match_kind: "text".into(),
        score: Some(0.5),
        match_text: Some("[passport]".into()),
    };
    let json = serde_json::to_value(&hit).unwrap();
    let obj = json.as_object().expect("a JSON object");
    let allowed = ["path", "volume", "matchKind", "score", "matchText"];
    for key in obj.keys() {
        assert!(allowed.contains(&key.as_str()), "unexpected field '{key}' on PhotoHit");
    }
    for (key, value) in obj {
        assert!(
            value.is_string() || value.is_number(),
            "field '{key}' must be text or a number (no bytes), got {value}"
        );
    }
}
