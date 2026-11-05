//! The `propose_rename_plan` tool boundary: schema in, staged proposal or typed refusal
//! out.
//!
//! Everything a plan must survive before a single row is staged lives here, in order: each
//! row's own checks and the one-volume, one-folder rule (`rows.rs`), the evidence check, the
//! folder's placement on a connected volume (the location resolver), and one bounded stat per
//! source (`sources.rs`). A plan may name any folder on any connected volume; no pane has to
//! show it. Every refusal names the rows it means, and one bad row refuses the WHOLE plan, so
//! the user never sees a partial plan they'd read as complete.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;
use tauri::{AppHandle, Manager, Runtime};

use super::rows::{RowProblem, RowRejection, most_common, own_problem, rows_rejected_content, rows_rejected_reason};
use super::store::{RenameDraft, RenameDraftRow, RenameProposalSnapshot};
use crate::agent::AgentDb;
use crate::agent::llm::types::AgentToolResult;
use crate::location::Location;
use crate::location::miss::{LocationMissResult, volume_not_connected};
use crate::location::resolve::{LocationInput, VolumeFacts, resolve};
use crate::tools::params::{ParamViolation, check_object, list_names};
use crate::tools::propose::evidence::{EvidenceRejection, EvidenceScope, ImageFactsLedger};
use crate::tools::{ToolError, ToolResult};
use cmdr_fs::validation::validate_filename;

pub(super) const MAX_RENAMES: usize = 200;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RenamePlanInput {
    renames: Vec<RenameInput>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct RenameInput {
    source_path: String,
    volume_id: String,
    destination_name: String,
    /// Required: what this name is based on. A row can't be staged without it, so a
    /// content-derived name always carries something the backend can check and the user
    /// can read.
    evidence: crate::tools::propose::evidence::RenameEvidence,
}

pub struct RenameDispatchOutcome {
    pub result: AgentToolResult,
    pub proposal: Option<RenameProposalSnapshot>,
}

pub fn propose_rename_plan_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": { "renames": { "type": "array", "items": { "type": "object", "properties": {
            "sourcePath": { "type": "string" }, "volumeId": { "type": "string" }, "destinationName": { "type": "string" },
            "evidence": {
                "type": "object",
                "description": "What the name is based on; the user sees it, and a content claim is checked against what image_facts returned to you.",
                "properties": {
                    "source": {
                        "type": "string",
                        "enum": ["imageText", "imageTags", "filename", "metadata", "userInstruction"],
                        "description": "imageText or imageTags only when image_facts returned that content for this exact path in this conversation; else filename (the old name), metadata (dates, size), or userInstruction (a rule the user gave)."
                    },
                    "detail": {
                        "type": "string",
                        "description": "Up to 160 characters. imageText: a VERBATIM quote of at least 12 characters (a phrase, not one word) from what image_facts returned for this path; imageTags: a tag it returned; else the concrete detail used, like 'Taken 2026-07-20'."
                    }
                },
                "required": ["source", "detail"], "additionalProperties": false
            }
        }, "required": ["sourcePath", "volumeId", "destinationName", "evidence"], "additionalProperties": false }, "maxItems": MAX_RENAMES } },
        "required": ["renames"], "additionalProperties": false
    })
}

/// Why a proposal call didn't stage anything. Each kind keeps its own typed shape so the model
/// reads which rows to fix rather than one flattened sentence:
///
/// - `Problem`: plan-wide, no row to name (no rows, too many, the store isn't there).
/// - `Rows`: every row that can't be staged, by source path and typed problem.
/// - `Evidence`: every row whose evidence didn't check out.
/// - `Location`: the plan's folder can't be placed on a connected volume, in the read tools'
///   miss shape (`location/miss.rs`), so it carries the same `nearby` next moves.
pub(super) enum ProposalRefusal {
    Problem(ToolError),
    Rows(Vec<RowRejection>),
    Evidence(Vec<EvidenceRejection>),
    Location(Box<LocationMissResult>),
}

impl From<ToolError> for ProposalRefusal {
    fn from(error: ToolError) -> Self {
        ProposalRefusal::Problem(error)
    }
}

