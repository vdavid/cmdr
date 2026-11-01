//! `outcome_of`: what came back, per tool, and the check order between refusal shapes.

use serde_json::{Value, json};

use super::fixtures;
use super::result;
use crate::agent::llm::types::{AgentToolResult, ToolId};
use crate::agent::memory::MemoryRefusalKind;
use crate::agent::tools::line::{IndexUnavailableReason, RefusalReason, ToolOutcome, outcome_of};
use crate::location::miss::MissToken;
use crate::tools::connect::ConnectOutcome;
use crate::tools::read::pane_listing::PaneListingScope;

fn outcome(tool: ToolId, content: Value) -> ToolOutcome {
    outcome_of(&tool, &result(content))
}

fn items(returned: u32, total: Option<u64>) -> ToolOutcome {
    ToolOutcome::Items {
        returned,
        total,
        lower_bound: false,
    }
}

fn refused(reason: RefusalReason) -> ToolOutcome {
    ToolOutcome::Refused { reason }
}

/// Every tool's success shape, paired with the outcome it reads as.
fn success_table() -> Vec<(ToolId, Value, ToolOutcome)> {
    vec![
        (
            ToolId::AppState,
            json!({ "focusedPane": "left" }),
            ToolOutcome::Answered,
        ),
        (ToolId::ListDir, fixtures::list_dir(), items(2, Some(5))),
        (
            ToolId::ListPaneFiles,
            fixtures::pane_listing(PaneListingScope::Folder, &["a", "b"], 640),
            items(2, Some(640)),
        ),
        (
            ToolId::ImportantFolders,
            fixtures::important_folders(&["/a", "/b"], false),
            items(2, Some(2)),
        ),
        (
            ToolId::ImportantFolders,
            fixtures::important_folders(&["/a", "/b"], true),
            items(2, None),
        ),
        (
            ToolId::FolderImportance,
            json!({ "status": "scored", "path": "/a" }),
            ToolOutcome::Answered,
        ),
        (ToolId::ListVolumes, fixtures::list_volumes(), items(2, Some(2))),
        (ToolId::InspectFile, fixtures::inspect_with_find(), items(5, Some(6))),
        (ToolId::OperationsList, fixtures::operations_list(), items(1, Some(3))),
        (
            ToolId::OperationsGet,
            json!({ "opId": "op-1", "items": [] }),
            ToolOutcome::Answered,
        ),
        (
            ToolId::Search,
            fixtures::search(4_812, 30, true, true),
            ToolOutcome::Matches {
                returned: 30,
                match_count: 4_812,
                lower_bound: true,
            },
        ),
        (
            ToolId::Search,
            fixtures::search(12, 12, true, false),
            ToolOutcome::Matches {
                returned: 12,
                match_count: 12,
                lower_bound: false,
            },
        ),
        (
            ToolId::Search,
            fixtures::search(0, 0, false, false),
            ToolOutcome::NothingFound { lower_bound: true },
        ),
        (
            ToolId::Search,
            fixtures::search(0, 0, true, false),
            ToolOutcome::NothingFound { lower_bound: false },
        ),
        (
            ToolId::SearchPhotos,
            fixtures::search_photos(40, true),
            ToolOutcome::Matches {
                returned: 3,
                match_count: 40,
                lower_bound: true,
            },
        ),
        (
            ToolId::SearchPhotos,
            fixtures::search_photos(0, false),
            ToolOutcome::NothingFound { lower_bound: false },
        ),
        (
            ToolId::SearchPhotos,
            json!({ "status": "semanticModelNotInstalled", "note": "…" }),
            ToolOutcome::IndexUnavailable {
                reason: IndexUnavailableReason::SemanticModelNotInstalled,
            },
        ),
        (ToolId::ImageFacts, fixtures::image_facts(), items(3, Some(3))),
        (
            ToolId::ImageFacts,
            json!({ "status": "imageIndexingOff", "note": "…" }),
            ToolOutcome::IndexUnavailable {
                reason: IndexUnavailableReason::ImageIndexingOff,
            },
        ),
        (
            ToolId::ProposeRenamePlan,
            json!({ "readyForReview": true, "count": 12 }),
            ToolOutcome::Staged { count: 12 },
        ),
        (ToolId::ListSuggestions, fixtures::list_suggestions(), items(1, Some(1))),
        (
            ToolId::GetSuggestionGroup,
            fixtures::suggestion_group(),
            items(1, Some(4)),
        ),
        (
            ToolId::GetSuggestionGroup,
            json!({ "found": false, "groupId": 7 }),
            ToolOutcome::NothingFound { lower_bound: false },
        ),
        (
            ToolId::ProposeSuggestions,
            fixtures::sweep_staged(),
            ToolOutcome::Staged { count: 6 },
        ),
        (
            ToolId::ProposeServer,
            fixtures::server_staged(),
            ToolOutcome::Staged { count: 1 },
        ),
        (
            ToolId::NothingToSuggest,
            json!({ "acknowledged": true }),
            ToolOutcome::Answered,
        ),
        (ToolId::MemoryWrite, fixtures::memory_saved(), ToolOutcome::Saved),
        (ToolId::MemoryEdit, fixtures::memory_saved(), ToolOutcome::Saved),
        (
            ToolId::ConnectVolume,
            fixtures::connect(ConnectOutcome::Connected),
            ToolOutcome::Connection {
                outcome: ConnectOutcome::Connected,
            },
        ),
    ]
}

