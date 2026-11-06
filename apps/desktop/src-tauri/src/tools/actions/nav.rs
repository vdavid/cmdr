//! Navigation tool handlers.

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::{
    AckSignal, NAV_ACK_TIMEOUT, NavAck, PaneStateStore, ROUND_TRIP_TIMEOUT_SECS, ToolError, ToolResult,
    mcp_nav_round_trip, mcp_round_trip, mcp_round_trip_ack, mcp_round_trip_parsed, snapshot_generation,
    user_path_param, validate_path_exists, wait_for_ack,
};
use crate::tools::ToolCtx;
use crate::tools::ask::{AskFacts, changed_while_asking};

/// Round-trip budget for `nav_to_path`. Generous because the FE waits for the listing to
/// complete, and a remote share's can take a while; it also has to outlast the FE's own
/// wait for the pane to come to rest (`mcp-nav-landing.ts` § `NAV_QUIET_WAIT`), so a pane
/// that never settles is reported by the FE with the location it holds rather than
/// surfacing here as a bare timeout.
pub(super) const NAV_TO_PATH_TIMEOUT_SECS: u64 = 30;

/// Word what the pane did with a navigation. Branches on the typed [`NavAck`], never on
/// message text.
///
/// The requested path stands in when the FE reports an empty one (a malformed or
/// outcome-less reply), so the message always names a place.
pub(super) fn nav_result(pane: &str, requested: &str, ack: NavAck) -> ToolResult {
    let landed_or = |landed: String| {
        if landed.is_empty() {
            requested.to_string()
        } else {
            landed
        }
    };
    match ack {
        NavAck::Navigated { path } => Ok(json!(format!("OK: Navigated {pane} pane to {}", landed_or(path)))),
        NavAck::FellBack { path } => Err(ToolError::internal(format!(
            "Navigation to {requested} didn't land: the {pane} pane came to rest at {} instead. Read cmdr://state to see what it's showing.",
            landed_or(path)
        ))),
        NavAck::DidNotSettle { path } => Err(ToolError::internal(format!(
            "Navigation to {requested} didn't settle: the {pane} pane is still listing and reports {}. Read cmdr://state to triage, then retry or use `await` to watch for the path.",
            landed_or(path)
        ))),
        NavAck::Stalled { path } => {
            let path = landed_or(path);
            Err(stalled_error(
                format!(
                    "The {pane} pane is on {path}, but the folder isn't answering: its server or drive stopped responding mid-read. Cmdr keeps retrying in the background and opens it once it answers. Navigate elsewhere, or retry later; cmdr://state shows `listing: stalled` meanwhile."
                ),
                &path,
            ))
        }
    }
}

/// The error for a folder that stalled, with a typed `reason` an agent can branch on.
fn stalled_error(message: String, path: &str) -> ToolError {
    ToolError::internal(message).with_data(json!({ "reason": "folderStalled", "path": path }))
}

/// Round-trip budget for `select_volume`. The FE holds its reply for the switch's
/// correction (bounded by its 500 ms existence checks) plus the same quiet wait
/// `nav_to_path` makes, so it gets the same budget.
const SELECT_VOLUME_TIMEOUT_SECS: u64 = NAV_TO_PATH_TIMEOUT_SECS;

