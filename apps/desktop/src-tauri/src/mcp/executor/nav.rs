//! Navigation tool handlers.

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::{
    AckSignal, NAV_ACK_TIMEOUT, NavAck, PaneStateStore, ToolError, ToolResult, mcp_nav_round_trip, mcp_round_trip,
    mcp_round_trip_ack, mcp_round_trip_with_timeout, snapshot_generation, user_path_param, validate_path_exists,
    wait_for_ack,
};

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
    }
}

/// Take the focused pane to `dir` and point it at `entries`: cursor on the first, all of
/// them selected when there's more than one. An empty `entries` just opens `dir`.
///
/// The one implementation of "go there and show me that", shared by the
/// `go_to_latest_download` tool and the OS reveal handler (`crate::reveal`). Both drive
/// the same three frontend events, and a second copy would drift on the parts that are
/// easy to get wrong: the typed landing check that stops a cursor move in the wrong
/// directory, and the pane the move applies to.
///
/// Ordering is load-bearing: the cursor moves before the multi-select, because the
/// frontend's cursor move resets the selection.
pub(crate) async fn go_to_in_focused_pane<R: Runtime>(
    app: &AppHandle<R>,
    dir: &str,
    entries: &[String],
) -> Result<(), ToolError> {
    let pane = app
        .try_state::<PaneStateStore>()
        .map(|store| store.get_focused_pane())
        .unwrap_or_else(|| "left".to_string());

    // Reuses the FE's `mcp-nav-to-path` handler, so every volume / listing edge case it
    // already handles applies here too — including the typed landing outcome, so a pane
    // that fell back somewhere else stops the flow instead of moving a cursor in the
    // wrong directory.
    let ack = mcp_nav_round_trip(app, json!({"pane": pane, "path": dir}), NAV_TO_PATH_TIMEOUT_SECS).await?;
    nav_result(&pane, dir, ack)?;

    let Some(first) = entries.first() else {
        return Ok(());
    };
    // If the entry vanished between resolving it and the FE placing the cursor, the FE
    // says so through `mcp-response` and the caller reports it. Jump-then-vanish is
    // acceptable to leak through: the navigation completed, only the cursor missed.
    mcp_round_trip_ack(app, "mcp-move-cursor", json!({"pane": pane, "to": first})).await?;

    if entries.len() > 1 {
        mcp_round_trip_ack(
            app,
            "mcp-select-names",
            json!({"pane": pane, "names": entries, "mode": "replace"}),
        )
        .await?;
    }
    Ok(())
}