/// What the model reads when a plan is refused. Typed variants, actionable wording, and
/// every offending item listed — a refused item is never silently dropped.
pub(super) fn refusal_content(refusal: &ProposalRefusal) -> Value {
    match refusal {
        ProposalRefusal::Problem(error) => serde_json::json!({ "readyForReview": false, "problem": error.message }),
        ProposalRefusal::Rows(rejections) => rows_rejected_content(rejections),
        ProposalRefusal::Location(miss) => {
            let mut content = serde_json::to_value(miss.as_ref()).unwrap_or_else(|_| serde_json::json!({}));
            content["readyForReview"] = serde_json::json!(false);
            content["refusal"] = serde_json::json!("location");
            content
        }
        ProposalRefusal::Evidence(rejections) => serde_json::json!({
            "readyForReview": false,
            "evidenceRejected": rejections,
            "guidance": "Nothing was staged, so fix these rows and send the whole plan again. A name based on what's inside an image needs image_facts to have returned that content for that exact path, and the detail must quote it. If you don't have the content, say so and name the file from its old name, its dates, or what the user asked for.",
        }),
    }
}

/// One refusal as a log line: which typed variant, and enough of the offending rows to work
/// from. Reads the variant, never the wording, and names paths (the log already carries them
/// freely) without the proposed NAMES, which is where a model's reading of file contents would
/// sit.
pub(super) fn refusal_reason(refusal: &ProposalRefusal) -> String {
    match refusal {
        ProposalRefusal::Problem(error) => error.message.clone(),
        ProposalRefusal::Rows(rejections) => rows_rejected_reason(rejections, MAX_NAMED_ROWS),
        ProposalRefusal::Location(miss) => format!("the plan's folder can't be placed ({:?})", miss.miss),
        ProposalRefusal::Evidence(rejections) => {
            let named: Vec<String> = rejections
                .iter()
                .take(MAX_NAMED_ROWS)
                .map(|r| format!("{} ({:?})", r.source_path, r.problem))
                .collect();
            let rest = match rejections.len().saturating_sub(named.len()) {
                0 => String::new(),
                more => format!(" and {more} more"),
            };
            let count = rejections.len();
            let noun = if count == 1 { "row" } else { "rows" };
            format!(
                "{count} {noun} carried evidence that didn't check out: {}{rest}",
                named.join(", ")
            )
        }
    }
}

pub async fn dispatch<R: Runtime>(
    app: &AppHandle<R>,
    scope: EvidenceScope,
    call_id: &str,
    params: &Value,
) -> RenameDispatchOutcome {
    let outcome = match build_draft(app, scope, params).await {
        Ok(draft) => stage_draft(app, scope, &draft),
        Err(refusal) => Err(refusal),
    };
    match outcome {
        Ok(snapshot) => {
            super::cut_listing::count_plan_from_cut_listing(app, scope, &snapshot);
            RenameDispatchOutcome {
                result: AgentToolResult {
                    call_id: call_id.to_string(),
                    content: serde_json::json!({ "readyForReview": true, "count": snapshot.rows.len() }),
                    elided: false,
                },
                proposal: Some(snapshot),
            }
        }
        Err(refusal) => {
            // A refused plan stages nothing, and until this line it said so nowhere the user
            // or a maintainer could see: the panel read "nothing is waiting for you", the log
            // held only the provider round trips, and characterizing one real report meant
            // reading the conversation rows out of `main.db`. One line, at the one place a
            // plan dies.
            log::warn!(target: "agent::propose", "a rename plan was refused, so nothing was staged: {}", refusal_reason(&refusal));
            RenameDispatchOutcome {
                result: AgentToolResult {
                    call_id: call_id.to_string(),
                    content: refusal_content(&refusal),
                    elided: false,
                },
                proposal: None,
            }
        }
    }
}