/// Word what a volume select did to the pane. Branches on the typed [`NavAck`], never on
/// message text.
///
/// A select doesn't pick the folder: the switch reopens the one last used on that volume,
/// so the `OK` names the folder the pane opened, and only the volume decides success.
pub(super) fn select_volume_result(pane: &str, volume_name: &str, ack: NavAck) -> ToolResult {
    match ack {
        NavAck::Navigated { path } if path.is_empty() => {
            Ok(json!(format!("OK: Switched {pane} pane to volume {volume_name}")))
        }
        NavAck::Navigated { path } => Ok(json!(format!(
            "OK: Switched {pane} pane to volume {volume_name}, at {path}"
        ))),
        NavAck::FellBack { path } => Err(ToolError::internal(format!(
            "Switching the {pane} pane to volume {volume_name} didn't land: it came to rest at {path} instead. Read cmdr://state to see what it's showing."
        ))),
        NavAck::DidNotSettle { path } => Err(ToolError::internal(format!(
            "Switching the {pane} pane to volume {volume_name} didn't settle: it's still listing and reports {path}. Read cmdr://state to triage, then retry."
        ))),
        NavAck::Stalled { path } => Err(stalled_error(
            format!(
                "Switched the {pane} pane to volume {volume_name}, but {path} isn't answering: its server or drive stopped responding mid-read. Cmdr keeps retrying in the background and opens it once it answers. Navigate elsewhere, or retry later; cmdr://state shows `listing: stalled` meanwhile."
            ),
            &path,
        )),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SelectableVolume {
    pub id: String,
    pub name: String,
    pub is_favorite: bool,
}

/// Resolves a stable id, or a unique legacy name, against the same rows the switcher receives.
/// An id wins when both are supplied so stale display copy can never redirect the selection.
#[cfg(any(target_os = "macos", test))]
pub(super) fn resolve_volume_selector(
    rows: &[SelectableVolume],
    volume_id: Option<&str>,
    volume_name: Option<&str>,
) -> Result<SelectableVolume, ToolError> {
    let servers = SelectableVolume {
        id: "network".to_string(),
        name: crate::volume_listing::SERVERS_VOLUME_NAME.to_string(),
        is_favorite: false,
    };
    let available = || {
        rows.iter()
            .chain(std::iter::once(&servers))
            .map(|row| format!("{} ({})", row.name, row.id))
            .collect::<Vec<_>>()
    };

    if let Some(id) = volume_id {
        return rows
            .iter()
            .chain(std::iter::once(&servers))
            .find(|row| row.id == id)
            .cloned()
            .ok_or_else(|| {
                ToolError::invalid_params(format!(
                    "Volume id '{id}' not found. Available volumes: {}",
                    available().join(", ")
                ))
                .with_data(json!({ "reason": "volumeNotFound", "availableVolumes": available() }))
            });
    }

    let Some(name) = volume_name else {
        return Err(ToolError::invalid_params("Give either 'volumeId' or 'name'"));
    };
    let matches: Vec<&SelectableVolume> = rows
        .iter()
        .chain(std::iter::once(&servers))
        .filter(|row| row.name == name)
        .collect();
    match matches.as_slice() {
        [found] => Ok((*found).clone()),
        [] => Err(ToolError::invalid_params(format!(
            "Volume '{name}' not found. Available volumes: {}",
            available().join(", ")
        ))
        .with_data(json!({ "reason": "volumeNotFound", "availableVolumes": available() }))),
        _ => {
            let matching_ids: Vec<&str> = matches.iter().map(|row| row.id.as_str()).collect();
            Err(ToolError::invalid_params(format!(
                "Volume name '{name}' is ambiguous. Use volumeId instead. Matching ids: {}",
                matching_ids.join(", ")
            ))
            .with_data(json!({ "reason": "ambiguousVolumeName", "matchingVolumeIds": matching_ids })))
        }
    }
}

/// Whether a pane's pushed state shows the selected volume. By id: a display name follows the UI
/// language (the servers hub pushes `Servery` in Czech), so it can't say which volume this is. The
/// name is only the fallback for a selection with no id, which is a name-only call off macOS.
pub(super) fn pane_reports_volume(
    pushed_id: Option<&str>,
    pushed_name: Option<&str>,
    selected: &SelectableVolume,
) -> bool {
    if selected.id.is_empty() {
        pushed_name == Some(selected.name.as_str())
    } else {
        pushed_id == Some(selected.id.as_str())
    }
}

/// Wait for the pane's pushed state to show the selected volume, so a `cmdr://state` read right
/// after the tool returns reports it.
///
/// The pane is at rest by the time this runs, so it usually matches on the first look.
/// The servers hub is why it stays: it pushes its own state (`NetworkMountView`), which
/// can trail the FE's reply.
async fn wait_for_pane_volume(
    store: &PaneStateStore,
    pane: &str,
    selected: &SelectableVolume,
) -> Result<(), ToolError> {
    let deadline = tokio::time::Instant::now() + NAV_ACK_TIMEOUT;
    let poll_interval = std::time::Duration::from_millis(250);
    loop {
        let state = if pane == "left" {
            store.get_left()
        } else {
            store.get_right()
        };
        if pane_reports_volume(state.volume_id.as_deref(), state.volume_name.as_deref(), selected) {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            let volume_name = &selected.name;
            let current_name = state.volume_name;
            return Err(ToolError::internal(format!(
                "The {pane} pane came to rest on volume '{volume_name}', but its state still reports volume {current_name:?}. Read cmdr://state to triage."
            )));
        }
        tokio::time::sleep(poll_interval).await;
    }
}

/// Which pane a [`go_to_in_pane`] lands in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "`Left` / `Right` come only from the macOS-only `cmdr` terminal command"
    )
)]
pub(crate) enum PaneChoice {
    /// Whichever pane has focus when the move starts.
    Focused,
    Left,
    Right,
}