#[test]
fn every_tools_success_shape_reads_as_its_outcome() {
    for (tool, content, expected) in success_table() {
        assert_eq!(outcome(tool.clone(), content), expected, "{tool:?}");
    }
}

#[test]
fn every_known_tool_has_a_success_row() {
    let covered: Vec<ToolId> = success_table().into_iter().map(|(tool, ..)| tool).collect();
    for tool in ToolId::KNOWN {
        assert!(covered.contains(&tool), "{tool:?} has no success fixture");
    }
}

#[test]
fn a_capped_live_listing_counts_a_lower_bound() {
    let mut capped = fixtures::list_dir();
    capped["totalIsLowerBound"] = json!(true);
    assert_eq!(
        outcome(ToolId::ListDir, capped),
        ToolOutcome::Items {
            returned: 2,
            total: Some(5),
            lower_bound: true,
        }
    );
}

#[test]
fn a_location_miss_reads_as_its_token_on_every_tool_that_takes_a_place() {
    let placed = [
        ToolId::ListDir,
        ToolId::ImportantFolders,
        ToolId::FolderImportance,
        ToolId::Search,
        ToolId::InspectFile,
    ];
    for tool in placed {
        let read = outcome(tool.clone(), fixtures::location_miss());
        assert_eq!(
            read,
            ToolOutcome::LocationMiss {
                miss: MissToken::VolumeNotConnected,
            },
            "{tool:?}"
        );
        assert!(!read.ok(), "{tool:?}");
    }
    // A token from a newer build reads as a plain answer, never a panic.
    let unknown = json!({ "miss": "fromTheFuture", "location": { "path": "/x" }, "detail": "…" });
    assert_eq!(outcome(ToolId::ListDir, unknown), ToolOutcome::Answered);
}

#[test]
fn a_connect_reads_as_how_it_went_and_only_a_live_volume_is_ok() {
    let all = [
        (ConnectOutcome::Connected, true),
        (ConnectOutcome::AlreadyConnected, true),
        (ConnectOutcome::StillReconnecting, false),
        (ConnectOutcome::NeedsCredentials, false),
        (ConnectOutcome::NeedsApproval, false),
        (ConnectOutcome::Rejected, false),
        (ConnectOutcome::Unreachable, false),
        (ConnectOutcome::NotSaved, false),
        (ConnectOutcome::Cancelled, false),
    ];
    for (connect, ok) in all {
        let read = outcome(ToolId::ConnectVolume, fixtures::connect(connect));
        assert_eq!(read, ToolOutcome::Connection { outcome: connect });
        assert_eq!(read.ok(), ok, "{connect:?}");
    }
}

#[test]
fn a_hallucinated_tool_is_refused_as_unavailable() {
    assert_eq!(
        outcome(ToolId::Unrecognized("delete".into()), fixtures::unavailable()),
        refused(RefusalReason::ToolUnavailable)
    );
}

#[test]
fn a_breaker_annotated_result_is_a_repeated_call_not_a_problem() {
    // The breaker KEEPS the original `problem`, so `repeatedCall` must be checked first.
    assert_eq!(
        outcome(ToolId::ListDir, fixtures::repeated()),
        refused(RefusalReason::RepeatedCall)
    );
    assert_eq!(
        outcome(ToolId::ListDir, fixtures::problem()),
        refused(RefusalReason::Problem)
    );
}

#[test]
fn a_propose_problem_is_not_staged_with_its_problem_kept_for_the_preview() {
    // `ensure_review_verdict` stamps `readyForReview: false` onto every propose problem.
    let stamped = json!({ "readyForReview": false, "problem": "renames needs at least one row." });
    assert_eq!(
        outcome(ToolId::ProposeRenamePlan, stamped.clone()),
        ToolOutcome::NotStaged
    );
    assert_eq!(
        outcome(ToolId::ProposeSuggestions, stamped.clone()),
        ToolOutcome::NotStaged
    );
    assert_eq!(outcome(ToolId::ProposeServer, stamped), ToolOutcome::NotStaged);
    let evidence = json!({ "readyForReview": false, "evidenceRejected": [], "guidance": "…" });
    assert_eq!(outcome(ToolId::ProposeRenamePlan, evidence), ToolOutcome::NotStaged);
}

