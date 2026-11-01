//! What came back from a call, read off the result's own typed keys.

use serde::Deserialize;
use serde_json::Value;

use super::{IndexUnavailableReason, RefusalReason, ToolOutcome, count_u32, len_u32};
use crate::agent::llm::types::{AgentToolResult, ToolId};
use crate::agent::memory::MemoryRefusalKind;
use crate::location::miss::MissToken;
use crate::tools::connect::ConnectOutcome;

/// What `result` says happened. The check order is the contract:
///
/// 1. `elided`: a set-aside stub carries none of the result's keys, so it reads as a bare
///    answer. (Nothing persists one today; this is defensive.)
/// 2. `available: false`: the view gate.
/// 3. `repeatedCall`: the repeat breaker KEEPS the original `problem` beside it, so it has to
///    win over `problem`.
/// 4. `readyForReview` on a propose tool: `ensure_review_verdict` stamps `false` onto every
///    propose problem, so a propose problem is `NotStaged`, or `LocationMiss` when it's a
///    place that can't run now (it carries the read tools' `miss`).
/// 5. `problem`, then 6. `refused` (memory), then 7. `miss` (the location contract's typed
///    miss, any tool that takes a place), then the per-tool success shapes.
///
/// Total: a shape it doesn't recognize reads as [`ToolOutcome::Answered`].
pub fn outcome_of(tool: &ToolId, result: &AgentToolResult) -> ToolOutcome {
    if result.elided {
        return ToolOutcome::Answered;
    }
    let content = &result.content;
    if content.get("available") == Some(&Value::Bool(false)) {
        return refused(RefusalReason::ToolUnavailable);
    }
    if content.get("repeatedCall") == Some(&Value::Bool(true)) {
        return refused(RefusalReason::RepeatedCall);
    }
    if is_propose(tool)
        && let Some(ready) = content.get("readyForReview").and_then(Value::as_bool)
    {
        return if ready {
            staged(tool, content)
        } else if let Some(miss) = miss_of(content) {
            // A place that can't run now refuses in the read tools' miss shape.
            ToolOutcome::LocationMiss { miss }
        } else {
            ToolOutcome::NotStaged
        };
    }
    if content.get("problem").is_some() {
        return refused(RefusalReason::Problem);
    }
    if let Some(token) = content.get("refused").and_then(Value::as_str) {
        return refused(RefusalReason::Memory {
            refusal: MemoryRefusalKind::from_token(token),
        });
    }
    if let Some(miss) = miss_of(content) {
        return ToolOutcome::LocationMiss { miss };
    }
    answered(tool, content).unwrap_or(ToolOutcome::Answered)
}

/// The location contract's typed miss, when the result carries a token this build knows.
fn miss_of(content: &Value) -> Option<MissToken> {
    content.get("miss").and_then(|token| MissToken::deserialize(token).ok())
}

fn refused(reason: RefusalReason) -> ToolOutcome {
    ToolOutcome::Refused { reason }
}

fn is_propose(tool: &ToolId) -> bool {
    matches!(
        tool,
        ToolId::ProposeRenamePlan | ToolId::ProposeSuggestions | ToolId::ProposeServer
    )
}

/// A staged plan's count: the rename plan says `count`; a sweep sums its groups' `opCount`; a
/// server suggestion is one card (its group has no ops), unless the user answered it first.
fn staged(tool: &ToolId, content: &Value) -> ToolOutcome {
    let count = match tool {
        ToolId::ProposeSuggestions => content.get("groups").and_then(Value::as_array).map(|groups| {
            groups
                .iter()
                .filter_map(|group| group.get("opCount").and_then(Value::as_u64))
                .fold(0u32, |sum, n| sum.saturating_add(u32::try_from(n).unwrap_or(u32::MAX)))
        }),
        ToolId::ProposeServer => content.get("groups").and_then(Value::as_array).map(|groups| {
            len_u32(
                groups
                    .iter()
                    .filter(|group| group.get("outcome").and_then(Value::as_str) != Some("alreadyAnswered"))
                    .count(),
            )
        }),
        _ => count_u32(content.get("count")),
    };
    ToolOutcome::Staged {
        count: count.unwrap_or(0),
    }
}