impl PaneChoice {
    /// The pane id the `mcp-*` events speak, given the store's focused pane.
    pub(crate) fn resolve(self, focused: &str) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Focused if focused == "right" => "right",
            Self::Focused => "left",
        }
    }
}

/// Where a request to show paths lands: which pane, and whether in a fresh tab there.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GoToTarget {
    pub pane: PaneChoice,
    /// Open a new tab in that pane first, so the tab the user was on keeps its place.
    pub new_tab: bool,
}

#[cfg(target_os = "macos")]
impl GoToTarget {
    /// The focused pane's current tab: what a reveal and the latest-download hotkey use.
    pub(crate) const FOCUSED: Self = Self {
        pane: PaneChoice::Focused,
        new_tab: false,
    };
}

/// Open a new tab in a pane, and report whether the pane took it.
///
/// `Ok(false)` is a pane at its tab limit. The frontend owns the limit (`MAX_TABS_PER_PANE`)
/// and answers on the request id, so a refusal comes back at once and the limit lives in
/// one place. The new tab is the frontend's clone trick: the clone keeps the old location
/// to the LEFT and the active tab moves on, so the next navigation lands in the active tab.
#[cfg(target_os = "macos")]
pub(crate) async fn open_tab_in_pane<R: Runtime>(app: &AppHandle<R>, pane: PaneChoice) -> Result<bool, ToolError> {
    let focused = app
        .try_state::<PaneStateStore>()
        .map(|store| store.get_focused_pane())
        .unwrap_or_default();
    super::app::open_tab_round_trip(app, pane.resolve(&focused)).await
}

/// Where a [`go_to_in_pane`] stopped short, so a caller that speaks to a person can say
/// which half went wrong: the pane never got there, or it got there and the thing to
/// point at wasn't in the listing. A tool caller folds both into its [`ToolError`].
#[derive(Debug)]
pub(crate) enum GoToFailure {
    /// The pane didn't come to rest on the folder (refused, unresolvable, fell back,
    /// stalled, or the frontend never answered).
    NotOpened(ToolError),
    /// The pane is on the folder, but the cursor or the selection couldn't land on the
    /// entries: they aren't in the listing (gone, renamed, or hidden while hidden files
    /// are off).
    NotPointed(ToolError),
}

// macOS-only like its one reader, `crate::reveal`: off macOS it has nobody, and `-D unused`.
#[cfg(target_os = "macos")]
impl GoToFailure {
    /// Whether the folder's server or drive stopped answering, read off the typed
    /// `data.reason` that `stalled_error` attaches, ❌ never off the message.
    pub(crate) fn folder_stalled(&self) -> bool {
        let Self::NotOpened(err) = self else {
            return false;
        };
        err.data
            .as_ref()
            .and_then(|data| data.get("reason"))
            .and_then(Value::as_str)
            == Some("folderStalled")
    }

