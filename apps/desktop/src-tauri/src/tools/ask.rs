//! The one way a tool waits for a person: a question named in typed facts, shown in the main window,
//! answered by a click or by nobody.
//!
//! `execute_tool` runs [`gate`] ahead of a `Sensitive` tool's handler, driven by the table's `ask` plan,
//! so no handler reinvents this. A plan reads the present state and is run again after a yes: whatever
//! the person agreed to has to still be true, or the tool refuses `changedWhileAsking`.
//! Why it's shaped this way: `DETAILS.md` § The human gate.

use std::sync::Mutex;
use std::time::Duration;

use cmdr_operation_log::types::Initiator;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_specta::Event;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use super::actions::ToolError;
use crate::ui_state::dialog_state::SoftDialogTracker;

/// The soft dialog id in `SoftDialogTracker` and the frontend's `dialog-registry.ts`.
pub(crate) const TOOL_ASK_DIALOG: &str = "tool-ask";

/// How long a question waits for a person who may have walked away.
const ASK_EXPIRY: Duration = Duration::from_secs(5 * 60);

/// The longest a model-chosen or on-disk text may be in a question. Past this the middle goes, since a
/// name's end (its extension) is what says what it does.
const MAX_FACT_CHARS: usize = 400;

/// One side of a setting change, as the question shows it: the stored value, and what a person calls
/// it when Cmdr knows a different name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SettingValueFact {
    /// The value as stored, through [`fact_text`]: `""` when empty, the mask for a secret.
    pub raw: String,
    /// The friendly name, or `None` when the raw value is all Cmdr has (or is already the name).
    pub name: Option<SettingValueName>,
}

/// What a person calls a setting's value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SettingValueName {
    /// Ready to show: the registry's option label, or an app's name the way Finder shows it.
    Named { text: String },
    /// The text editor's `system` choice, which the frontend words in the person's language.
    SystemDefault,
}

/// What a question is about. Each arm is one sentence in `tool-ask-sentence.ts`, and every string in
/// here passes through [`fact_text`], since a file name or a setting value can be anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum AskFacts {
    /// Open what's under the cursor in another app, or launch it.
    OpenFile {
        /// The exact path, so a handler can tell whether it's still what's under the cursor.
        path: String,
        /// The path as a person reads it.
        shown_path: String,
        name: String,
        /// A bundle is a directory that launches, so no app opens it: it runs.
        is_bundle: bool,
        /// The app that opens it, or `None` when nothing can tell.
        opener: Option<String>,
    },
    SetSetting {
        id: String,
        label: String,
        old_value: SettingValueFact,
        new_value: SettingValueFact,
    },
    Quit,
    Eject {
        volume_id: String,
        name: String,
        /// The whole disk, with every volume on it, over the one volume.
        whole_disk: bool,
    },
    RemoveServer {
        host_id: String,
        name: String,
    },
    Indexing {
        action: IndexingAction,
        volume_id: String,
        name: String,
    },
}

/// What the `indexing` tool can do to a volume's index, as a question names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum IndexingAction {
    Enable,
    Disable,
    Rescan,
    Forget,
}

/// `tool-ask-requested`: a question for the main window to show.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ToolAskRequested {
    pub request_id: String,
    pub facts: AskFacts,
    pub started_by: Initiator,
}

/// `tool-ask-dismissed`: nobody's waiting on this question any more, so it comes down.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ToolAskDismissed {
    pub request_id: String,
}

/// What the dialog answers through `answer_tool_ask`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ToolAskResponse {
    Allowed,
    Declined,
    /// Another modal got there first, so the question never went up.
    Blocked {
        dialog: String,
    },
    /// The dialog went away without an answer: a reload, an error boundary, a hot swap.
    Gone,
}

/// What came of asking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AskAnswer {
    Allowed,
    Declined,
    /// Nobody answered in time.
    Expired,
    /// The turn's Stop, or the client went away.
    Cancelled,
    /// There's no window to ask in, or it went away.
    NoWindow,
    /// A dialog is up, so no question can go over it. Names the dialog.
    Blocked(String),
}

/// What a `Sensitive` tool's table entry says about one call, from the present state.
#[derive(Debug)]
pub enum AskPlan {
    /// Nothing here needs a person (a folder under the cursor, a setting marked ask-free).
    NoAsk,
    Ask(AskFacts),
    Refuse(ToolError),
}

