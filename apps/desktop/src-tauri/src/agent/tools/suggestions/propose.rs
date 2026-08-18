//! The `propose_suggestions` tool: the one way the agent puts operations in front of the
//! user, and the one way it amends what it already proposed.
//!
//! `Access::Propose`. It stages rows in `main.db` and nothing else: no file moves, no
//! approval, no way to approve. Amend folds in here deliberately — a separate mutating
//! `amend` tool would be `Access::Write` under the registry's own tiebreaker and could never
//! be reachable from the agent's view.
//!
//! ## Nothing is staged until everything checks out
//!
//! Validation, selector resolution, and the ownership checks on an amendment all run BEFORE
//! the first write. A call whose second group names an already-approved group stages
//! nothing, because a half-applied sweep leaves the user reading a mix of what the agent
//! meant and what it managed.
//!
//! ## A selector resolves exactly once, here
//!
//! [`resolve_selector_ops`] runs against the drive index at creation and the resulting rows
//! ARE the proposal from then on. ❌ Nothing re-resolves at approval: freezing is what makes
//! "what the user saw is what runs" true.

use rusqlite::Connection;
use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Manager, Runtime};

use super::input::{
    ExplicitNaming, GroupProblem, MAX_GROUPS, MAX_PATHS, PlanRefusal, PlannedGroup, PlannedOps, PlannedSources,
    PlannedSweep, plan_sweep,
};
use crate::agent::AgentDb;
use crate::agent::store::AgentStoreError;
use crate::agent::store::proposals::{
    GroupIntent, NewGroup, NewOp, NewSweep, ReproposeOutcome, create_sweep, get_group, get_sweep,
};
use crate::agent::suggested_ops::{
    DriveIndex, OpSelector, SelectorIndex, SelectorRefusal, resolve_selector_ops, selector_group,
};
use crate::agent::types::{ProposalStatus, ProposalVerb};
use crate::mcp::{ToolError, ToolResult};

// ── The tool boundary ─────────────────────────────────────────────────────────