    pub(crate) fn error(&self) -> &ToolError {
        match self {
            Self::NotOpened(err) | Self::NotPointed(err) => err,
        }
    }
}

// A method on `GoToFailure`, ❌ not `impl From<GoToFailure> for ToolError`: cargo-modules
// files a `From` under the module defining `ToolError` (`actions`), drawing `actions → nav`
// and closing an `actions → nav → ask/policy → actions` circle (`module-cycles`).
impl GoToFailure {
    /// The [`ToolError`] a tool caller answers with, whichever half went wrong.
    pub(crate) fn into_error(self) -> ToolError {
        match self {
            Self::NotOpened(err) | Self::NotPointed(err) => err,
        }
    }
}

/// Take a pane to `dir` and point it at `entries`: cursor on the first, all of them
/// selected when there's more than one. An empty `entries` just opens `dir`.
///
/// The one implementation of "go there and show me that", shared by the
/// `go_to_latest_download` tool, the OS reveal handler, and the `cmdr` terminal command
/// (both `crate::reveal`). They all drive the same frontend events, and a second copy
/// would drift on the parts that are easy to get wrong: the typed landing check that stops
/// a cursor move in the wrong directory, and the pane the move applies to.
///
/// Ordering is load-bearing: the cursor moves before the multi-select, because the
/// frontend's cursor move resets the selection. A new tab, when one is wanted, is
/// `open_tab_in_pane`'s job and comes before this call, so the navigation lands in it.
pub(crate) async fn go_to_in_pane<R: Runtime>(
    app: &AppHandle<R>,
    pane: PaneChoice,
    dir: &str,
    entries: &[String],
) -> Result<(), GoToFailure> {
    let store = app.try_state::<PaneStateStore>();
    let focused = store.as_ref().map(|store| store.get_focused_pane()).unwrap_or_default();
    let pane = pane.resolve(&focused).to_string();
    if let Some(store) = &store {
        // Same as `nav_to_path`: the FE moves focus to the navigated pane, so the store
        // follows now and a focused-pane op right after acts on the right one.
        store.set_focused_pane(pane.clone());
    }

    // Reuses the FE's `mcp-nav-to-path` handler, so every volume / listing edge case it
    // already handles applies here too — including the typed landing outcome, so a pane
    // that fell back somewhere else stops the flow instead of moving a cursor in the
    // wrong directory.
    let ack = mcp_nav_round_trip(
        app,
        "mcp-nav-to-path",
        json!({"pane": pane, "path": dir}),
        NAV_TO_PATH_TIMEOUT_SECS,
    )
    .await
    .map_err(GoToFailure::NotOpened)?;
    nav_result(&pane, dir, ack).map_err(GoToFailure::NotOpened)?;

    let Some(first) = entries.first() else {
        return Ok(());
    };
    // If the entry vanished between resolving it and the FE placing the cursor, the FE
    // says so through `mcp-response` and the caller reports it. Jump-then-vanish is
    // acceptable to leak through: the navigation completed, only the cursor missed.
    mcp_round_trip_ack(app, "mcp-move-cursor", json!({"pane": pane, "to": first}))
        .await
        .map_err(GoToFailure::NotPointed)?;

    if entries.len() > 1 {
        mcp_round_trip_ack(
            app,
            "mcp-select-names",
            json!({"pane": pane, "names": entries, "mode": "replace"}),
        )
        .await
        .map_err(GoToFailure::NotPointed)?;
    }
    Ok(())
}