/// Write the plan into `main.db` as one group in a sweep of its own, and answer the snapshot
/// the review dialog opens on.
///
/// The proposal outlives the turn that made it and has no expiry, so the store — not this
/// process — is what the review, the preflight, and the apply all read from afterwards.
fn stage_draft<R: Runtime>(
    app: &AppHandle<R>,
    scope: EvidenceScope,
    draft: &RenameDraft,
) -> Result<RenameProposalSnapshot, ProposalRefusal> {
    let unavailable = || ToolError::internal("The proposal store isn't available, so nothing was staged.");
    let db = app.try_state::<AgentDb>().ok_or_else(unavailable)?;
    let conn = db.open_write_connection().map_err(|e| {
        log::warn!(target: "agent::propose", "staging a rename proposal couldn't open main.db: {e}");
        unavailable()
    })?;
    super::store::stage(&conn, scope.conversation_id(), draft, now_secs())
        .map_err(|e| {
            log::warn!(target: "agent::propose", "staging a rename proposal didn't land: {e}");
            unavailable()
        })?
        .ok_or_else(unavailable)
        .map_err(ProposalRefusal::from)
}

fn now_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub async fn execute_propose_rename_plan<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    // The shared registry path has no chat thread, so it can cite no delivered facts.
    let outcome = dispatch(app, EvidenceScope::NoThread, "registry", params).await;
    Ok(outcome.result.content)
}

/// Record what an `image_facts` result delivered, so a later plan can cite it. Called by
/// the agent dispatcher; a result the runtime already marked elided never lands, because
/// the model didn't read it.
pub fn note_image_facts_delivered<R: Runtime>(app: &AppHandle<R>, scope: EvidenceScope, result: &AgentToolResult) {
    if result.elided {
        return;
    }
    if let Some(ledger) = app.try_state::<ImageFactsLedger>() {
        ledger.record_delivered(scope, &result.call_id, &result.content);
    }
}

/// Withdraw what the named `image_facts` calls delivered: prompt assembly dropped those
/// results, so the model never read them and no plan may cite their contents. The mirror of
/// [`note_image_facts_delivered`], called by the runtime's dispatch seam.
pub fn revoke_image_facts_evidence<R: Runtime>(app: &AppHandle<R>, call_ids: &[String]) {
    if let Some(ledger) = app.try_state::<ImageFactsLedger>() {
        for call_id in call_ids {
            ledger.revoke_call(call_id);
        }
    }
}

/// A plain param refusal, pre-wrapped so the boundary's `return Err(...)` sites stay
/// readable next to the typed evidence refusal.
fn invalid_params(message: impl Into<String>) -> ProposalRefusal {
    ProposalRefusal::Problem(ToolError::invalid_params(message))
}

/// The most rows one violation names before it counts the rest, so a 200-row plan can't turn
/// one missing field into a wall of numbers.
const MAX_NAMED_ROWS: usize = 5;

/// Why the `renames` array doesn't match the row shape the schema declares: which rows, and
/// which field. `None` when every row's SHAPE checks out, whatever else may be wrong with the
/// plan.
///
/// It exists because the refusal a model reads has to be actionable. One fixed sentence
/// listing all four field names told a model nothing it didn't already believe it had done,
/// and it re-sent the identical call until the turn's budget ran out
/// (`chat/runtime/repeats.rs` now stops that too, but a refusal earning the retry is the
/// better half of the fix).
///
/// The row shape comes out of [`propose_rename_plan_schema`] itself rather than a second list
/// here, so this and the prompt can never disagree about what a row takes.
pub(super) fn rows_problem(params: &Value) -> Option<String> {
    let schema = propose_rename_plan_schema();
    let row_schema = schema.get("properties")?.get("renames")?.get("items")?;
    let rows = params.get("renames")?.as_array()?;

    // Keyed by the violation so a plan that repeats one mistake reads as one sentence, and
    // ordered by the field name for a stable refusal.
    let mut by_violation: BTreeMap<ParamViolation, Vec<usize>> = BTreeMap::new();
    let mut accepted = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let problems = check_object(row_schema, row);
        if accepted.is_empty() {
            accepted = problems.accepted.clone();
        }
        for violation in problems.violations {
            by_violation.entry(violation).or_default().push(index + 1);
        }
    }
    if by_violation.is_empty() {
        return None;
    }
    let clauses: Vec<String> = by_violation
        .into_iter()
        .map(|(violation, at)| describe_rows(&violation, &at, rows.len()))
        .collect();
    Some(format!(
        "{}. A row takes {}.",
        join_clauses(&clauses),
        list_names(&accepted)
    ))
}

