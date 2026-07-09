//! The `favorites` tool: manage the user's favorites (add / rename / remove /
//! reorder).
//!
//! Thin adapter over the typed `commands::favorites` pass-throughs (smart backend
//! / thin frontend). Each mutation persists `favorites.json` and re-emits
//! `volumes-changed` itself, so both panes' switchers refresh live — the handler
//! invents no ack and returns the backend result directly (the `indexing` /
//! `queue` precedent, so there is no FE action to ack). Gate `Always`: persistent app-config mutation
//! with no confirmation dialog to piggyback on.
//!
//! Ids are discoverable via `cmdr://state` under `favorites:`.

use serde_json::{Value, json};

use super::{ToolError, ToolResult, expand_user_path};

pub async fn execute_favorites(params: &Value) -> ToolResult {
    let action = params
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'action' parameter"))?;

    match action {
        "add" => {
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .map(expand_user_path)
                .ok_or_else(|| ToolError::invalid_params("add requires a 'path' parameter"))?;
            let name = params.get("name").and_then(|v| v.as_str()).map(str::to_string);
            crate::commands::favorites::add_favorite(path.clone(), name)
                .await
                .map_err(|e| ToolError::internal(format!("Couldn't add favorite: {}", e.message)))?;
            Ok(json!(format!("OK: Added favorite for {path}.")))
        }
        "rename" => {
            let id = require_id(params)?;
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::invalid_params("rename requires a 'name' parameter"))?;
            crate::commands::favorites::rename_favorite(id.clone(), name.to_string())
                .await
                .map_err(|e| ToolError::internal(format!("Couldn't rename favorite: {}", e.message)))?;
            Ok(json!(format!("OK: Renamed favorite {id} to {name}.")))
        }
        "remove" => {
            let id = require_id(params)?;
            crate::commands::favorites::remove_favorite(id.clone())
                .await
                .map_err(|e| ToolError::internal(format!("Couldn't remove favorite: {}", e.message)))?;
            Ok(json!(format!("OK: Removed favorite {id}.")))
        }
        "reorder" => {
            let ordered_ids = params
                .get("orderedIds")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect::<Vec<_>>()
                })
                .filter(|ids: &Vec<String>| !ids.is_empty())
                .ok_or_else(|| {
                    ToolError::invalid_params(
                        "reorder requires a non-empty 'orderedIds' array (the complete new ordering)",
                    )
                })?;
            crate::commands::favorites::reorder_favorites(ordered_ids)
                .await
                .map_err(|e| ToolError::internal(format!("Couldn't reorder favorites: {}", e.message)))?;
            Ok(json!("OK: Reordered favorites."))
        }
        other => Err(ToolError::invalid_params(format!(
            "action must be 'add', 'rename', 'remove', or 'reorder' (got '{other}')"
        ))),
    }
}

fn require_id(params: &Value) -> Result<String, ToolError> {
    params
        .get("id")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| ToolError::invalid_params("This action requires an 'id' parameter (see cmdr://state favorites)"))
}