/// The schema, kept TERSE on purpose: it rides in the cached prefix of every turn, so a
/// sentence here is paid for on calls that never propose anything. The guidance a caller
/// can't infer from the field names (a selector is how a large set is proposed, a folder is
/// one op, there is no last-opened predicate) lives in the registry description and the
/// system prompt, each said once.
pub fn propose_suggestions_schema() -> Value {
    let location = |what: &str| {
        json!({
            "type": "object",
            "description": what,
            "properties": {
                "volumeId": { "type": "string" },
                "path": { "type": "string" }
            },
            "required": ["volumeId", "path"],
            "additionalProperties": false
        })
    };
    json!({
        "type": "object",
        "properties": {
            "sweepId": { "type": "integer", "description": "Add to or amend this sweep. Required with any groupId." },
            "rationale": { "type": "string", "description": "Your reason for the sweep, shown as your words." },
            "groups": {
                "type": "array",
                "minItems": 1,
                "maxItems": MAX_GROUPS,
                "description": "Each group is approved or rejected on its own.",
                "items": {
                    "type": "object",
                    "properties": {
                        "groupId": { "type": "integer", "description": "A pending group of this sweep to rewrite whole." },
                        "verb": {
                            "type": "string",
                            "enum": ["move", "copy", "trash", "delete", "rename", "compress", "extract"],
                            "description": "delete is permanent; trash isn't."
                        },
                        "destination": location("Destination folder (move, copy, extract) or archive path (compress)."),
                        "overwritesExisting": { "type": "boolean", "description": "compress only: that archive exists, so the group can't be undone." },
                        "parent": { "type": "string", "description": "rename only: the folder every source shares." },
                        "sourceVolumeId": { "type": "string", "description": "The sources' volume. With paths or renames; a selector supplies its own." },
                        "displayName": { "type": "string", "description": "The group's title for the user. With paths or renames; a selector is named by its pattern." },
                        "rationale": { "type": "string", "description": "Why this group, shown as your words." },
                        "paths": {
                            "type": "array",
                            "items": { "type": "string" },
                            "maxItems": MAX_PATHS,
                            "description": "Sources by absolute path. A folder's path is ONE op."
                        },
                        "renames": {
                            "type": "array",
                            "maxItems": MAX_PATHS,
                            "description": "rename only: each source and the bare name it becomes.",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "path": { "type": "string" },
                                    "newName": { "type": "string" }
                                },
                                "required": ["path", "newName"],
                                "additionalProperties": false
                            }
                        },
                        "selector": {
                            "type": "object",
                            "description": "Propose over a pattern instead of a list. Not for rename.",
                            "properties": {
                                "root": location("The subtree to search."),
                                "nameGlob": { "type": "string", "description": "Glob over the file name, such as *.dmg." },
                                "minSizeBytes": { "type": "integer" },
                                "maxSizeBytes": { "type": "integer" },
                                "olderThanDays": { "type": "integer", "description": "Modified more than this many days ago." },
                                "newerThanDays": { "type": "integer", "description": "Modified within this many days." }
                            },
                            "required": ["root"],
                            "additionalProperties": false
                        }
                    },
                    "required": ["verb"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["groups"],
        "additionalProperties": false
    })
}

/// The registry path (an external MCP client), which has no chat thread to attribute the
/// sweep to.
pub async fn execute_propose_suggestions<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    propose_in_thread(app, None, params).await
}

/// Stage a sweep, attributing it to the chat thread that asked for it. The link is nullable
/// and `ON DELETE SET NULL`, so tidying the thread away later leaves the decision record
/// whole.
pub(crate) async fn propose_in_thread<R: Runtime>(
    app: &AppHandle<R>,
    conversation_id: Option<i64>,
    params: &Value,
) -> ToolResult {
    let now = now_secs();
    let planned = match plan_sweep(params, now) {
        Ok(planned) => planned,
        Err(refusal) => return Ok(plan_refusal_content(&refusal)),
    };
    let db_path = app
        .try_state::<AgentDb>()
        .ok_or_else(|| ToolError::internal("Cmdr's suggestion store isn't open yet."))?
        .db_path()
        .to_path_buf();

    let outcome = tokio::task::spawn_blocking(move || {
        let conn = crate::agent::store::open_write_connection(&db_path).map_err(ApplyRefusal::Store)?;
        apply_planned_sweep(&conn, &DriveIndex, planned, conversation_id, now)
    })
    .await
    .map_err(|e| ToolError::internal(e.to_string()))?;

    match outcome {
        Ok(report) => serde_json::to_value(&report).map_err(|e| ToolError::internal(e.to_string())),
        Err(ApplyRefusal::Store(e)) => Err(ToolError::internal(e.to_string())),
        Err(ApplyRefusal::Internal(detail)) => Err(ToolError::internal(detail)),
        Err(refusal) => Ok(apply_refusal_content(&refusal)),
    }
}

fn now_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ── What comes back ───────────────────────────────────────────────────────────

/// What one group of a successful call became.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum GroupOutcome {
    /// A new group in the sweep.
    Created,
    /// An existing pending group whose op list was replaced.
    Amended,
    /// It left `pending` between the check and the write (the user answered it just now), so
    /// it kept the answer the user gave.
    AlreadyAnswered,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GroupReport {
    pub group_id: i64,
    pub verb: ProposalVerb,
    pub display_name: String,
    pub op_count: usize,
    pub outcome: GroupOutcome,
}

/// A staged sweep, as the model reads it back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProposeReport {
    /// Always `true` here: the user reviews and decides, and no tool approves anything.
    pub ready_for_review: bool,
    pub sweep_id: i64,
    pub groups: Vec<GroupReport>,
}

// ── Refusals ──────────────────────────────────────────────────────────────────

/// Why a call staged nothing once it got past its own shape.
#[derive(Debug)]
pub(super) enum ApplyRefusal {
    UnknownSweep {
        sweep_id: i64,
    },
    UnknownGroup {
        group: usize,
        group_id: i64,
    },
    /// The group left `pending`, so it's the user's now.
    GroupNotPending {
        group: usize,
        group_id: i64,
        status: ProposalStatus,
    },
    GroupNotInSweep {
        group: usize,
        group_id: i64,
        sweep_id: i64,
    },
    /// The index couldn't answer the selector. A refusal is NOT an empty list.
    Selector {
        group: usize,
        refusal: SelectorRefusal,
    },
    /// The selector resolved to nothing. Refused rather than staged, because an empty group
    /// costs the user a review that can't contain anything.
    SelectorMatchedNothing {
        group: usize,
        pattern: String,
    },
    Store(AgentStoreError),
    /// Something that can't happen in practice did (a selector that won't serialize).
    /// Surfaced as a tool error, never as advice to the model.
    Internal(String),
}

