//! What each `Sensitive` tool asks a person about, as the table's `ask:` field names it.
//!
//! A plan only reads, and runs twice: once before the question and once after a yes, so the person's
//! answer has to still describe what's there. The two that depend on what only the frontend knows (the
//! Enter policy and what's under the cursor, the settings registry) split into a round trip and a pure
//! `*_plan`, which is what the tests cover.

use serde::Deserialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Runtime};

use super::ToolCtx;
use super::actions::async_tools::managed_write_refusal;
use super::actions::{ROUND_TRIP_TIMEOUT_SECS, ToolError, mcp_round_trip_parsed};
use super::ask::{AskFacts, AskPlan, IndexingAction, SettingValueFact, SettingValueName, fact_text};
use crate::file_system::text_editor::SYSTEM_DEFAULT_CHOICE;
use crate::tools::resources::volumes::snapshot_volumes;

// ── open_under_cursor ───────────────────────────────────────────────────────

/// What opening the entry under the cursor would do, as `entry-activation.ts` resolves it. ❌ Only
/// `browse` and `viewer` skip a person, so a new way to launch something reads as a question.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum Activation {
    /// Steps into a folder or archive, or leaves for a real volume.
    Browse,
    /// Shows it in Cmdr's own viewer.
    Viewer,
    /// Hands it to another app.
    Launch,
    /// The Enter policy says to ask each time: the Browse | Open popup.
    Choose,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CursorItem {
    pub activation: Activation,
    pub path: String,
    pub name: String,
    pub is_directory: bool,
}

/// `None` is no entry under the cursor, which the handler refuses on its own.
pub(super) fn open_plan(item: Option<&CursorItem>, opener: Option<String>) -> AskPlan {
    let Some(item) = item else {
        return AskPlan::NoAsk;
    };
    match item.activation {
        Activation::Browse | Activation::Viewer => AskPlan::NoAsk,
        Activation::Launch | Activation::Choose => AskPlan::Ask(AskFacts::OpenFile {
            path: item.path.clone(),
            shown_path: fact_text(&item.path),
            name: fact_text(&item.name),
            is_bundle: item.is_directory,
            opener: opener.as_deref().map(fact_text),
        }),
    }
}

fn parse_cursor_item(payload: &str, expected_id: &str) -> Option<Result<Option<CursorItem>, String>> {
    let resp = serde_json::from_str::<Value>(payload).ok()?;
    if resp.get("requestId").and_then(|v| v.as_str()) != Some(expected_id) {
        return None;
    }
    if resp.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        return Some(Err("The frontend couldn't say what's under the cursor".to_string()));
    }
    Some(serde_json::from_value(resp.get("item").cloned().unwrap_or(Value::Null)).map_err(|e| e.to_string()))
}

pub(super) async fn plan_open_under_cursor<R: Runtime>(app: &AppHandle<R>, _ctx: &ToolCtx, _params: &Value) -> AskPlan {
    let described = mcp_round_trip_parsed(
        app,
        "mcp-describe-open-under-cursor",
        json!({}),
        ROUND_TRIP_TIMEOUT_SECS,
        parse_cursor_item,
    )
    .await;
    let item = match described {
        Ok(item) => item,
        Err(_) => return AskPlan::Refuse(couldnt_read("what's under the cursor")),
    };
    let opener = match &item {
        Some(item) if !item.is_directory => default_opener(&item.path).await,
        _ => None,
    };
    open_plan(item.as_ref(), opener)
}

#[cfg(target_os = "macos")]
async fn default_opener(path: &str) -> Option<String> {
    crate::commands::file_actions::default_app_name(path.to_string()).await
}

#[cfg(not(target_os = "macos"))]
async fn default_opener(_path: &str) -> Option<String> {
    None
}

/// A question can't be worded without this, and asking without it would fail open.
fn couldnt_read(what: &str) -> ToolError {
    ToolError::internal(format!("Couldn't read {what}, so nothing happened."))
}

// ── set_setting ─────────────────────────────────────────────────────────────

/// What `mcp-describe-setting` knows about a setting, off the frontend's registry.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SettingDescription {
    pub label: String,
    pub old_value: Value,
    pub ask_free: bool,
    /// `true`, `false`, or `"offOnly"`, as the registry's `mcpSettable` means them.
    pub mcp_settable: Value,
    /// The value is a secret, so `old_value` is already masked and the new one must be too.
    pub sensitive: bool,
    /// The organization's policy refuses this value.
    pub locked: bool,
    /// Why the registry refuses this value, when it does.
    pub invalid: Option<String>,
    /// The registry's option label for the current value, when the setting has options.
    #[serde(default)]
    pub old_value_label: Option<String>,
    /// The registry's option label for the asked-for value, when the setting has options.
    #[serde(default)]
    pub new_value_label: Option<String>,
    /// The value names an app (a bundle id, an `.app` path, or the text editor's `system`), so the
    /// question names the app.
    #[serde(default)]
    pub value_names_app: bool,
}