/// Execute a navigation command without parameters.
/// These emit keyboard-equivalent events to the frontend.
///
/// Ack contract:
/// - `nav_to_parent`, `nav_back`, `nav_forward` → pane generation must advance (path changes get
///   pushed via `update_*_pane_state`).
///
/// The `nav_to_parent` / `nav_back` / `nav_forward` family uses `NAV_ACK_TIMEOUT` (5 s)
/// instead of the default 1500 ms because navigation can touch a remote backend
/// (SMB/MTP), whose directory listing can take a few seconds even on success. With the
/// default budget, every remote-share `Enter` would surface a false-negative timeout
/// while the navigation actually succeeded in the background.
pub async fn execute_nav_command<R: Runtime>(app: &AppHandle<R>, name: &str) -> ToolResult {
    let key = match name {
        "nav_to_parent" => "Backspace",
        "nav_back" => "GoBack",       // Custom event, handled by frontend
        "nav_forward" => "GoForward", // Custom event
        _ => return Err(ToolError::invalid_params(format!("Unknown nav command: {name}"))),
    };

    let action = match name {
        "nav_to_parent" => "Navigated to parent directory",
        "nav_back" => "Navigated back",
        "nav_forward" => "Navigated forward",
        _ => "Navigation action completed",
    };

    let pre_gen = snapshot_generation(app);
    app.emit("mcp-key", json!({"key": key}))?;

    wait_for_ack(app, AckSignal::GenerationAdvanced { from: pre_gen }, NAV_ACK_TIMEOUT).await?;
    Ok(json!(format!("OK: {action}")))
}

/// Execute a navigation command with parameters.
/// What came of `mcp-open-under-cursor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenAck {
    Opened,
    /// What's under the cursor isn't what the person agreed to, or it would launch something nobody
    /// agreed to at all.
    ChangedWhileAsking,
}

fn parse_open_response(payload: &str, expected_id: &str) -> Option<Result<OpenAck, String>> {
    let resp = serde_json::from_str::<Value>(payload).ok()?;
    if resp.get("requestId").and_then(|v| v.as_str()) != Some(expected_id) {
        return None;
    }
    if resp.get("refusal").and_then(|v| v.as_str()) == Some("changedWhileAsking") {
        return Some(Ok(OpenAck::ChangedWhileAsking));
    }
    Some(if resp.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        Ok(OpenAck::Opened)
    } else {
        Err(resp
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown error")
            .to_string())
    })
}

/// What the frontend checks at the moment it opens, so nothing can slip between the plan's read and
/// the open. A yes names the path it was about. With no yes, a guarded caller may browse but never
/// launch: the plan saw nothing to ask about, and a cursor that moved since must not change that.
fn open_gate_payload(ctx: &ToolCtx) -> Value {
    match &ctx.asked {
        Some(AskFacts::OpenFile { path, .. }) => json!({ "gate": { "expectedPath": path } }),
        _ if ctx.caller.guarded() => json!({ "gate": { "mayLaunch": false } }),
        _ => json!({}),
    }
}

/// Execute `open_under_cursor`: a round trip via `mcp-open-under-cursor`. The FE awaits the resolved
/// action (directory navigation, viewer window, or OS open-with-default) and emits `mcp-response`. We
/// can't rely on `GenerationAdvanced` or `WindowAppeared` here because the OS-open branch produces
/// neither: `openFile()` hands the path to the OS default and returns, no state push, no viewer
/// window. The round trip is the only honest ack for this multi-mode command. Five seconds covers
/// the slow case (a large listing) without being open-ended.
pub async fn execute_open_under_cursor<R: Runtime>(app: &AppHandle<R>, ctx: &ToolCtx, _params: &Value) -> ToolResult {
    let ack = mcp_round_trip_parsed(
        app,
        "mcp-open-under-cursor",
        open_gate_payload(ctx),
        ROUND_TRIP_TIMEOUT_SECS,
        parse_open_response,
    )
    .await?;
    match ack {
        OpenAck::Opened => Ok(json!("OK: Opened item under cursor")),
        OpenAck::ChangedWhileAsking => Err(changed_while_asking()),
    }
}

