//! View tool handlers (toggle_hidden, set_view_mode, sort).

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::{AckSignal, DEFAULT_ACK_TIMEOUT, PaneStateStore, ToolError, ToolResult, snapshot_generation, wait_for_ack};
use crate::commands::menu_state::toggle_hidden_files;

/// Execute toggle_hidden command. Ack: pane generation advances when the FE re-pushes
/// state with the new visibility flag.
pub async fn execute_toggle_hidden<R: Runtime>(app: &AppHandle<R>) -> ToolResult {
    let pre_gen = snapshot_generation(app);
    let result = toggle_hidden_files(app.clone()).map_err(ToolError::internal)?;
    wait_for_ack(
        app,
        AckSignal::GenerationAdvanced { from: pre_gen },
        DEFAULT_ACK_TIMEOUT,
    )
    .await?;
    let state = if result { "visible" } else { "hidden" };
    Ok(json!(format!("OK: Hidden files now {state}")))
}

/// Execute set_view_mode command. Ack: pane generation advances.
pub async fn execute_set_view_mode<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let pane = params
        .get("pane")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'pane' parameter"))?;
    let mode = params
        .get("mode")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'mode' parameter"))?;

    if !["left", "right"].contains(&pane) {
        return Err(ToolError::invalid_params("pane must be 'left' or 'right'"));
    }
    if !["brief", "full"].contains(&mode) {
        return Err(ToolError::invalid_params("mode must be 'brief' or 'full'"));
    }

    let mut already_there = false;
    if let Some(store) = app.try_state::<PaneStateStore>() {
        store.set_focused_pane(pane.to_string());
        let state = if pane == "left" {
            store.get_left()
        } else {
            store.get_right()
        };
        already_there = state.view_mode == mode;
    }

    // Nothing changes, so the frontend pushes nothing, and waiting would call a pane already in
    // this view an unknown outcome. The event still goes out, in case this copy of the state lags.
    let pre_gen = snapshot_generation(app);
    app.emit("mcp-set-view-mode", json!({"pane": pane, "mode": mode}))?;
    if !already_there {
        wait_for_ack(
            app,
            AckSignal::GenerationAdvanced { from: pre_gen },
            DEFAULT_ACK_TIMEOUT,
        )
        .await?;
    }
    Ok(json!(format!("OK: Set {pane} pane to {mode} view")))
}

/// Execute unified sort command. Ack: pane generation advances (the FE re-orders and
/// re-pushes state).
pub async fn execute_sort<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let pane = params
        .get("pane")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'pane' parameter"))?;
    let by = params
        .get("by")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'by' parameter"))?;
    let order = params
        .get("order")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'order' parameter"))?;

    if !["left", "right"].contains(&pane) {
        return Err(ToolError::invalid_params("pane must be 'left' or 'right'"));
    }
    if !["name", "ext", "size", "modified", "created", "added"].contains(&by) {
        return Err(ToolError::invalid_params(
            "by must be 'name', 'ext', 'size', 'modified', 'created', or 'added'",
        ));
    }
    if !["asc", "desc"].contains(&order) {
        return Err(ToolError::invalid_params("order must be 'asc' or 'desc'"));
    }

    if let Some(store) = app.try_state::<PaneStateStore>() {
        store.set_focused_pane(pane.to_string());
    }

    let pre_gen = snapshot_generation(app);
    app.emit("mcp-sort", json!({"pane": pane, "by": by, "order": order}))?;
    wait_for_ack(
        app,
        AckSignal::GenerationAdvanced { from: pre_gen },
        DEFAULT_ACK_TIMEOUT,
    )
    .await?;

    let order_name = if order == "asc" { "ascending" } else { "descending" };
    Ok(json!(format!("OK: Sorted {pane} pane by {by} ({order_name})")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_state::pane_state::PaneState;

    /// Found in a live run: a pane in `full` view asked for `full` waited out the ack and came
    /// back an unknown outcome, because the frontend had nothing to push.
    #[tokio::test]
    async fn a_view_the_pane_already_has_is_an_ok_with_nothing_to_wait_for() {
        let app = tauri::test::mock_app();
        let store = PaneStateStore::new();
        store.set_right(PaneState {
            view_mode: "full".to_string(),
            ..Default::default()
        });
        app.manage(store);

        let answer = execute_set_view_mode(app.handle(), &json!({ "pane": "right", "mode": "full" })).await;

        assert_eq!(
            answer.map_err(|error| error.message),
            Ok(json!("OK: Set right pane to full view"))
        );
    }
}
