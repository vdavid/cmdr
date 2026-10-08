//! Counts rename plans built while the thread's latest `list_pane_files` was cut.
//!
//! A big folder's listing stops at a page, and the system prompt requires the reply to say
//! it looked at N of M items. Nothing checks that the model obeyed, and a user who believes
//! every file was renamed when only the first page was is the failure. Refusing such a plan
//! is the fix, but only if models actually misstate coverage, so this counts first: one
//! `rename_plan_from_cut_listing` event per staged plan whose folder the model last saw cut.
//!
//! The plan is compared against the listing's NUMBERS, never against what the reply says:
//! reading the model's prose for a coverage claim is string matching that breaks on a
//! paraphrase or another language.
//!
//! ❌ Every property is a bucket or a categorical token. The folder path stays in memory
//! here to match a plan to its listing and never reaches the event.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use serde::Deserialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Manager, Runtime};

use super::store::RenameProposalSnapshot;
use crate::agent::llm::types::AgentToolResult;
use crate::agent::tools::propose::evidence::EvidenceScope;
use crate::analytics::item_count_bucket;
use crate::ignore_poison::IgnorePoison;

/// The latest `list_pane_files` answer each chat thread was handed. Process-local: a plan
/// staged after a restart has no listing to compare with and counts nothing.
#[derive(Default)]
pub struct PaneListingCuts {
    latest: Mutex<HashMap<i64, ListingSeen>>,
}

/// The three numbers of a `list_pane_files` result this counter reads, by their wire names.
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct ListingSeen {
    path: String,
    total: usize,
    returned: usize,
}

impl PaneListingCuts {
    /// Remember what one `list_pane_files` result told the model. A problem result carries
    /// no listing and leaves the previous one standing; an elided one never reached the model.
    pub fn note_listing(&self, scope: EvidenceScope, result: &AgentToolResult) {
        let Some(thread) = scope.conversation_id() else {
            return;
        };
        if result.elided {
            return;
        }
        let Ok(seen) = ListingSeen::deserialize(&result.content) else {
            return;
        };
        self.latest.lock_ignore_poison().insert(thread, seen);
    }

    /// The event's properties when `plan` was built from a listing the thread saw cut, or
    /// `None` when there's nothing to count.
    fn cut_plan_props(&self, scope: EvidenceScope, plan: &RenameProposalSnapshot) -> Option<Value> {
        let thread = scope.conversation_id()?;
        let seen = self.latest.lock_ignore_poison().get(&thread).cloned()?;
        if seen.returned >= seen.total {
            return None;
        }
        let folder = Path::new(&seen.path);
        let rows = plan
            .rows
            .iter()
            .filter(|row| Path::new(&row.source_path).parent() == Some(folder))
            .count();
        if rows == 0 {
            return None;
        }
        Some(json!({
            "rows": item_count_bucket(rows),
            "listing_returned": item_count_bucket(seen.returned),
            "listing_total": item_count_bucket(seen.total),
            "coverage": coverage_token(rows, &seen),
        }))
    }
}

/// How the plan's row count in the listed folder compares with what the listing showed and
/// what the folder holds. `matches_total` is the case a refusal would target: a row for
/// every file, from a listing that showed fewer. Buckets alone can't say this (200 and 250
/// share one), which is why it's its own token.
fn coverage_token(rows: usize, seen: &ListingSeen) -> &'static str {
    if rows <= seen.returned {
        "within_returned"
    } else if rows < seen.total {
        "beyond_returned"
    } else if rows == seen.total {
        "matches_total"
    } else {
        "beyond_total"
    }
}

/// Record what a `list_pane_files` result handed the model. Called by the agent dispatcher.
pub fn note_pane_listing<R: Runtime>(app: &AppHandle<R>, scope: EvidenceScope, result: &AgentToolResult) {
    if let Some(cuts) = app.try_state::<PaneListingCuts>() {
        cuts.note_listing(scope, result);
    }
}