/// Execute a navigation command without parameters.
/// These emit keyboard-equivalent events to the frontend.
///
/// Ack contract:
/// - `nav_to_parent`, `nav_back`, `nav_forward` → pane generation must advance (path changes get
///   pushed via `update_*_pane_state`).
/// - `open_under_cursor` → round-trip via `mcp-open-under-cursor`. The FE awaits the resolved
///   action (directory navigation, viewer window, or OS open-with-default) and emits
///   `mcp-response`. We can't rely on `GenerationAdvanced` or `WindowAppeared` here because the
///   OS-open branch produces neither — `openFile()` hands the path to the OS default and returns,
///   no state push, no viewer window. The round-trip is the only honest ack for this multi-mode
///   command.
///
/// The `nav_to_parent` / `nav_back` / `nav_forward` family uses `NAV_ACK_TIMEOUT` (5 s)
/// instead of the default 1500 ms because navigation can touch a remote backend
/// (SMB/MTP), whose directory listing can take a few seconds even on success. With the
/// default budget, every remote-share `Enter` would surface a false-negative timeout
/// while the navigation actually succeeded in the background.
pub async fn execute_nav_command<R: Runtime>(app: &AppHandle<R>, name: &str) -> ToolResult {
    if name == "open_under_cursor" {
        // Round-trip: FE awaits `handleNavigate(entry)` and emits `mcp-response`.
        // 5 s timeout covers the slow case (large directory listing) without being
        // open-ended.
        return mcp_round_trip_with_timeout(
            app,
            "mcp-open-under-cursor",
            json!({}),
            "OK: Opened item under cursor".to_string(),
            5,
        )
        .await;
    }

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
pub async fn execute_nav_command_with_params<R: Runtime>(app: &AppHandle<R>, name: &str, params: &Value) -> ToolResult {
    match name {
        "select_volume" => {
            let pane = params
                .get("pane")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::invalid_params("Missing 'pane' parameter"))?;
            let volume_name = params
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::invalid_params("Missing 'name' parameter"))?;

            if !["left", "right"].contains(&pane) {
                return Err(ToolError::invalid_params("pane must be 'left' or 'right'"));
            }

            // Validate that the volume exists
            #[cfg(target_os = "macos")]
            {
                let locations = crate::volumes::list_locations();
                let is_virtual = volume_name == crate::volume_listing::SERVERS_VOLUME_NAME;
                let is_local = locations.iter().any(|loc| loc.name == volume_name);

                // Check MTP volumes if not a local or virtual volume
                let is_mtp = if !is_virtual && !is_local {
                    let devices = crate::mtp::connection_manager().get_all_connected_devices().await;
                    devices.iter().any(|d| {
                        let has_multiple = d.storages.len() > 1;
                        let device_name = d
                            .device
                            .product
                            .as_deref()
                            .or(d.device.manufacturer.as_deref())
                            .unwrap_or(&d.device.id);
                        d.storages.iter().any(|s| {
                            let name = if has_multiple {
                                format!("{} - {}", device_name, s.name)
                            } else {
                                device_name.to_string()
                            };
                            name == volume_name
                        })
                    })
                } else {
                    false
                };

                if !is_virtual && !is_local && !is_mtp {
                    let mut available: Vec<&str> = locations.iter().map(|l| l.name.as_str()).collect();
                    available.push(crate::volume_listing::SERVERS_VOLUME_NAME);
                    return Err(ToolError::invalid_params(format!(
                        "Volume '{}' not found. Available volumes: {}",
                        volume_name,
                        available.join(", ")
                    )));
                }
            }

            let store = app
                .try_state::<PaneStateStore>()
                .ok_or_else(|| ToolError::internal("Pane state not available"))?;
            store.set_focused_pane(pane.to_string());
            app.emit("mcp-volume-select", json!({"pane": pane, "name": volume_name}))?;

            // Wait for the target pane's `volume_name` to match the requested
            // name. This is the actual condition we care about and works
            // uniformly for local, MTP, SMB, and virtual (Network) volumes.
            //
            // The previous "wait for `path` to change" formulation had two
            // false-timeout failure modes:
            //   1. Re-selecting the same volume: path doesn't change → 30s timeout (a no-op should succeed
            //      instantly).
            //   2. Switching to the virtual `Network` volume: the FE swaps in NetworkBrowser without changing
            //      the pane path, so polling for path change deadlocked. This was the actual root cause of the
            //      SMB tests' `retries: 1` paying ~30s per first-run failure.
            //
            // `volume_name` flows through `PaneState` via `update_left_pane_state`
            // / `update_right_pane_state` on every FE-side state push (see
            // `FilePane.svelte`). For an already-on-target volume the first poll
            // matches immediately; for a real switch we wait for the FE to push
            // the new name.
            let target_name = volume_name.to_string();
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
            let poll_interval = std::time::Duration::from_millis(250);
            loop {
                let current_name = match pane {
                    "left" => store.get_left().volume_name,
                    "right" => store.get_right().volume_name,
                    _ => unreachable!(),
                };
                if current_name.as_deref() == Some(target_name.as_str()) {
                    break;
                }
                if tokio::time::Instant::now() >= deadline {
                    return Err(ToolError::internal(format!(
                        "Timed out waiting for volume '{volume_name}' to load on {pane} pane (last seen volume_name: {current_name:?})"
                    )));
                }
                tokio::time::sleep(poll_interval).await;
            }
            Ok(json!(format!("OK: Switched {pane} pane to volume {volume_name}")))
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

            let ack = mcp_nav_round_trip(app, json!({"pane": pane, "path": path}), NAV_TO_PATH_TIMEOUT_SECS).await?;
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