/// A server card is one suggestion with no ops, so the line counts the card; one the user
/// answered in the meantime kept its old answer and staged nothing new.
#[test]
fn a_server_suggestion_counts_its_card_not_its_ops() {
    let mut answered = fixtures::server_staged();
    answered["groups"][0]["outcome"] = json!("alreadyAnswered");
    assert_eq!(
        outcome(ToolId::ProposeServer, answered),
        ToolOutcome::Staged { count: 0 }
    );
    let refused = json!({ "readyForReview": false, "refusal": "addressChange", "field": "host", "problem": "…" });
    assert_eq!(outcome(ToolId::ProposeServer, refused), ToolOutcome::NotStaged);
}

/// A proposal naming a place that can't run now refuses in the read tools' miss shape, so the
/// line says why ("that drive isn't connected") the way a read's does, still never as staged.
#[test]
fn a_propose_location_refusal_reads_as_its_miss() {
    let mut refused = fixtures::location_miss();
    refused["readyForReview"] = json!(false);
    refused["refusal"] = json!("location");
    refused["group"] = json!(0);
    refused["field"] = json!("destination");
    let read = outcome(ToolId::ProposeSuggestions, refused);
    assert_eq!(
        read,
        ToolOutcome::LocationMiss {
            miss: MissToken::VolumeNotConnected
        }
    );
    assert!(!read.ok());
}

#[test]
fn a_memory_refusal_carries_its_kind() {
    for kind in MemoryRefusalKind::ALL {
        assert_eq!(
            outcome(ToolId::MemoryWrite, fixtures::memory_refused(kind.token())),
            refused(RefusalReason::Memory { refusal: kind }),
            "{kind:?}"
        );
    }
    assert_eq!(
        outcome(ToolId::MemoryEdit, fixtures::memory_refused("fromTheFuture")),
        refused(RefusalReason::Memory {
            refusal: MemoryRefusalKind::Other,
        })
    );
}

#[test]
fn an_elided_row_reads_as_answered_whatever_its_stub_says() {
    // Nothing persists `elided: true` today (elision is assembly-only), so this is defensive.
    let stub = AgentToolResult {
        call_id: "call-1".into(),
        content: json!({ "elided_tool_result": true, "problem": "a stub never reads as a refusal" }),
        elided: true,
    };
    assert_eq!(outcome_of(&ToolId::ListDir, &stub), ToolOutcome::Answered);
}

#[test]
fn an_unexpected_shape_falls_back_to_answered_never_a_panic() {
    for tool in ToolId::KNOWN {
        for content in [json!(null), json!([1, 2]), json!("text"), json!({ "returned": "many" })] {
            let read = outcome(tool.clone(), content.clone());
            assert!(read.ok(), "{tool:?} {content} reads as {read:?}");
        }
    }
}

/// The line's glyph and the breaker's judgement read the same keys, so they agree on every
/// fixture EXCEPT where a typed refusal carries no `problem` key (`dispatch_ok` reads those as
/// ok): a memory refusal, a rename plan whose evidence was rejected, a location miss, and a
/// connect that didn't connect.
#[test]
fn ok_agrees_with_dispatch_ok_except_typed_refusals_without_a_problem_key() {
    let mut agreeing: Vec<(ToolId, Value)> = success_table()
        .into_iter()
        .map(|(tool, content, _)| (tool, content))
        .collect();
    agreeing.extend([
        (ToolId::Unrecognized("delete".into()), fixtures::unavailable()),
        (ToolId::ListDir, fixtures::repeated()),
        (ToolId::ListDir, fixtures::problem()),
        (
            ToolId::ProposeRenamePlan,
            json!({ "readyForReview": false, "problem": "renames needs at least one row." }),
        ),
    ]);
    for (tool, content) in agreeing {
        let stored = result(content.clone());
        assert_eq!(
            outcome_of(&tool, &stored).ok(),
            !stored.reports_a_problem(),
            "{tool:?} {content}"
        );
    }

    let disagreeing = [
        (ToolId::MemoryWrite, fixtures::memory_refused("memoryFull")),
        (
            ToolId::ProposeRenamePlan,
            json!({ "readyForReview": false, "evidenceRejected": [], "guidance": "…" }),
        ),
        (ToolId::ListDir, fixtures::location_miss()),
        (
            ToolId::ConnectVolume,
            fixtures::connect(ConnectOutcome::NeedsCredentials),
        ),
    ];
    for (tool, content) in disagreeing {
        let stored = result(content.clone());
        assert!(!stored.reports_a_problem(), "dispatch_ok reads {content} as ok");
        assert!(!outcome_of(&tool, &stored).ok(), "the line shows {content} as not ok");
    }
}