/// The one question open right now, if any.
#[derive(Default)]
pub struct ToolAskRegistry {
    pending: Mutex<Option<Pending>>,
}

struct Pending {
    request_id: String,
    answer: oneshot::Sender<ToolAskResponse>,
}

impl ToolAskRegistry {
    /// Holds the one slot for this request, or `None` while another question has it.
    fn begin(&self, request_id: &str) -> Option<oneshot::Receiver<ToolAskResponse>> {
        let mut pending = self.lock();
        if pending.is_some() {
            return None;
        }
        let (answer, receiver) = oneshot::channel();
        *pending = Some(Pending {
            request_id: request_id.to_string(),
            answer,
        });
        Some(receiver)
    }

    /// Answers this request, if it's still the open one. A late click on a question that already
    /// came down names an old id, and lands nowhere.
    pub(crate) fn answer(&self, request_id: &str, response: ToolAskResponse) {
        let mut pending = self.lock();
        if pending.as_ref().is_some_and(|open| open.request_id == request_id)
            && let Some(open) = pending.take()
        {
            let _ = open.answer.send(response);
        }
    }

    /// Answers whichever question is open, for what doesn't hold its id. Whether one was.
    pub(crate) fn answer_open(&self, response: ToolAskResponse) -> bool {
        let Some(open) = self.lock().take() else {
            return false;
        };
        open.answer.send(response).is_ok()
    }