/// One violation as a clause naming the rows it applies to: `every rename row needs
/// volumeId`, or `rename rows 2 and 7 have no note parameters`. Lower case, because a clause
/// can land second in the sentence; [`join_clauses`] capitalizes the one that leads.
fn describe_rows(violation: &ParamViolation, at: &[usize], total: usize) -> String {
    let subject = if at.len() == total && total > 1 {
        "every rename row".to_string()
    } else if at.len() == 1 {
        format!("rename row {}", at[0])
    } else {
        format!("rename rows {}", join_rows(at))
    };
    let plural = at.len() > 1 && at.len() != total;
    match violation {
        // allowed-pluralize-noun: `needs` is the verb and `field` is one property name, never a count plus a noun
        ParamViolation::Missing(field) => format!("{subject} needs {field}"),
        ParamViolation::Unknown(field) => {
            let noun = if plural { "parameters" } else { "parameter" };
            format!("{subject} has no {field} {noun}")
        }
        // A row schema declares no arrays of its own, so nothing here can be hoisted out of
        // one. Rendered rather than collapsed into `Unknown` so a row schema that grows an
        // array later still reads honestly.
        ParamViolation::Misplaced { property, rows } => {
            format!("{subject} has no {property} parameter; each {rows} row takes {property}")
        }
    }
}

fn join_rows(at: &[usize]) -> String {
    let shown: Vec<String> = at.iter().take(MAX_NAMED_ROWS).map(usize::to_string).collect();
    match at.len().checked_sub(MAX_NAMED_ROWS) {
        Some(rest) if rest > 0 => format!("{} and {rest} more", shown.join(", ")),
        _ => list_names(&shown),
    }
}

/// Each clause carries its own subject, so they join with a comma even at two, and the one
/// that leads gets the capital: `Rename row 2 has no note parameter, and rename row 3 needs
/// destinationName`.
fn join_clauses(clauses: &[String]) -> String {
    let joined = match clauses {
        [rest @ .., last] if !rest.is_empty() => format!("{}, and {last}", rest.join(", ")),
        [one] => one.clone(),
        _ => String::new(),
    };
    let mut chars = joined.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => joined,
    }
}

/// Everything before the write, in order: the pure checks ([`plan_draft`]), then one bounded
/// stat per source on the plan's volume ([`super::sources::check_sources`]). The only async
/// step is that stat, and its shared deadline is what keeps a dead mount from hanging the turn.
async fn build_draft<R: Runtime>(
    app: &AppHandle<R>,
    evidence_scope: EvidenceScope,
    params: &Value,
) -> Result<RenameDraft, ProposalRefusal> {
    let facts = VolumeFacts::now().await;
    // An empty ledger IS the fail-closed ledger, so a missing registration refuses every
    // content claim rather than waving it through.
    let no_facts = ImageFactsLedger::default();
    let registered = app.try_state::<ImageFactsLedger>();
    let ledger = registered.as_deref().unwrap_or(&no_facts);
    let mut draft = plan_draft(params, &facts, ledger, evidence_scope)?;
    // The facts said live a moment ago, so a miss here is a disconnect in between.
    let volume = crate::file_system::volume::manager::get_volume_manager()
        .get(&draft.volume_id)
        .ok_or_else(|| {
            invalid_params("The volume disconnected while the plan was being checked. Call list_volumes, then send the plan again.")
        })?;
    let deadline = crate::tools::suggestions::snapshot::snapshot_deadline();
    super::sources::check_sources(volume, &mut draft.rows, &deadline)
        .await
        .map_err(ProposalRefusal::Rows)?;
    Ok(draft)
}