/// The typed refusal the model reads: a token it can branch on, the group it's about, and a
/// sentence saying what to send instead. ❌ Nothing downstream matches on the sentence.
fn refusal_content(token: &str, group: Option<usize>, problem: String) -> Value {
    let mut content = json!({ "readyForReview": false, "refusal": token, "problem": problem });
    if let Some(index) = group {
        content["group"] = json!(index);
    }
    content
}

fn plan_refusal_content(refusal: &PlanRefusal) -> Value {
    match refusal {
        PlanRefusal::Malformed => refusal_content(
            "malformed",
            None,
            "That isn't the shape this tool takes. Send groups, each with a verb and either paths, renames, or a selector.".into(),
        ),
        PlanRefusal::NoGroups => refusal_content("noGroups", None, "A sweep needs at least one group.".into()),
        PlanRefusal::TooManyGroups { sent } => refusal_content(
            "tooManyGroups",
            None,
            format!(
                "{} is more than one sitting's worth of review. Send at most {MAX_GROUPS}, and follow up with the rest.",
                crate::pluralize::pluralize(*sent as u64, "group")
            ),
        ),
        PlanRefusal::GroupIdWithoutSweep { group } => refusal_content(
            "groupIdWithoutSweep",
            Some(*group),
            "Amending a group needs the sweepId it belongs to as well.".into(),
        ),
        PlanRefusal::Group { group, problem } => {
            let (token, sentence) = group_problem_content(problem);
            refusal_content(token, Some(*group), sentence)
        }
    }
}

fn group_problem_content(problem: &GroupProblem) -> (&'static str, String) {
    match problem {
        GroupProblem::NoSources => (
            "noSources",
            "This group names nothing to act on. Give paths, renames, or a selector.".into(),
        ),
        GroupProblem::AmbiguousSources => (
            "ambiguousSources",
            "Give exactly one of paths, renames, or a selector, so it's clear what the user is reviewing.".into(),
        ),
        GroupProblem::RenamesVerbMismatch => (
            "renamesVerbMismatch",
            "renames belongs to the rename verb and to no other, and a rename group needs them.".into(),
        ),
        GroupProblem::SelectorCantRename => (
            "selectorCantRename",
            "A selector matches files, so it can't say what they should be called. List the renames.".into(),
        ),
        GroupProblem::UnboundField { field } => (
            "unboundField",
            format!("This verb doesn't take {field}, so leave it out."),
        ),
        GroupProblem::MissingField { field } => ("missingField", format!("This group needs {field}.")),
        GroupProblem::SelectorSuppliesField { field } => (
            "selectorSuppliesField",
            format!("A selector already supplies {field} from its own root and pattern, so leave it out."),
        ),
        GroupProblem::EmptySources => (
            "emptySources",
            "This group's source list is empty, so there's nothing to review.".into(),
        ),
        GroupProblem::TooManyPaths { sent } => (
            "tooManyPaths",
            format!(
                "{} is past the {MAX_PATHS} one group may name. Describe them with a selector instead: Cmdr resolves it here and the user still reviews every file it matched.",
                crate::pluralize::pluralize(*sent as u64, "path")
            ),
        ),
        GroupProblem::RelativePath { path } => (
            "relativePath",
            format!("{path} isn't an absolute path, and a proposal has no folder to be relative to."),
        ),
        GroupProblem::NotABareName { name } => (
            "notABareName",
            format!("{name} has to be a bare file name; a rename can't move a file to another folder."),
        ),
        GroupProblem::ImpossibleWindow => (
            "impossibleWindow",
            "That size or age window can't match anything. Check which bound is which.".into(),
        ),
    }
}