    fn forget(&self, request_id: &str) {
        let mut pending = self.lock();
        if pending.as_ref().is_some_and(|open| open.request_id == request_id) {
            *pending = None;
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Pending>> {
        self.pending.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The dialog's answer.
#[tauri::command]
#[specta::specta]
pub fn answer_tool_ask(app: AppHandle, request_id: String, response: ToolAskResponse) {
    if let Some(registry) = app.try_state::<ToolAskRegistry>() {
        registry.answer(&request_id, response);
    }
}

/// A reloaded window can't answer what the old one showed, and nothing else would wake the wait.
pub(crate) fn window_reloaded<R: Runtime>(app: &AppHandle<R>) {
    if let Some(registry) = app.try_state::<ToolAskRegistry>() {
        registry.answer_open(ToolAskResponse::Gone);
    }
}

/// Takes the question down and frees the slot however the wait ends, a dropped future included.
struct Asked<'a, R: Runtime> {
    app: &'a AppHandle<R>,
    registry: &'a ToolAskRegistry,
    request_id: &'a str,
}

impl<R: Runtime> Drop for Asked<'_, R> {
    fn drop(&mut self) {
        self.registry.forget(self.request_id);
        let dismissed = ToolAskDismissed {
            request_id: self.request_id.to_string(),
        };
        // Off the specta helper, which panics when events aren't mounted, and this is a destructor.
        if let Err(error) = self.app.emit_to("main", ToolAskDismissed::NAME, dismissed) {
            log::warn!(target: "tools::ask", "Couldn't take a question down: {error}");
        }
    }
}

/// Puts `facts` in front of the person and waits for an answer, `cancel`, or the expiry.
pub async fn ask<R: Runtime>(
    app: &AppHandle<R>,
    facts: AskFacts,
    started_by: Initiator,
    cancel: &CancellationToken,
) -> AskAnswer {
    ask_within(app, facts, started_by, cancel, ASK_EXPIRY).await
}

async fn ask_within<R: Runtime>(
    app: &AppHandle<R>,
    facts: AskFacts,
    started_by: Initiator,
    cancel: &CancellationToken,
    expiry: Duration,
) -> AskAnswer {
    let (Some(_), Some(registry)) = (app.get_webview_window("main"), app.try_state::<ToolAskRegistry>()) else {
        return AskAnswer::NoWindow;
    };
    if let Some(blocking) = app.try_state::<SoftDialogTracker>().and_then(|t| t.blocking_dialog()) {
        return AskAnswer::Blocked(blocking);
    }
    let request_id = uuid::Uuid::new_v4().to_string();
    let Some(answer) = registry.begin(&request_id) else {
        return AskAnswer::Blocked(TOOL_ASK_DIALOG.to_string());
    };
    let _asked = Asked {
        app,
        registry: registry.inner(),
        request_id: &request_id,
    };
    let requested = ToolAskRequested {
        request_id: request_id.clone(),
        facts,
        started_by,
    };
    if app.emit_to("main", ToolAskRequested::NAME, requested).is_err() {
        return AskAnswer::NoWindow;
    }
    tokio::select! {
        biased;
        answer = answer => match answer {
            Ok(ToolAskResponse::Allowed) => AskAnswer::Allowed,
            Ok(ToolAskResponse::Declined) => AskAnswer::Declined,
            Ok(ToolAskResponse::Blocked { dialog }) => AskAnswer::Blocked(dialog),
            Ok(ToolAskResponse::Gone) | Err(_) => AskAnswer::NoWindow,
        },
        () = cancel.cancelled() => AskAnswer::Cancelled,
        () = tokio::time::sleep(expiry) => AskAnswer::Expired,
    }
}

/// Runs a tool's plan and, when it says so, the question, before the handler sees the call. Answers
/// with the facts a person agreed to, or `None` when nobody was asked. Runs only on `admit`'s `Ask`, so
/// direct mode never reaches it.
pub(super) async fn gate<R, Plan, Planned>(
    app: &AppHandle<R>,
    started_by: Initiator,
    cancel: &CancellationToken,
    plan: Plan,
) -> Result<Option<AskFacts>, ToolError>
where
    R: Runtime,
    Plan: Fn() -> Planned,
    Planned: Future<Output = AskPlan>,
{
    let facts = match plan().await {
        AskPlan::NoAsk => return Ok(None),
        AskPlan::Refuse(refusal) => return Err(refusal),
        AskPlan::Ask(facts) => facts,
    };
    let answer = ask(app, facts.clone(), started_by, cancel).await;
    log::info!(target: "tools::ask", "A question ended: {answer:?}");
    if answer != AskAnswer::Allowed {
        return Err(refusal_for(answer));
    }
    match plan().await {
        AskPlan::Ask(now) if now == facts => Ok(Some(facts)),
        AskPlan::Refuse(refusal) => Err(refusal),
        AskPlan::Ask(_) | AskPlan::NoAsk => Err(changed_while_asking()),
    }
}

/// What a tool answers when the person's yes no longer describes what's there.
pub(crate) fn changed_while_asking() -> ToolError {
    ToolError::invalid_params(
        "That changed while the person was deciding, so nothing happened. Look again, and ask again if it still makes sense.",
    )
    .with_data(json!({ "refusal": "changedWhileAsking" }))
}

/// The typed result for every answer but a yes. A decline counts as a problem, so the repeat breaker
/// stops a model asking again within a turn.
fn refusal_for(answer: AskAnswer) -> ToolError {
    let (refusal, message) = match answer {
        AskAnswer::Declined => ("declined", "The person said no, so nothing happened."),
        AskAnswer::Expired => ("expired", "Nobody answered the question in time, so nothing happened."),
        AskAnswer::Cancelled => ("cancelled", "Stopped before the person answered, so nothing happened."),
        AskAnswer::NoWindow | AskAnswer::Allowed => (
            "noWindow",
            "Cmdr has no window to ask in right now, so nothing happened.",
        ),
        AskAnswer::Blocked(dialog) => {
            return ToolError::invalid_params(format!(
                "The {dialog} dialog is open, so nothing can ask a question over it. Nothing happened."
            ))
            .with_data(json!({ "blockingDialog": dialog }));
        }
    };
    ToolError::invalid_params(message).with_data(json!({ "refusal": refusal }))
}

/// Text from outside Cmdr's own words, made safe to show: plain, bounded, and reading as what it is.
/// Control and bidirectional-override characters go, since `cod\u{202E}fdp.command` would otherwise
/// show a person a PDF. The dialog renders text nodes only, so markup is inert either way.
pub(crate) fn fact_text(text: &str) -> String {
    let plain: Vec<char> = text
        .chars()
        .filter(|c| {
            !c.is_control() && !matches!(c, '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
        })
        .collect();
    if plain.len() <= MAX_FACT_CHARS {
        return plain.into_iter().collect();
    }
    let keep = MAX_FACT_CHARS / 2;
    let head: String = plain[..keep].iter().collect();
    let tail: String = plain[plain.len() - keep..].iter().collect();
    format!("{head}…{tail}")
}

#[cfg(test)]
#[path = "ask_tests.rs"]
mod tests;