/// Parse, check, and place a plan, with no I/O: each row's own checks and the one-volume,
/// one-folder rule, then the evidence, then the folder's place. Every row-level problem is
/// collected over the whole plan before it refuses. Pure over `facts` and the ledger, so a
/// test needs no app and no registered volume.
pub(super) fn plan_draft(
    params: &Value,
    facts: &VolumeFacts,
    ledger: &ImageFactsLedger,
    evidence_scope: EvidenceScope,
) -> Result<RenameDraft, ProposalRefusal> {
    let input: RenamePlanInput = serde_json::from_value(params.clone()).map_err(|_| {
        ToolError::invalid_params(rows_problem(params).unwrap_or_else(|| {
            "Provide a rename plan with sourcePath, volumeId, destinationName, and evidence for every row.".to_string()
        }))
    })?;
    if input.renames.is_empty() {
        return Err(invalid_params("A rename plan needs at least one row."));
    }
    if input.renames.len() > MAX_RENAMES {
        return Err(invalid_params("A rename plan can contain up to 200 items.".to_string()));
    }
    let (volume_id, parent, mut rows) = plan_rows(input.renames).map_err(ProposalRefusal::Rows)?;
    // One unbacked claim refuses the WHOLE plan: staging the rest would show the user a
    // partial plan they'd read as complete, and the model needs to resend it anyway. Checked
    // on the paths as sent, which are the paths `image_facts` answered for.
    check_row_evidence(ledger, evidence_scope, &mut rows).map_err(ProposalRefusal::Evidence)?;
    let placed = place_folder(facts, &volume_id, &parent).map_err(ProposalRefusal::Location)?;
    if placed.path != parent {
        // Rebased onto the volume's current root, or re-addressed onto a live sibling of the
        // same share: the stored rows are what preflight and apply read, so they say where
        // the files really are now.
        for row in &mut rows {
            let name = Path::new(&row.source_path).file_name().unwrap_or_default();
            row.source_path = Path::new(&placed.path).join(name).to_string_lossy().into_owned();
        }
    }
    Ok(RenameDraft {
        volume_id: placed.volume_id,
        parent: placed.path,
        rows,
    })
}