fn apply_refusal_content(refusal: &ApplyRefusal) -> Value {
    match refusal {
        ApplyRefusal::UnknownSweep { sweep_id } => refusal_content(
            "unknownSweep",
            None,
            format!("There's no sweep {sweep_id}. Call list_suggestions to see what's waiting."),
        ),
        ApplyRefusal::UnknownGroup { group, group_id } => {
            refusal_content("unknownGroup", Some(*group), format!("There's no group {group_id}."))
        }
        ApplyRefusal::GroupNotPending {
            group,
            group_id,
            status,
        } => refusal_content(
            "groupNotPending",
            Some(*group),
            format!(
                "Group {group_id} is {}, so it's the user's now and can't be rewritten. Propose a new group instead.",
                status.as_token()
            ),
        ),
        ApplyRefusal::GroupNotInSweep {
            group,
            group_id,
            sweep_id,
        } => refusal_content(
            "groupNotInSweep",
            Some(*group),
            format!("Group {group_id} isn't part of sweep {sweep_id}."),
        ),
        ApplyRefusal::Selector { group, refusal } => {
            let sentence = match refusal {
                SelectorRefusal::NotIndexed { volume_id } => format!(
                    "Cmdr has no index for volume {volume_id}, so it can't tell what the pattern matches. That's different from nothing matching: say so rather than reporting an empty result."
                ),
                SelectorRefusal::NotOnVolume { volume_id, path } => {
                    format!("{path} isn't on volume {volume_id}.")
                }
                SelectorRefusal::RootNotFound { path } => {
                    format!("{path} isn't in the index yet, so the pattern has nothing to look through.")
                }
                SelectorRefusal::BadPattern { .. } => {
                    "That name pattern didn't compile. Use a plain glob such as *.dmg.".into()
                }
                SelectorRefusal::IndexUnavailable { .. } => {
                    "Cmdr couldn't read the index just now, so the pattern went unresolved.".into()
                }
            };
            refusal_content("selector", Some(*group), sentence)
        }
        ApplyRefusal::SelectorMatchedNothing { group, pattern } => refusal_content(
            "selectorMatchedNothing",
            Some(*group),
            format!("Nothing in the index matches {pattern}, so there'd be nothing for the user to review."),
        ),
        // Both surface as tool errors; these arms exist only to keep the match total.
        ApplyRefusal::Store(e) => refusal_content("storeUnavailable", None, e.to_string()),
        ApplyRefusal::Internal(detail) => refusal_content("storeUnavailable", None, detail.clone()),
    }
}

// ── The write path ────────────────────────────────────────────────────────────

/// One group with its ops resolved: everything needed to write it, and nothing left to
/// decide.
struct ResolvedGroup {
    group_id: Option<i64>,
    group: NewGroup,
}

/// Resolve, check, then write. The order is the contract: a selector is resolved and an
/// amendment's target is checked BEFORE the first row is written, so a refusal leaves the
/// store exactly as it was.
pub(super) fn apply_planned_sweep(
    conn: &Connection,
    index: &dyn SelectorIndex,
    planned: PlannedSweep,
    conversation_id: Option<i64>,
    now: i64,
) -> Result<ProposeReport, ApplyRefusal> {
    let sweep_id = planned.sweep_id;
    let mut resolved = Vec::with_capacity(planned.groups.len());
    for (index_of, group) in planned.groups.into_iter().enumerate() {
        if let Some(group_id) = group.group_id {
            check_amendable(conn, index_of, group_id, sweep_id)?;
        }
        resolved.push(resolve_group(index, index_of, group)?);
    }
    if let Some(sweep_id) = sweep_id
        && get_sweep(conn, sweep_id).map_err(ApplyRefusal::Store)?.is_none()
    {
        return Err(ApplyRefusal::UnknownSweep { sweep_id });
    }

    let sweep_id = match sweep_id {
        Some(existing) => existing,
        None => create_sweep(
            conn,
            &NewSweep {
                conversation_id,
                created_by_model: None,
                rationale: planned.rationale,
            },
            now,
        )
        .map_err(ApplyRefusal::Store)?,
    };

    let mut groups = Vec::with_capacity(resolved.len());
    for entry in resolved {
        groups.push(write_group(conn, sweep_id, entry, now)?);
    }
    Ok(ProposeReport {
        ready_for_review: true,
        sweep_id,
        groups,
    })
}

/// Whether a group may be rewritten: it exists, it belongs to the named sweep, and it is
/// still `pending`. `approved`, `interrupted`, `completed`, and `rejected` are the user's.
fn check_amendable(
    conn: &Connection,
    group_index: usize,
    group_id: i64,
    sweep_id: Option<i64>,
) -> Result<(), ApplyRefusal> {
    let group = get_group(conn, group_id)
        .map_err(ApplyRefusal::Store)?
        .ok_or(ApplyRefusal::UnknownGroup {
            group: group_index,
            group_id,
        })?;
    if let Some(sweep_id) = sweep_id
        && group.set_id != sweep_id
    {
        return Err(ApplyRefusal::GroupNotInSweep {
            group: group_index,
            group_id,
            sweep_id,
        });
    }
    if group.status != ProposalStatus::Pending {
        return Err(ApplyRefusal::GroupNotPending {
            group: group_index,
            group_id,
            status: group.status,
        });
    }
    Ok(())
}