/// The success shape per tool. `None` when the shape isn't the one expected.
fn answered(tool: &ToolId, content: &Value) -> Option<ToolOutcome> {
    match tool {
        ToolId::AppState
        | ToolId::FolderImportance
        | ToolId::OperationsGet
        | ToolId::NothingToSuggest
        | ToolId::ProposeRenamePlan
        | ToolId::ProposeSuggestions
        | ToolId::ProposeServer
        | ToolId::Unrecognized(_) => Some(ToolOutcome::Answered),
        ToolId::ListDir
        | ToolId::ListPaneFiles
        | ToolId::InspectFile
        | ToolId::OperationsList
        | ToolId::ListSuggestions => paged(content),
        ToolId::GetSuggestionGroup => match content.get("found") {
            Some(Value::Bool(false)) => Some(ToolOutcome::NothingFound { lower_bound: false }),
            _ => paged(content),
        },
        ToolId::ImportantFolders => {
            let folders = content.get("folders")?.as_array()?;
            let truncated = content.get("truncated").and_then(Value::as_bool).unwrap_or(false);
            Some(ToolOutcome::Items {
                returned: len_u32(folders.len()),
                total: (!truncated).then_some(folders.len() as u64),
                lower_bound: false,
            })
        }
        ToolId::ListVolumes => {
            let volumes = content.get("volumes")?.as_array()?;
            Some(ToolOutcome::Items {
                returned: len_u32(volumes.len()),
                total: Some(volumes.len() as u64),
                lower_bound: false,
            })
        }
        ToolId::Search => {
            let coverage = content.get("coverage");
            let complete = flag(coverage, "complete");
            let lower_bound = !complete || flag(coverage, "capped");
            matches_of(
                count_u32(content.get("returned"))?,
                content.get("matchCount")?.as_u64()?,
                lower_bound,
            )
        }
        ToolId::SearchPhotos => match content.get("status")?.as_str()? {
            "ok" => {
                let lower_bound = content
                    .get("coverage")
                    .and_then(Value::as_array)
                    .is_some_and(|volumes| {
                        volumes
                            .iter()
                            .any(|volume| flag(Some(volume), "incomplete") || flag(Some(volume), "indexing"))
                    });
                matches_of(
                    count_u32(content.get("returned"))?,
                    content.get("total")?.as_u64()?,
                    lower_bound,
                )
            }
            other => index_unavailable(other),
        },
        ToolId::ImageFacts => match content.get("status")?.as_str()? {
            "ok" => paged(content),
            other => index_unavailable(other),
        },
        ToolId::ConnectVolume => Some(ToolOutcome::Connection {
            outcome: ConnectOutcome::deserialize(content.get("outcome")?).ok()?,
        }),
        ToolId::MemoryWrite | ToolId::MemoryEdit => match content.get("saved") {
            Some(Value::Bool(true)) => Some(ToolOutcome::Saved),
            _ => None,
        },
    }
}

/// The paged-list contract every list tool shares: `returned` of `total`, a lower bound when a
/// capped live `list_dir` read only counted what it kept (`totalIsLowerBound`).
fn paged(content: &Value) -> Option<ToolOutcome> {
    Some(ToolOutcome::Items {
        returned: count_u32(content.get("returned"))?,
        total: content.get("total").and_then(Value::as_u64),
        lower_bound: flag(Some(content), "totalIsLowerBound"),
    })
}

fn matches_of(returned: u32, match_count: u64, lower_bound: bool) -> Option<ToolOutcome> {
    Some(if match_count == 0 {
        ToolOutcome::NothingFound { lower_bound }
    } else {
        ToolOutcome::Matches {
            returned,
            match_count,
            lower_bound,
        }
    })
}

/// A photo tool's typed "the index can't answer" status. The tokens are the result enum's own
/// serde tags (`SearchPhotosResult`, `ImageFactsResult`).
fn index_unavailable(status: &str) -> Option<ToolOutcome> {
    let reason = match status {
        "imageIndexingOff" => IndexUnavailableReason::ImageIndexingOff,
        "semanticModelNotInstalled" => IndexUnavailableReason::SemanticModelNotInstalled,
        _ => return None,
    };
    Some(ToolOutcome::IndexUnavailable { reason })
}

fn flag(object: Option<&Value>, key: &str) -> bool {
    object.and_then(|object| object.get(key)).and_then(Value::as_bool) == Some(true)
}