pub async fn execute_nav_command_with_params<R: Runtime>(app: &AppHandle<R>, name: &str, params: &Value) -> ToolResult {
    match name {
        "select_volume" => {
            let pane = params
                .get("pane")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::invalid_params("Missing 'pane' parameter"))?;
            let requested_name = params.get("name").and_then(|v| v.as_str());
            let requested_id = params.get("volumeId").and_then(|v| v.as_str());

            if !["left", "right"].contains(&pane) {
                return Err(ToolError::invalid_params("pane must be 'left' or 'right'"));
            }
            if requested_id.is_none() && requested_name.is_none() {
                return Err(ToolError::invalid_params("Give either 'volumeId' or 'name'"));
            }

            // Resolve against the same completed pipeline that supplies the switcher. This
            // folds in every device provider, so ADB and MTP keep their own stable ids.
            #[cfg(target_os = "macos")]
            let selected = {
                let locations = crate::volume_listing::complete(crate::volumes::list_locations()).await;
                let rows: Vec<SelectableVolume> = locations
                    .into_iter()
                    .map(|location| SelectableVolume {
                        id: location.id,
                        name: location.name,
                        is_favorite: location.category == crate::volumes::LocationCategory::Favorite,
                    })
                    .collect();
                resolve_volume_selector(&rows, requested_id, requested_name)?
            };
            #[cfg(not(target_os = "macos"))]
            let selected = SelectableVolume {
                id: requested_id.unwrap_or_default().to_string(),
                name: requested_name.or(requested_id).unwrap_or_default().to_string(),
                is_favorite: false,
            };

            let store = app
                .try_state::<PaneStateStore>()
                .ok_or_else(|| ToolError::internal("Pane state not available"))?;
            store.set_focused_pane(pane.to_string());

            // The FE replies once the pane has come to rest (`mcp-volume-select.ts`): after
            // the switch's remembered-folder correction has landed and the listing it picked
            // has settled, with the typed outcome and the folder it opened.
            let mut select_payload = json!({"pane": pane, "name": &selected.name});
            if !selected.id.is_empty() {
                select_payload["volumeId"] = json!(&selected.id);
            }
            let ack = mcp_nav_round_trip(app, "mcp-volume-select", select_payload, SELECT_VOLUME_TIMEOUT_SECS).await?;
            // A favorite leaves the pane on the volume that holds it, and that's the
            // one it reports: waiting for the favorite itself failed a select that landed.
            if matches!(ack, NavAck::Navigated { .. }) && !selected.is_favorite {
                wait_for_pane_volume(&store, pane, &selected).await?;
            }
            select_volume_result(pane, &selected.name, ack)
        }
        "nav_to_path" => {
            let pane = params
                .get("pane")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::invalid_params("Missing 'pane' parameter"))?;
            let path = user_path_param(params, "path")?;

            if !["left", "right"].contains(&pane) {
                return Err(ToolError::invalid_params("pane must be 'left' or 'right'"));
            }

            // Virtual paths (mtp://, smb://) skip the local check; local paths are
            // probed on the blocking pool under a timeout.
            validate_path_exists(&path).await?;

            if let Some(store) = app.try_state::<PaneStateStore>() {
                store.set_focused_pane(pane.to_string());
            }

            let ack = mcp_nav_round_trip(
                app,
                "mcp-nav-to-path",
                json!({"pane": pane, "path": path}),
                NAV_TO_PATH_TIMEOUT_SECS,
            )
            .await?;
            nav_result(pane, &path, ack)
        }
        "move_cursor" => {
            let pane = params
                .get("pane")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::invalid_params("Missing 'pane' parameter"))?;

            if !["left", "right"].contains(&pane) {
                return Err(ToolError::invalid_params("pane must be 'left' or 'right'"));
            }

            let index_param = params.get("index");
            let filename_param = params.get("filename");

            let to = match (index_param, filename_param) {
                (Some(_), Some(_)) => {
                    return Err(ToolError::invalid_params(
                        "Provide either 'index' or 'filename', not both",
                    ));
                }
                (None, None) => {
                    return Err(ToolError::invalid_params("Provide either 'index' or 'filename'"));
                }
                (Some(idx), None) => {
                    let index = idx
                        .as_i64()
                        .ok_or_else(|| ToolError::invalid_params("'index' must be an integer"))?;
                    if index < 0 {
                        return Err(ToolError::invalid_params("index must be >= 0"));
                    }
                    json!(index)
                }
                (None, Some(name)) => {
                    let filename = name
                        .as_str()
                        .ok_or_else(|| ToolError::invalid_params("'filename' must be a string"))?;
                    json!(filename)
                }
            };

            if let Some(store) = app.try_state::<PaneStateStore>() {
                store.set_focused_pane(pane.to_string());
            }

            mcp_round_trip(
                app,
                "mcp-move-cursor",
                json!({"pane": pane, "to": to}),
                format!("OK: Moved cursor in {pane} pane to {to}"),
            )
            .await
        }
        "scroll_to" => {
            let pane = params
                .get("pane")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::invalid_params("Missing 'pane' parameter"))?;
            let index = params
                .get("index")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| ToolError::invalid_params("Missing 'index' parameter"))?;

            if !["left", "right"].contains(&pane) {
                return Err(ToolError::invalid_params("pane must be 'left' or 'right'"));
            }
            if index < 0 {
                return Err(ToolError::invalid_params("index must be >= 0"));
            }

            if let Some(store) = app.try_state::<PaneStateStore>() {
                store.set_focused_pane(pane.to_string());
            }

            app.emit("mcp-scroll-to", json!({"pane": pane, "index": index}))?;
            Ok(json!(format!("OK: Scrolled {pane} pane to index {index}")))
        }
        _ => Err(ToolError::invalid_params(format!("Unknown nav command: {name}"))),
    }
}

