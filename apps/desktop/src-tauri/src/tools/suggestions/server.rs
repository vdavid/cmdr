//! The `propose_server` tool: the agent puts a server to add, or a change to a saved one, in
//! front of the user as a suggestion card.
//!
//! `Access::Propose`. It stages one zero-op `SaveServer` group on the proposal spine and
//! nothing else: it saves no server, contacts no host, and takes no secret. The user's click
//! is what saves (`suggested_ops::server`), through the servers hub's own cores.
//!
//! Only on a turn the user started: its handler reads the turn's origin and refuses a wake or a
//! follow-up itself, so the dispatch gate stays tier-only (`DETAILS.md` § Proposing a server).
//! The checks: `server_plan.rs`. The saved servers they read, and the checks the approval runs
//! again: `suggested_ops::server`.

use rusqlite::Connection;
use serde_json::{Value, json};
use tauri::{AppHandle, Manager, Runtime};

use super::propose::{
    ApplyRefusal, ProposeReport, ResolvedGroup, apply_refusal_content, check_amendable, now_secs, sweep_for,
    write_group,
};
use super::server_plan::{ServerProblem, plan_server};
use crate::agent::AgentDb;
use crate::agent::store::proposals::{GroupIntent, NewGroup};
use crate::agent::suggested_ops::server::SavedServers;
use crate::agent::types::TurnOrigin;
use crate::tools::{ToolError, ToolResult};

/// The schema, TERSE: it rides every turn's prefix. Which fields each protocol takes, on an add
/// and on an edit, the refusals teach (`unboundField`, `missingField`), and `DETAILS.md` §
/// Proposing a server holds the map.
pub fn propose_server_schema() -> Value {
    let text = |what: &str| json!({ "type": "string", "description": what });
    json!({
        "type": "object",
        "properties": {
            "sweepId": { "type": "integer" },
            "groupId": { "type": "integer", "description": "A pending suggestion of sweepId to rewrite." },
            "editing": text("A saved server's volumeId, to change it instead of adding one."),
            "rationale": text("Why, shown as your words."),
            "protocol": { "type": "string", "enum": ["smb", "sftp", "webdav", "nfs", "s3"] },
            "name": { "type": "string" },
            "host": { "type": "string" },
            "port": { "type": "integer" },
            "username": { "type": "string" },
            "share": { "type": "string" },
            "path": text("The remote root."),
            "url": { "type": "string" },
            "export": { "type": "string" },
            "bucket": { "type": "string" },
            "account": text("A saved S3 account's id."),
            "startFolder": { "type": "string" },
            "uid": { "type": "integer" },
            "gid": { "type": "integer" }
        },
        // Closed, like every agent schema: the dispatch gate refuses a `password` (or any
        // undeclared) key by name before the handler runs. The handler refuses them again
        // (`secretRefused`, `unboundField`) for any path that skips the gate.
        "required": ["protocol"],
        "additionalProperties": false
    })
}

/// The registry path. It knows no turn, so it can't know the user started one: it refuses, as a
/// background turn does. The agent's calls come through [`propose_server_in_thread`].
pub async fn execute_propose_server<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    propose_server_in_thread(app, None, params, TurnOrigin::Wake).await
}

/// Stage a server suggestion in the chat thread that asked for it.
pub(crate) async fn propose_server_in_thread<R: Runtime>(
    app: &AppHandle<R>,
    conversation_id: Option<i64>,
    params: &Value,
    origin: TurnOrigin,
) -> ToolResult {
    // Before reading anything: a background turn learns nothing about what's saved.
    if !origin.user_started() {
        return Ok(problem_content(&ServerProblem::RefusedInBackground));
    }
    let saved = SavedServers::now(app);
    let db_path = app
        .try_state::<AgentDb>()
        .ok_or_else(|| ToolError::internal("Cmdr's suggestion store isn't open yet."))?
        .db_path()
        .to_path_buf();
    let params = params.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        let conn = crate::agent::store::open_write_connection(&db_path)
            .map_err(|e| ServerRefusal::Apply(ApplyRefusal::Store(e)))?;
        apply_server(&conn, &saved, &params, origin, conversation_id, now_secs())
    })
    .await
    .map_err(|e| ToolError::internal(e.to_string()))?;
    match outcome {
        Ok(report) => serde_json::to_value(&report).map_err(|e| ToolError::internal(e.to_string())),
        Err(ServerRefusal::Apply(ApplyRefusal::Store(e))) => Err(ToolError::internal(e.to_string())),
        Err(refusal) => Ok(refusal_content(&refusal)),
    }
}

/// Why a server suggestion staged nothing.
#[derive(Debug)]
pub(super) enum ServerRefusal {
    Problem(ServerProblem),
    /// The sweep or the group to amend: the file tool's own refusals.
    Apply(ApplyRefusal),
}