/// The rows as draft rows plus the plan's one volume and one folder, or every row that can't be
/// staged. The volume and folder are the ones MOST rows name, so one stray row is the one named.
///
/// One group is one `start_bulk_rename` call, and that executor refuses a row whose source and
/// destination parents differ, so the group binds ONE parent folder and a plan spanning folders
/// is refused here rather than half-applied later.
fn plan_rows(renames: Vec<RenameInput>) -> Result<(String, String, Vec<RenameDraftRow>), Vec<RowRejection>> {
    let parents: Vec<String> = renames
        .iter()
        .map(|rename| {
            Path::new(&rename.source_path)
                .parent()
                .map(|parent| parent.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
        .collect();
    let volume_id = most_common(renames.iter().map(|rename| rename.volume_id.as_str()))
        .unwrap_or_default()
        .to_string();
    let parent = most_common(
        renames
            .iter()
            .zip(&parents)
            .filter(|(rename, _)| is_plain(rename))
            .map(|(_, parent)| parent.as_str()),
    )
    .unwrap_or_default()
    .to_string();

    let mut rejections = Vec::new();
    let mut source_paths = HashSet::new();
    let mut destination_names = HashSet::new();
    let mut rows = Vec::with_capacity(renames.len());
    for (rename, row_parent) in renames.into_iter().zip(parents) {
        let problem = own_problem(&rename.source_path, &rename.volume_id, &rename.destination_name)
            .or_else(|| (rename.volume_id != volume_id).then_some(RowProblem::OtherVolume))
            .or_else(|| (row_parent != parent).then_some(RowProblem::OtherFolder))
            .or_else(|| (!source_paths.insert(rename.source_path.clone())).then_some(RowProblem::RepeatedSource))
            .or_else(|| {
                (!destination_names.insert(destination_key(&rename.destination_name)))
                    .then_some(RowProblem::RepeatedName)
            });
        match problem {
            Some(problem) => rejections.push(RowRejection::new(&rename.source_path, problem)),
            None => rows.push(RenameDraftRow {
                source_path: rename.source_path,
                destination_name: rename.destination_name,
                evidence: rename.evidence,
                coverage: None,
                snapshot: None,
            }),
        }
    }
    if rejections.is_empty() {
        Ok((volume_id, parent, rows))
    } else {
        Err(rejections)
    }
}

/// Whether a row's path counts toward the plan's folder: a row whose path isn't plain is
/// refused on its own, and must not out-vote the rest.
fn is_plain(rename: &RenameInput) -> bool {
    super::rows::is_plain_path(&rename.source_path)
}

/// Where the plan's folder is: placed by the location resolver (`crate::location`), on its own
/// volume when that's live, else on the first live, proven sibling of the same share, else the
/// read tools' typed miss. Any connected volume's folder qualifies; no pane has to show it.
pub(super) fn place_folder(
    facts: &VolumeFacts,
    volume_id: &str,
    parent: &str,
) -> Result<Location, Box<LocationMissResult>> {
    let input = LocationInput {
        volume_id: Some(volume_id.to_string()),
        path: parent.to_string(),
    };
    let resolved = resolve(facts, &input).map_err(|miss| Box::new(miss.into()))?;
    if resolved.live {
        return Ok(resolved.location);
    }
    facts
        .readers(&resolved)
        .into_iter()
        .skip(1)
        .find(|reader| reader.live)
        .map(|reader| reader.location)
        .ok_or_else(|| Box::new(volume_not_connected(facts, &resolved)))
}

/// Check every row's evidence, recording the accepted rows' display coverage in place, or
/// refuse with every row that didn't check out, in plan order. Pure over the ledger, so the
/// guardrail is testable without a Tauri app.
///
/// Coverage lands only on a row the ledger already accepted, so it can never become a way to
/// pass the check: it describes a delivery, after that delivery vouched for the quote.
pub(super) fn check_row_evidence(
    ledger: &ImageFactsLedger,
    scope: EvidenceScope,
    rows: &mut [RenameDraftRow],
) -> Result<(), Vec<EvidenceRejection>> {
    let mut rejections = Vec::new();
    for row in rows.iter_mut() {
        match ledger.check(scope, &row.source_path, &row.evidence) {
            Ok(coverage) => row.coverage = coverage,
            Err(problem) => rejections.push(EvidenceRejection {
                source_path: row.source_path.clone(),
                proposed_name: row.destination_name.clone(),
                evidence_source: row.evidence.source,
                problem,
            }),
        }
    }
    if rejections.is_empty() { Ok(()) } else { Err(rejections) }
}

/// Whether `source_path` names something strictly INSIDE an archive, which a
/// rename plan has to refuse: an archive-inner path has no file on disk for the
/// bulk-rename executor to touch.
///
/// The archive FILE itself is not that. `/photos/trip.zip` is an ordinary file
/// and renaming it is an ordinary rename, so this asks the same narrow question
/// `cmdr_archive::path_is_inside_archive` does (a non-empty inner path) rather
/// than merely finding an archive-suffixed component. Purely lexical, no I/O:
/// validation runs per row and shouldn't stat.
///
/// Load-bearing now that `.docx` is a browsable container: the wide question
/// would refuse a bulk rename of every Office document in a folder, which is one
/// of the things people most want bulk rename FOR.
pub(super) fn is_archive_inner_path(source_path: &str) -> bool {
    cmdr_fs::archive_format::archive_boundary_candidate(Path::new(source_path))
        .is_some_and(|(_archive, inner)| !inner.as_os_str().is_empty())
}

/// The key two destination names share when a filesystem may consider them one: the same fold
/// every proposal's names and the transfer engine use, on every platform. ❌ Not
/// `normalize_for_comparison`, which follows THIS machine's disk (identity off macOS), so a
/// Linux run would miss a case-only duplicate bound for a case-insensitive share.
pub(super) fn destination_key(name: &str) -> String {
    cmdr_fs::name_fold::fold_name(name).into_owned()
}

pub(super) fn validate_destination_name(name: &str) -> Result<(), ToolError> {
    if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(ToolError::invalid_params(
            "Each destinationName must be one filename, not a path.",
        ));
    }
    validate_filename(name).map_err(|_| ToolError::invalid_params("Each destinationName must be a valid filename."))
}