#[cfg(test)]
mod open_gate_tests {
    use serde_json::json;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::tools::Caller;

    fn ctx(caller: Caller, asked: Option<AskFacts>) -> ToolCtx {
        ToolCtx {
            caller,
            cancel: CancellationToken::new(),
            conversation_id: None,
            asked,
        }
    }

    const GUARDED: Caller = Caller::External { direct: false };

    #[test]
    fn a_yes_names_the_exact_path_it_was_about() {
        let long = format!("/a/{}", "b".repeat(2_000));
        let asked = AskFacts::OpenFile {
            path: long.clone(),
            shown_path: "/a/b…b".to_string(),
            name: "b".to_string(),
            is_bundle: false,
            opener: None,
        };
        assert_eq!(
            open_gate_payload(&ctx(GUARDED, Some(asked))),
            json!({ "gate": { "expectedPath": long } })
        );
    }

    #[test]
    fn a_guarded_caller_nobody_asked_about_may_only_browse() {
        assert_eq!(
            open_gate_payload(&ctx(GUARDED, None)),
            json!({ "gate": { "mayLaunch": false } })
        );
    }

    #[test]
    fn direct_mode_carries_no_gate() {
        assert_eq!(
            open_gate_payload(&ctx(Caller::External { direct: true }, None)),
            json!({})
        );
    }

    #[test]
    fn a_change_is_typed_and_the_rest_keeps_its_meaning() {
        let parse = |body: &str| parse_open_response(body, "r");
        assert_eq!(
            parse(r#"{"requestId":"r","ok":false,"refusal":"changedWhileAsking"}"#),
            Some(Ok(OpenAck::ChangedWhileAsking))
        );
        assert_eq!(parse(r#"{"requestId":"r","ok":true}"#), Some(Ok(OpenAck::Opened)));
        assert!(matches!(parse(r#"{"requestId":"r"}"#), Some(Err(_))));
        assert_eq!(parse(r#"{"requestId":"other","ok":true}"#), None);
    }
}