const MASKED: &str = "********";

/// `None` is a setting the registry doesn't know: it refuses, since nothing can say what it does.
/// `app_name` names the app an app-picker value points at (`app_display_name` outside tests).
pub(super) fn setting_plan(
    id: &str,
    new_value: &Value,
    described: Option<SettingDescription>,
    app_name: impl Fn(&str) -> Option<String>,
) -> AskPlan {
    let Some(setting) = described else {
        return AskPlan::Refuse(
            ToolError::invalid_params(format!("Cmdr has no setting called '{id}'."))
                .with_data(json!({ "refusal": "unknownSetting" })),
        );
    };
    if setting.mcp_settable == json!(false) {
        return AskPlan::Refuse(
            ToolError::invalid_params(format!(
                "'{id}' records a person's consent answer, so it can't be set over MCP. Only the person can change it, in Cmdr itself."
            ))
            .with_data(json!({ "refusal": "notSettableOverMcp" })),
        );
    }
    if setting.mcp_settable == json!("offOnly") && new_value != &json!(false) {
        return AskPlan::Refuse(
            ToolError::invalid_params(format!(
                "'{id}' records a person's data-sharing choice, so over MCP it can only be turned off (false). Only the person can turn it on, in Cmdr itself."
            ))
            .with_data(json!({ "refusal": "onlyOffOverMcp" })),
        );
    }
    if setting.locked {
        return AskPlan::Refuse(
            ToolError::invalid_params(format!(
                "'{id}' is managed by the organization's policy on this Mac, so it can't be changed here."
            ))
            .with_data(json!({ "refusal": "managedByOrganization" })),
        );
    }
    if let Some(reason) = setting.invalid {
        return AskPlan::Refuse(
            ToolError::invalid_params(format!("'{id}' can't take that value: {reason}"))
                .with_data(json!({ "refusal": "invalidValue" })),
        );
    }
    // Asking a person to agree to nothing would train them to click through. The handler's write is
    // a no-op, and it still answers the caller.
    if setting.ask_free || (!setting.sensitive && &setting.old_value == new_value) {
        return AskPlan::NoAsk;
    }
    let side = |value: &Value, label: Option<String>| {
        if setting.sensitive {
            SettingValueFact {
                raw: MASKED.to_string(),
                name: None,
            }
        } else {
            value_fact(value, label, setting.value_names_app, &app_name)
        }
    };
    AskPlan::Ask(AskFacts::SetSetting {
        id: fact_text(id),
        label: fact_text(&setting.label),
        old_value: side(&setting.old_value, setting.old_value_label.clone()),
        new_value: side(new_value, setting.new_value_label.clone()),
    })
}

/// One side of the change: the raw value, plus the name a person knows it by. An option's label wins;
/// an app-picker value names its app. A name that only repeats the raw value is dropped, so the
/// question doesn't say the same thing twice.
fn value_fact(
    value: &Value,
    label: Option<String>,
    names_app: bool,
    app_name: &impl Fn(&str) -> Option<String>,
) -> SettingValueFact {
    let raw = fact_text(&value_text(value));
    let name = if raw.is_empty() {
        None
    } else if let Some(label) = label {
        Some(SettingValueName::Named {
            text: fact_text(&label),
        })
    } else if names_app && raw == SYSTEM_DEFAULT_CHOICE {
        Some(SettingValueName::SystemDefault)
    } else if names_app {
        app_name(&raw).map(|text| SettingValueName::Named { text: fact_text(&text) })
    } else {
        None
    };
    let name = name.filter(|name| !matches!(name, SettingValueName::Named { text } if text.eq_ignore_ascii_case(&raw)));
    SettingValueFact { raw, name }
}

/// The app a stored app-picker value names, the way Finder shows it. A bundle id resolves through
/// LaunchServices; a "Choose an app…" path reads its bundle, or its file name once it's gone.
/// `None` when nothing on this Mac answers to it: the question then shows the raw id alone.
#[cfg(target_os = "macos")]
fn app_display_name(choice: &str) -> Option<String> {
    use crate::file_system::open_with::{installed_app_path, read_app_display_name};
    let path = std::path::Path::new(choice);
    if path.is_absolute() {
        if path.is_dir() {
            return Some(read_app_display_name(path));
        }
        return path.file_stem().map(|stem| stem.to_string_lossy().into_owned());
    }
    installed_app_path(choice).map(|app| read_app_display_name(&app))
}

