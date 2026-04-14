//! Navigation tool handlers.

use std::path::Path;

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::{PaneStateStore, ToolError, ToolResult, mcp_round_trip, mcp_round_trip_with_timeout};

/// Execute a navigation command without parameters.
/// These emit keyboard-equivalent events to the frontend.
pub fn execute_nav_command<R: Runtime>(app: &AppHandle<R>, name: &str) -> ToolResult {
    let key = match name {
        "open_under_cursor" => "Enter",
        "nav_to_parent" => "Backspace",
        "nav_back" => "GoBack",       // Custom event, handled by frontend
        "nav_forward" => "GoForward", // Custom event
        _ => return Err(ToolError::invalid_params(format!("Unknown nav command: {name}"))),
    };

    let action = match name {
        "open_under_cursor" => "Opened item under cursor",
        "nav_to_parent" => "Navigated to parent directory",
        "nav_back" => "Navigated back",
        "nav_forward" => "Navigated forward",
        _ => "Navigation action completed",
    };

    app.emit("mcp-key", json!({"key": key}))?;
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
                let is_virtual = volume_name == "Network";
                let is_local = locations.iter().any(|loc| loc.name == volume_name);

                // Check MTP volumes if not a local or virtual volume
                let is_mtp = if !is_virtual && !is_local {
                    let devices = crate::mtp::connection::connection_manager()
                        .get_all_connected_devices()
                        .await;
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
                    available.push("Network");
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
            let path_before = match pane {
                "left" => store.get_left().path,
                "right" => store.get_right().path,
                _ => unreachable!(),
            };
            app.emit("mcp-volume-select", json!({"pane": pane, "name": volume_name}))?;

            // Wait for the target pane's path to change (meaning the volume switch
            // and directory listing completed, and state was pushed to the store).
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
            let poll_interval = std::time::Duration::from_millis(250);
            loop {
                let current_path = match pane {
                    "left" => store.get_left().path,
                    "right" => store.get_right().path,
                    _ => unreachable!(),
                };
                if current_path != path_before {
                    break;
                }
                if tokio::time::Instant::now() >= deadline {
                    return Err(ToolError::internal(format!(
                        "Timed out waiting for volume '{volume_name}' to load on {pane} pane"
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
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::invalid_params("Missing 'path' parameter"))?;

            if !["left", "right"].contains(&pane) {
                return Err(ToolError::invalid_params("pane must be 'left' or 'right'"));
            }

            // Validate that the path exists (skip for mtp:// virtual paths)
            if !path.starts_with("mtp://") && !Path::new(path).exists() {
                return Err(ToolError::invalid_params(format!("Path does not exist: {}", path)));
            }

            if let Some(store) = app.try_state::<PaneStateStore>() {
                store.set_focused_pane(pane.to_string());
            }

            mcp_round_trip_with_timeout(
                app,
                "mcp-nav-to-path",
                json!({"pane": pane, "path": path}),
                format!("OK: Navigated {pane} pane to {path}"),
                30,
            )
            .await
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
