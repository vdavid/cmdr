//! The `propose_rename_plan` tool boundary: schema in, staged proposal or typed refusal
//! out.
//!
//! Everything a plan must survive before a single row is staged lives here — the pane's
//! effective scope, per-row parameter validation, and the evidence check. One unbacked
//! content claim refuses the WHOLE plan, so the user never sees a partial plan they'd read
//! as complete.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;
use tauri::{AppHandle, Manager, Runtime};

use super::preflight::volume_uses_local_paths;
use super::store::{RenameDraft, RenameDraftRow, RenameProposalSnapshot};
use crate::agent::AgentDb;
use crate::agent::llm::types::AgentToolResult;
use crate::agent::tools::propose::evidence::{EvidenceRejection, EvidenceScope, ImageFactsLedger};
use crate::file_system::validation::validate_filename;
use crate::mcp::pane_state::{PaneFileEntry, PaneState, PaneStateStore};
use crate::mcp::{ToolError, ToolResult};

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
    evidence: crate::agent::tools::propose::evidence::RenameEvidence,
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

/// Why a proposal call didn't stage anything: an ordinary param/scope problem, or evidence
/// that didn't check out. Kept apart so the model gets the typed per-item verdict for the
/// second case instead of one flattened sentence.
pub(super) enum ProposalRefusal {
    Problem(ToolError),
    Evidence(Vec<EvidenceRejection>),
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
        ProposalRefusal::Evidence(rejections) => serde_json::json!({
            "readyForReview": false,
            "evidenceRejected": rejections,
            "guidance": "Nothing was staged, so fix these rows and send the whole plan again. A name based on what's inside an image needs image_facts to have returned that content for that exact path, and the detail must quote it. If you don't have the content, say so and name the file from its old name, its dates, or what the user asked for.",
        }),
    }
}