/// Turn a validated group into rows, resolving a selector against the drive index on the
/// way. This is the freeze: from here on the rows are the proposal.
fn resolve_group(
    index: &dyn SelectorIndex,
    group_index: usize,
    planned: PlannedGroup,
) -> Result<ResolvedGroup, ApplyRefusal> {
    let group: NewGroup = match planned.ops {
        PlannedOps::Rename {
            parent,
            renames,
            naming,
        } => new_group(GroupIntent::Rename { parent, renames }, naming, planned.rationale),
        PlannedOps::Sources { shape, sources } => match sources {
            PlannedSources::Paths { paths, naming } => {
                let ops = paths
                    .into_iter()
                    .map(|source_path| NewOp {
                        source_path,
                        snapshot: None,
                    })
                    .collect();
                new_group(shape.into_intent(ops), naming, planned.rationale)
            }
            PlannedSources::Selector(selector) => {
                let ops = resolve_selector(index, group_index, &selector)?;
                // `selector_group` is the one place that decides how a pattern names its
                // group and which volume it binds, so a selector group is built there
                // rather than assembled twice.
                selector_group(&selector, shape.into_intent(ops), planned.rationale)
                    .map_err(|e| ApplyRefusal::Internal(e.to_string()))?
            }
        },
    };
    Ok(ResolvedGroup {
        group_id: planned.group_id,
        group,
    })
}

/// Resolve one selector, distinguishing "I can't see that drive" from "nothing matched" —
/// they read the same as an empty list and mean opposite things.
fn resolve_selector(
    index: &dyn SelectorIndex,
    group_index: usize,
    selector: &OpSelector,
) -> Result<Vec<NewOp>, ApplyRefusal> {
    let ops = resolve_selector_ops(index, selector).map_err(|refusal| ApplyRefusal::Selector {
        group: group_index,
        refusal,
    })?;
    if ops.is_empty() {
        return Err(ApplyRefusal::SelectorMatchedNothing {
            group: group_index,
            pattern: selector.pattern_text(),
        });
    }
    Ok(ops)
}

/// The store's group shape for a group the model listed by hand: it carries no selector,
/// because there was no pattern behind it.
fn new_group(intent: GroupIntent, naming: ExplicitNaming, rationale: Option<String>) -> NewGroup {
    NewGroup {
        intent,
        source_volume_id: naming.source_volume_id,
        display_name: naming.display_name,
        rationale,
        selector: None,
    }
}

/// Write one group: a fresh one, or a replacement for a pending one. A group that left
/// `pending` between the check and this write keeps the answer the user gave it, and the
/// report says so.
fn write_group(conn: &Connection, sweep_id: i64, entry: ResolvedGroup, now: i64) -> Result<GroupReport, ApplyRefusal> {
    let verb = entry.group.intent.verb();
    let op_count = entry.group.intent.op_count();
    let display_name = entry.group.display_name.clone();
    let (group_id, outcome) = match entry.group_id {
        Some(group_id) => {
            let outcome = crate::agent::suggested_ops::repropose(conn, group_id, &entry.group, now)
                .map_err(ApplyRefusal::Store)?;
            match outcome {
                ReproposeOutcome::Reproposed => (group_id, GroupOutcome::Amended),
                ReproposeOutcome::NotPending { .. } | ReproposeOutcome::Unknown => {
                    (group_id, GroupOutcome::AlreadyAnswered)
                }
            }
        }
        None => (
            crate::agent::suggested_ops::add_group(conn, sweep_id, &entry.group, now).map_err(ApplyRefusal::Store)?,
            GroupOutcome::Created,
        ),
    };
    Ok(GroupReport {
        group_id,
        verb,
        display_name,
        op_count: op_count_of(&outcome, op_count),
        outcome,
    })
}

/// A group that was already answered kept its old op list, so reporting the count this call
/// carried would describe rows that were never written.
fn op_count_of(outcome: &GroupOutcome, proposed: usize) -> usize {
    match outcome {
        GroupOutcome::AlreadyAnswered => 0,
        _ => proposed,
    }
}