#[cfg(not(target_os = "macos"))]
fn app_display_name(_choice: &str) -> Option<String> {
    None
}

/// A value as a person reads it: a string as itself, anything else as JSON.
fn value_text(value: &Value) -> String {
    value.as_str().map_or_else(|| value.to_string(), str::to_string)
}

fn parse_setting_description(payload: &str, expected_id: &str) -> Option<Result<Option<SettingDescription>, String>> {
    let resp = serde_json::from_str::<Value>(payload).ok()?;
    if resp.get("requestId").and_then(|v| v.as_str()) != Some(expected_id) {
        return None;
    }
    if resp.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        return Some(Err("The frontend couldn't describe the setting".to_string()));
    }
    Some(serde_json::from_value(resp.get("setting").cloned().unwrap_or(Value::Null)).map_err(|e| e.to_string()))
}

pub(super) async fn plan_set_setting<R: Runtime>(app: &AppHandle<R>, _ctx: &ToolCtx, params: &Value) -> AskPlan {
    let (Some(id), Some(value)) = (params.get("id").and_then(|v| v.as_str()), params.get("value")) else {
        // The handler answers a missing parameter in its own words.
        return AskPlan::NoAsk;
    };
    if let Some(refusal) = managed_write_refusal(id, value) {
        return AskPlan::Refuse(refusal);
    }
    let described = mcp_round_trip_parsed(
        app,
        "mcp-describe-setting",
        json!({ "settingId": id, "value": value }),
        ROUND_TRIP_TIMEOUT_SECS,
        parse_setting_description,
    )
    .await;
    match described {
        Ok(described) => setting_plan(id, value, described, app_display_name),
        Err(_) => AskPlan::Refuse(couldnt_read("that setting")),
    }
}

// ── quit ────────────────────────────────────────────────────────────────────

pub(super) async fn plan_quit<R: Runtime>(_app: &AppHandle<R>, _ctx: &ToolCtx, _params: &Value) -> AskPlan {
    AskPlan::Ask(AskFacts::Quit)
}

// ── eject, indexing ─────────────────────────────────────────────────────────

/// What a person calls this volume, else its id: asking without a name beats not asking.
async fn volume_name(volume_id: &str) -> String {
    let name = snapshot_volumes()
        .await
        .into_iter()
        .find(|volume| volume.id == volume_id)
        .map_or_else(|| volume_id.to_string(), |volume| volume.name);
    fact_text(&name)
}

pub(super) async fn plan_eject<R: Runtime>(_app: &AppHandle<R>, _ctx: &ToolCtx, params: &Value) -> AskPlan {
    let Some(volume_id) = params.get("volumeId").and_then(|v| v.as_str()) else {
        return AskPlan::NoAsk;
    };
    AskPlan::Ask(AskFacts::Eject {
        volume_id: fact_text(volume_id),
        name: volume_name(volume_id).await,
        whole_disk: params.get("scope").and_then(|v| v.as_str()) == Some("disk"),
    })
}

pub(super) async fn plan_indexing<R: Runtime>(_app: &AppHandle<R>, _ctx: &ToolCtx, params: &Value) -> AskPlan {
    let (Some(action), Some(volume_id)) = (
        params.get("action").and_then(|v| v.as_str()),
        params.get("volumeId").and_then(|v| v.as_str()),
    ) else {
        return AskPlan::NoAsk;
    };
    let action = match action {
        "enable" => IndexingAction::Enable,
        "disable" => IndexingAction::Disable,
        "rescan" => IndexingAction::Rescan,
        "forget" => IndexingAction::Forget,
        // Nothing runs for it: the handler refuses in its own words.
        _ => return AskPlan::NoAsk,
    };
    AskPlan::Ask(AskFacts::Indexing {
        action,
        volume_id: fact_text(volume_id),
        name: volume_name(volume_id).await,
    })
}

// ── remove_manual_server ────────────────────────────────────────────────────

pub(super) async fn plan_remove_manual_server<R: Runtime>(
    app: &AppHandle<R>,
    _ctx: &ToolCtx,
    params: &Value,
) -> AskPlan {
    let Some(host_id) = params.get("hostId").and_then(|v| v.as_str()) else {
        return AskPlan::NoAsk;
    };
    match crate::network::manual_servers::manual_server_label(host_id, app) {
        Some(name) => AskPlan::Ask(AskFacts::RemoveServer {
            host_id: fact_text(host_id),
            name: fact_text(&name),
        }),
        // Nothing to remove: the handler says so.
        None => AskPlan::NoAsk,
    }
}

#[cfg(test)]
#[path = "ask_plans_tests.rs"]
mod tests;