pub async fn dispatch<R: Runtime>(
    app: &AppHandle<R>,
    scope: EvidenceScope,
    call_id: &str,
    params: &Value,
) -> RenameDispatchOutcome {
    let outcome = build_draft(app, scope, params).and_then(|draft| stage_draft(app, scope, &draft));
    match outcome {
        Ok(snapshot) => RenameDispatchOutcome {
            result: AgentToolResult {
                call_id: call_id.to_string(),
                content: serde_json::json!({ "readyForReview": true, "count": snapshot.rows.len() }),
                elided: false,
            },
            proposal: Some(snapshot),
        },
        Err(refusal) => RenameDispatchOutcome {
            result: AgentToolResult {
                call_id: call_id.to_string(),
                content: refusal_content(&refusal),
                elided: false,
            },
            proposal: None,
        },
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

fn build_draft<R: Runtime>(
    app: &AppHandle<R>,
    evidence_scope: EvidenceScope,
    params: &Value,
) -> Result<RenameDraft, ProposalRefusal> {
    let input: RenamePlanInput = serde_json::from_value(params.clone()).map_err(|_| {
        ToolError::invalid_params(
            "Provide a rename plan with sourcePath, volumeId, destinationName, and evidence for every row.",
        )
    })?;
    if input.renames.is_empty() {
        return Err(invalid_params("A rename plan needs at least one row."));
    }
    if input.renames.len() > MAX_RENAMES {
        return Err(invalid_params("A rename plan can contain up to 200 items.".to_string()));
    }
    let store = app
        .try_state::<PaneStateStore>()
        .ok_or_else(|| ToolError::internal("Pane state isn't available yet"))?;
    let state = focused_state(&store);
    let volume_id = state
        .volume_id
        .clone()
        .ok_or_else(|| ToolError::invalid_params("The focused pane has no volume id yet."))?;
    let scope = scoped_files(&state)?;
    // An empty ledger IS the fail-closed ledger, so a missing registration refuses every
    // content claim rather than waving it through.
    let no_facts = ImageFactsLedger::default();
    let registered = app.try_state::<ImageFactsLedger>();
    let ledger = registered.as_deref().unwrap_or(&no_facts);
    let mut source_paths = HashSet::new();
    let mut destination_names = HashSet::new();
    let mut parent: Option<String> = None;
    let mut rows = Vec::with_capacity(input.renames.len());
    for rename in input.renames {
        if rename.volume_id != volume_id {
            return Err(invalid_params("Every rename must use the focused pane's volume id."));
        }
        if !source_paths.insert(rename.source_path.clone()) {
            return Err(invalid_params("A source file can appear only once in a rename plan."));
        }
        validate_destination_name(&rename.destination_name)?;
        let destination_key = cmdr_index::store::normalize_for_comparison(&rename.destination_name);
        if !destination_names.insert(destination_key) {
            return Err(invalid_params("Destination names must be unique on this volume."));
        }
        if let Some(entry) = scope.get(rename.source_path.as_str()) {
            if entry.is_directory {
                return Err(invalid_params("Rename plans can contain files, not folders."));
            }
        } else if !missing_local_child(&state, &volume_id, &rename.source_path) {
            return Err(invalid_params(
                "Every source must be in the focused pane's effective scope.",
            ));
        }
        if crate::file_system::volume::backends::archive::archive_boundary_candidate(Path::new(&rename.source_path))
            .is_some()
        {
            return Err(invalid_params("Rename plans can't include files inside an archive."));
        }
        // One group is one `start_bulk_rename` call, and that executor refuses a row whose
        // source and destination parents differ — so the group binds ONE parent folder, and a
        // plan that spans folders is refused here rather than half-applied later.
        let row_parent = Path::new(&rename.source_path)
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_default();
        if *parent.get_or_insert(row_parent.clone()) != row_parent {
            return Err(invalid_params("Every rename must stay in one folder."));
        }
        rows.push(RenameDraftRow {
            source_path: rename.source_path,
            destination_name: rename.destination_name,
            evidence: rename.evidence,
            coverage: None,
        });
    }
    // One unbacked claim refuses the WHOLE plan: staging the rest would show the user a
    // partial plan they'd read as complete, and the model needs to resend it anyway.
    check_row_evidence(ledger, evidence_scope, &mut rows).map_err(ProposalRefusal::Evidence)?;
    Ok(RenameDraft {
        volume_id,
        parent: parent.unwrap_or_default(),
        rows,
    })
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

/// A model may invent a filename that is not in the pane cache. Keep that row
/// reviewable only when it names a nonexistent direct child of the focused local
/// folder; preflight then reports `SourceMissing`. Existing out-of-scope files and
/// every remote path stay rejected at the proposal boundary.
pub(super) fn missing_local_child(state: &PaneState, volume_id: &str, source_path: &str) -> bool {
    if !volume_uses_local_paths(volume_id) || std::fs::symlink_metadata(source_path).is_ok() {
        return false;
    }
    let source = Path::new(source_path);
    source.parent() == Some(Path::new(&state.path)) && source.file_name().is_some()
}

fn focused_state(store: &PaneStateStore) -> PaneState {
    if store.get_focused_pane() == "right" {
        store.get_right()
    } else {
        store.get_left()
    }
}

pub(super) fn scoped_files(state: &PaneState) -> Result<HashMap<&str, &PaneFileEntry>, ToolError> {
    // `selected_indices` are GLOBAL listing indices, while `files` is only the loaded window
    // from `loaded_start`; convert before indexing (as `read::pane_listing` and
    // `mcp::executor` do) or a scrolled pane scopes the plan to the wrong files. Folder
    // scope needs no conversion: those indices are already window-local.
    let indexes: Vec<Option<usize>> = if state.selected_indices.is_empty() {
        (0..state.files.len()).map(Some).collect()
    } else {
        state
            .selected_indices
            .iter()
            // `checked_sub`, never `saturating_sub`: a row scrolled out below the window must
            // stay unresolvable rather than collapse onto the window's first entry.
            .map(|index| index.checked_sub(state.loaded_start))
            .collect()
    };
    let mut files = HashMap::with_capacity(indexes.len());
    for index in indexes {
        let entry = index.and_then(|index| state.files.get(index)).ok_or_else(|| {
            ToolError::invalid_params(
                "The selected files are not fully loaded. Ask the user to narrow or reload the selection.",
            )
        })?;
        files.insert(entry.path.as_str(), entry);
    }
    Ok(files)
}

pub(super) fn validate_destination_name(name: &str) -> Result<(), ToolError> {
    if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(ToolError::invalid_params(
            "Each destinationName must be one filename, not a path.",
        ));
    }
    validate_filename(name).map_err(|_| ToolError::invalid_params("Each destinationName must be a valid filename."))
}