/// Count a staged plan if the folder it renames was last listed cut.
pub(super) fn count_plan_from_cut_listing<R: Runtime>(
    app: &AppHandle<R>,
    scope: EvidenceScope,
    plan: &RenameProposalSnapshot,
) {
    let Some(cuts) = app.try_state::<PaneListingCuts>() else {
        return;
    };
    if let Some(props) = cuts.cut_plan_props(scope, plan) {
        crate::analytics::events::capture("rename_plan_from_cut_listing", props);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tools::propose::evidence::{EvidenceSource, RenameEvidence};
    use crate::agent::tools::propose::rename::RenameProposalRowSnapshot;

    const THREAD: EvidenceScope = EvidenceScope::Thread(7);

    fn listing(path: &str, total: usize, returned: usize) -> AgentToolResult {
        AgentToolResult {
            call_id: "list-1".to_string(),
            content: json!({
                "pane": "left",
                "path": path,
                "volumeId": "root",
                "scope": "folder",
                "total": total,
                "returned": returned,
                "truncated": total > returned,
                "entries": [],
            }),
            elided: false,
        }
    }

    fn plan(folder: &str, rows: usize) -> RenameProposalSnapshot {
        RenameProposalSnapshot {
            proposal_id: "proposal-1".to_string(),
            rows: (0..rows)
                .map(|i| RenameProposalRowSnapshot {
                    row_id: format!("row-{i}"),
                    source_name: format!("IMG_{i}.jpg"),
                    destination_name: format!("Photo {i}.jpg"),
                    source_path: format!("{folder}/IMG_{i}.jpg"),
                    volume_id: "root".to_string(),
                    evidence: RenameEvidence {
                        source: EvidenceSource::Filename,
                        detail: format!("IMG_{i}.jpg"),
                    },
                    coverage: None,
                })
                .collect(),
        }
    }

    fn cuts_after(result: &AgentToolResult) -> PaneListingCuts {
        let cuts = PaneListingCuts::default();
        cuts.note_listing(THREAD, result);
        cuts
    }

    #[test]
    fn a_plan_from_a_cut_listing_reports_how_its_rows_compare() {
        let cuts = cuts_after(&listing("/shots", 5_000, 200));

        let props = cuts.cut_plan_props(THREAD, &plan("/shots", 150)).expect("counted");
        assert_eq!(props["coverage"], json!("within_returned"));
        assert_eq!(props["rows"], json!("101-1000"));
        assert_eq!(props["listing_returned"], json!("101-1000"));
        assert_eq!(props["listing_total"], json!("1000+"));

        let beyond = cuts.cut_plan_props(THREAD, &plan("/shots", 300)).expect("counted");
        assert_eq!(beyond["coverage"], json!("beyond_returned"));
    }

    /// The case step 2 would refuse: as many rows as the folder holds, from a listing that
    /// showed fewer.
    #[test]
    fn a_plan_with_a_row_for_every_file_is_its_own_token() {
        let cuts = cuts_after(&listing("/shots", 250, 200));

        let props = cuts.cut_plan_props(THREAD, &plan("/shots", 250)).expect("counted");
        assert_eq!(props["coverage"], json!("matches_total"));
    }

    #[test]
    fn a_whole_listing_counts_nothing() {
        let cuts = cuts_after(&listing("/shots", 40, 40));
        assert_eq!(cuts.cut_plan_props(THREAD, &plan("/shots", 40)), None);
    }

    #[test]
    fn a_plan_in_another_folder_counts_nothing() {
        let cuts = cuts_after(&listing("/shots", 5_000, 200));
        assert_eq!(cuts.cut_plan_props(THREAD, &plan("/elsewhere", 10)), None);
    }

    #[test]
    fn another_thread_or_no_thread_counts_nothing() {
        let cuts = cuts_after(&listing("/shots", 5_000, 200));
        let plan = plan("/shots", 10);
        assert_eq!(cuts.cut_plan_props(EvidenceScope::Thread(8), &plan), None);
        assert_eq!(cuts.cut_plan_props(EvidenceScope::NoThread, &plan), None);
    }

    /// The model saw only the latest listing it was handed, so that's the one a plan answers
    /// to; a later problem result is no listing and leaves it standing.
    #[test]
    fn the_latest_listing_wins_and_a_problem_result_does_not_erase_it() {
        let cuts = cuts_after(&listing("/shots", 5_000, 200));
        cuts.note_listing(THREAD, &listing("/shots", 40, 40));
        assert_eq!(cuts.cut_plan_props(THREAD, &plan("/shots", 40)), None);

        cuts.note_listing(THREAD, &listing("/shots", 5_000, 200));
        cuts.note_listing(
            THREAD,
            &AgentToolResult {
                call_id: "list-2".to_string(),
                content: json!({ "problem": "The focused pane's listing isn't available yet" }),
                elided: false,
            },
        );
        assert!(cuts.cut_plan_props(THREAD, &plan("/shots", 40)).is_some());
    }

    #[test]
    fn an_elided_listing_never_reached_the_model() {
        let mut result = listing("/shots", 5_000, 200);
        result.elided = true;
        let cuts = cuts_after(&result);
        assert_eq!(cuts.cut_plan_props(THREAD, &plan("/shots", 10)), None);
    }

    /// Every value is a bucket or a token: never the folder, a name, or a raw count.
    #[test]
    fn every_prop_value_is_a_short_token_or_bucket() {
        let cuts = cuts_after(&listing("/Users/dave/shots", 5_000, 200));
        let props = cuts
            .cut_plan_props(THREAD, &plan("/Users/dave/shots", 12))
            .expect("counted");
        for (key, value) in props.as_object().expect("props are an object") {
            let s = value.as_str().unwrap_or_else(|| panic!("{key} is a string token"));
            assert!(
                !s.contains('/') && !s.contains("dave") && s.len() <= 24,
                "prop '{key}' carries a non-categorical value: {s}"
            );
        }
    }
}