/// Check, then write. The origin first, then every check, so a refusal leaves the store exactly
/// as it was. The report is `propose_suggestions`' own shape, one group long.
pub(super) fn apply_server(
    conn: &Connection,
    saved: &SavedServers,
    params: &Value,
    origin: TurnOrigin,
    conversation_id: Option<i64>,
    now: i64,
) -> Result<ProposeReport, ServerRefusal> {
    if !origin.user_started() {
        return Err(ServerRefusal::Problem(ServerProblem::RefusedInBackground));
    }
    let planned = plan_server(params, saved).map_err(ServerRefusal::Problem)?;
    if let Some(group_id) = planned.group_id {
        check_amendable(conn, 0, group_id, planned.sweep_id).map_err(ServerRefusal::Apply)?;
    }
    let sweep_id = sweep_for(conn, planned.sweep_id, conversation_id, None, now).map_err(ServerRefusal::Apply)?;
    let group = NewGroup {
        intent: GroupIntent::SaveServer {
            draft: planned.draft,
            edit: planned.edit,
        },
        // The spine stores what a server group's column holds (the edited place, or "").
        source_volume_id: String::new(),
        display_name: planned.display_name,
        rationale: planned.rationale,
        selector: None,
    };
    let entry = ResolvedGroup {
        group_id: planned.group_id,
        group,
        via: None,
    };
    let report = write_group(conn, sweep_id, entry, now).map_err(ServerRefusal::Apply)?;
    Ok(ProposeReport {
        ready_for_review: true,
        sweep_id,
        groups: vec![report],
    })
}

fn refusal_content(refusal: &ServerRefusal) -> Value {
    match refusal {
        ServerRefusal::Problem(problem) => problem_content(problem),
        ServerRefusal::Apply(refusal) => {
            let mut content = apply_refusal_content(refusal);
            // The file tool's refusals name a group by its index in the call; this call has one.
            if let Some(map) = content.as_object_mut() {
                map.remove("group");
            }
            content
        }
    }
}

/// The typed refusal: a token to branch on, the field it's about where there is one, and a
/// sentence saying what to send instead. ❌ Nothing downstream matches on the sentence.
pub(super) fn problem_content(problem: &ServerProblem) -> Value {
    let (token, sentence) = problem_words(problem);
    let mut content = json!({ "readyForReview": false, "refusal": token, "problem": sentence });
    match problem {
        ServerProblem::SecretRefused { field } | ServerProblem::UnboundField { field } => {
            content["field"] = json!(field);
        }
        ServerProblem::MissingField { field }
        | ServerProblem::BadField { field, .. }
        | ServerProblem::AddressChange { field } => content["field"] = json!(field),
        ServerProblem::AlreadySaved { server_id } => content["serverId"] = json!(server_id),
        _ => {}
    }
    content
}

fn problem_words(problem: &ServerProblem) -> (&'static str, String) {
    match problem {
        ServerProblem::Malformed => (
            "malformed",
            "That isn't the shape this tool takes. Send protocol plus the fields that protocol takes.".into(),
        ),
        ServerProblem::RefusedInBackground => (
            "refusedInBackground",
            "A server is suggested only when the user asks for something, and Cmdr started this turn on its own. \
             Say what you'd suggest; the user can ask for it."
                .into(),
        ),
        ServerProblem::SecretRefused { field } => (
            "secretRefused",
            format!("Cmdr never takes a {field} from you; the user types it when they connect. Leave it out."),
        ),
        ServerProblem::UnboundField { field } => (
            "unboundField",
            format!("This protocol doesn't take {field} here, so leave it out."),
        ),
        ServerProblem::MissingField { field } => ("missingField", format!("Adding this server needs {field}.")),
        ServerProblem::BadField { field, rule } => ("badField", format!("{field} won't do: {}.", rule.reason())),
        ServerProblem::ProtocolNotProposable => (
            "protocolNotProposable",
            "Cmdr can suggest an SMB, SFTP, WebDAV, or NFS server, or a bucket under an S3 account the user already \
             saved (name it by its id). Ask the user to add anything else in Servers."
                .into(),
        ),
        ServerProblem::UnknownServer { id } => (
            "unknownServer",
            format!("Nothing saved has the id {id}. list_volumes shows the saved places and their ids."),
        ),
        ServerProblem::AddressChange { field } => (
            "addressChange",
            format!(
                "A change can't move a saved server's {field}: that's where it is, or who it signs in as. Suggest a \
                 new server instead, or ask the user to change it in Servers."
            ),
        ),
        ServerProblem::AlreadySaved { server_id } => (
            "alreadySaved",
            format!("That server is saved already. Suggest a change to it instead, with editing: {server_id}."),
        ),
        ServerProblem::NameTaken { name } => (
            "nameTaken",
            format!("Another saved server is called {name}. Pick a name no saved server has."),
        ),
        ServerProblem::GroupIdWithoutSweep => (
            "groupIdWithoutSweep",
            "Rewriting a suggestion needs the sweepId it belongs to as well.".into(),
        ),
    }
}
