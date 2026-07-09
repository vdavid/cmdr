//! MCP resource definitions.
//!
//! Defines resources for reading app state via the MCP protocol.
//! Resources are read-only state that agents can query.
//!
//! The registry ([`get_all_resources`]), URI/query parsing, and the
//! `cmdr://state` builder live here as the shared spine. The two
//! independently-evolving plain-text builders live in their own modules:
//! [`logs`] (`cmdr://logs`) and [`indexing`] (`cmdr://indexing`).

pub(crate) mod importance;
pub(crate) mod indexing;
pub(crate) mod logs;
pub(crate) mod operations;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{Emitter, Listener, Manager, Runtime, WebviewWindow};

use logs::{parse_log_options, read_log_tail};

use super::dialog_state::SoftDialogTracker;
use super::pane_state::{PaneFileEntry, PaneState, PaneStateStore, TabInfo};
use crate::ignore_poison::IgnorePoison;
use crate::search::format_size;
#[cfg(target_os = "macos")]
use crate::volumes;

/// A resource definition for MCP.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub mime_type: String,
}

/// Resource content returned by resources/read.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceContent {
    pub uri: String,
    pub mime_type: String,
    pub text: String,
}

/// Get all available resources.
pub fn get_all_resources() -> Vec<Resource> {
    vec![
        Resource {
            uri: "cmdr://state".to_string(),
            name: "App state".to_string(),
            description: "Complete app state (both panes, volumes, dialogs, listings, recent listing errors, \
                          queued/running/paused operations with progress/speed/ETA, favorites). Supports \
                          `?include=panes,volumes,dialogs,listings,recentErrors,operations,favorites` to project \
                          only listed sections, and `?compact=true` to drop the per-pane file lists. Examples: \
                          `cmdr://state?include=operations` or `cmdr://state?compact=true`. File entries carry a \
                          `[tags:red,blue]` marker when they have Finder tags."
                .to_string(),
            mime_type: "text/yaml".to_string(),
        },
        Resource {
            uri: "cmdr://dialogs/available".to_string(),
            name: "Available dialogs".to_string(),
            description: "List of dialog types that can be opened and their parameters".to_string(),
            mime_type: "text/yaml".to_string(),
        },
        Resource {
            uri: "cmdr://indexing".to_string(),
            name: "Indexing status".to_string(),
            description: "Per-volume drive indexing status: one block per known volume with freshness \
                          (fresh/scanning/stale/off), current phase, scan progress, last scan, and DB \
                          stats. Add `?volume=<id>` for a single volume's deep debug view (phase \
                          timeline, trigger history, watcher stats)."
                .to_string(),
            mime_type: "text/plain".to_string(),
        },
        Resource {
            uri: "cmdr://importance".to_string(),
            name: "Folder importance".to_string(),
            description: "Folder-importance scores (which folders matter), offline-capable so it answers about \
                          unmounted drives. `?path=<abs-path>` gives one folder's score with its signal breakdown, \
                          or why it floors; `?top=<n>&volume=<id>` the top-N folders (volume optional); \
                          `?threshold=<f>` folders scoring at or above `f`. No query returns usage plus a \
                          per-volume overview. `~` expands to home."
                .to_string(),
            mime_type: "text/plain".to_string(),
        },
        Resource {
            uri: "cmdr://settings".to_string(),
            name: "Settings".to_string(),
            description: "All settings with current values, defaults, types, and constraints".to_string(),
            mime_type: "text/yaml".to_string(),
        },
        Resource {
            uri: "cmdr://logs".to_string(),
            name: "Recent logs".to_string(),
            description:
                "Tail of the live cmdr.log file. Query: `?since=<unix-ms-or-iso>&filter=<substring>&limit=<n>`. \
                          `limit` defaults to 100, cap 1000. `filter` is a case-sensitive substring match. \
                          `since` drops lines whose timestamp is <= the given moment. Lines come back oldest-first, \
                          one per line, in the same format the on-disk log uses."
                    .to_string(),
            mime_type: "text/plain".to_string(),
        },
    ]
}

/// Options parsed from a `cmdr://state?...` URI.
#[derive(Debug, Clone, Default)]
pub(crate) struct StateOptions {
    /// Whitelist of top-level sections to include. `None` = include all.
    pub(crate) include: Option<std::collections::HashSet<String>>,
    /// When true, omit `files:` lists in each pane to cut the largest source of
    /// noise. The per-pane summary fields (`path`, `volumeId`, `cursor.index`,
    /// `totalFiles`, etc.) are still rendered.
    pub(crate) compact: bool,
}

/// Parses `?k=v&k=v` query string into a flat map. Returns an empty map for
/// `None` or `Some("")`. Percent-decodes values (and keys).
fn parse_query(q: Option<&str>) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    let Some(q) = q else { return out };
    if q.is_empty() {
        return out;
    }
    for pair in q.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = match pair.split_once('=') {
            Some(kv) => kv,
            None => (pair, ""),
        };
        let key = urlencoding::decode(k)
            .map(|c| c.into_owned())
            .unwrap_or_else(|_| k.to_string());
        let value = urlencoding::decode(v)
            .map(|c| c.into_owned())
            .unwrap_or_else(|_| v.to_string());
        out.insert(key, value);
    }
    out
}

/// Splits a URI into `(base, query)`. `cmdr://state?a=b` → `("cmdr://state", Some("a=b"))`.
pub(crate) fn split_uri(uri: &str) -> (&str, Option<&str>) {
    match uri.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (uri, None),
    }
}

pub(crate) fn parse_state_options(query: Option<&str>) -> StateOptions {
    let q = parse_query(query);
    let include = q.get("include").map(|v| {
        v.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<std::collections::HashSet<String>>()
    });
    let compact = q.get("compact").map(|v| v == "true" || v == "1").unwrap_or(false);
    StateOptions { include, compact }
}

impl StateOptions {
    pub(crate) fn includes(&self, section: &str) -> bool {
        match &self.include {
            None => true,
            Some(set) => set.contains(section),
        }
    }
}

/// Format a file entry in compact format.
/// Format: `i:INDEX TYPE NAME [SIZE] [DATES] [MARKERS]`
pub(crate) fn format_file_compact(
    file: &PaneFileEntry,
    index: usize,
    is_cursor: bool,
    is_selected: bool,
    include_details: bool,
) -> String {
    let file_type = if file.is_directory {
        "d"
    } else if file.path.contains(" -> ") {
        "l" // symlink indicated by arrow in path
    } else {
        "f"
    };

    let mut parts = vec![format!("i:{} {} {}", index, file_type, file.name)];

    if include_details {
        if let Some(size) = file.size {
            parts.push(format_size(size));
        } else if let Some(recursive_size) = file.recursive_size {
            parts.push(format_size(recursive_size));
        }
        if let Some(ref modified) = file.modified {
            parts.push(modified.clone());
        }
    }

    if is_cursor {
        parts.push("[cur]".to_string());
    }
    if is_selected {
        parts.push("[sel]".to_string());
    }
    // The recursive size is mid-update (indexer still draining writes for this
    // dir or a descendant). Mirrors the per-row "size updating" hourglass.
    if file.recursive_size_pending == Some(true) {
        parts.push("[size-pending]".to_string());
    }
    if let Some(marker) = tags_marker(&file.tags) {
        parts.push(marker);
    }

    parts.join(" ")
}

/// The `[tags:...]` marker for a file's Finder tags, or `None` when it has none
/// (zero cost in the common case). Colored tags render as their color name (the
/// dot the UI shows); a colorless custom tag renders as its own name. Pure, so
/// it's unit-testable.
pub(crate) fn tags_marker(tags: &[crate::file_system::listing::metadata::TagRef]) -> Option<String> {
    if tags.is_empty() {
        return None;
    }
    let labels: Vec<String> = tags
        .iter()
        .map(|t| match tag_color_name(t.color) {
            Some(color) => color.to_string(),
            None => t.name.clone(),
        })
        .collect();
    Some(format!("[tags:{}]", labels.join(",")))
}

/// The lowercase color name for a Finder color index (1..=7), or `None` for the
/// colorless index 0.
fn tag_color_name(color: u8) -> Option<&'static str> {
    Some(match color {
        1 => "gray",
        2 => "green",
        3 => "purple",
        4 => "blue",
        5 => "yellow",
        6 => "red",
        7 => "orange",
        _ => return None,
    })
}

/// Build YAML for a single pane.
///
/// When `compact` is true, omits the `files:` list (the largest source of YAML
/// volume in the default state read) while keeping every summary field. The
/// per-pane `cursor`, `totalFiles`, and `loadedRange` still show, so callers
/// can still tell where the cursor is without paying for 100 file lines.
pub(crate) fn build_pane_yaml_with_options(state: &PaneState, indent: &str, compact: bool) -> String {
    let mut lines = Vec::new();

    // Tabs (first, gives context for which tab is active before showing its content)
    if !state.tabs.is_empty() {
        lines.push(format!("{}tabs:", indent));
        for (idx, tab) in state.tabs.iter().enumerate() {
            let formatted = format_tab_compact(tab, idx);
            lines.push(format!("{}  - {}", indent, formatted));
        }
    }

    // Volume and path
    lines.push(format!(
        "{}volume: {}",
        indent,
        state.volume_name.as_deref().unwrap_or("unknown")
    ));
    if let Some(ref vid) = state.volume_id {
        lines.push(format!("{}volumeId: {}", indent, vid));
    }
    lines.push(format!("{}path: {}", indent, state.path));
    lines.push(format!("{}view: {}", indent, state.view_mode));
    lines.push(format!(
        "{}sort: \"{}:{}\"",
        indent,
        if state.sort_field.is_empty() {
            "name"
        } else {
            &state.sort_field
        },
        if state.sort_order.is_empty() {
            "asc"
        } else {
            &state.sort_order
        }
    ));
    lines.push(format!("{}totalFiles: {}", indent, state.total_files));
    lines.push(format!(
        "{}loadedRange: [{}, {}]",
        indent, state.loaded_start, state.loaded_end
    ));

    // Cursor info. `cursor_index` is global while `files` holds only the loaded
    // window, so the detail lookup is window-relative; a cursor outside the
    // window shows no details rather than a wrong file's.
    lines.push(format!("{}cursor:", indent));
    lines.push(format!("{}  index: {}", indent, state.cursor_index));
    let cursor_window_index = state.cursor_index.checked_sub(state.loaded_start);
    if state.view_mode == "brief"
        && let Some(cursor_file) = cursor_window_index.and_then(|i| state.files.get(i))
    {
        lines.push(format!("{}  name: {}", indent, cursor_file.name));
        if let Some(size) = cursor_file.size {
            lines.push(format!("{}  size: {}", indent, format_size(size)));
        } else if let Some(recursive_size) = cursor_file.recursive_size {
            lines.push(format!("{}  size: {}", indent, format_size(recursive_size)));
        }
        if let Some(ref modified) = cursor_file.modified {
            lines.push(format!("{}  modified: {}", indent, modified));
        }
    }

    // Selected count
    lines.push(format!("{}selected: {}", indent, state.selected_indices.len()));

    // Type-to-jump state: only emitted while a buffer or visible indicator
    // exists, so the YAML stays clean during the common case.
    if let Some(ref ttj) = state.type_to_jump {
        lines.push(format!("{}typeToJump:", indent));
        lines.push(format!("{}  buffer: {:?}", indent, ttj.buffer));
        lines.push(format!("{}  indicatorVisible: {}", indent, ttj.indicator_visible));
        lines.push(format!("{}  indicatorStale: {}", indent, ttj.indicator_stale));
        if let Some(ref name) = ttj.last_matched_name {
            lines.push(format!("{}  lastMatchedName: {}", indent, name));
        }
    }

    // Files list
    if compact {
        // `compact` callers care about path / volumeId / cursor / totalFiles, not
        // the 100-entry virtual-scroll window. Emit a single placeholder so the
        // YAML still shows the section exists.
        lines.push(format!("{}files: <omitted: compact=true>", indent));
    } else {
        lines.push(format!("{}files:", indent));

        let is_full_mode = state.view_mode == "full";
        let selected_set: std::collections::HashSet<usize> = state.selected_indices.iter().copied().collect();

        for (idx, file) in state.files.iter().enumerate() {
            // Convert local index to global index based on loaded_start
            let global_idx = state.loaded_start + idx;
            let is_cursor = global_idx == state.cursor_index;
            let is_selected = selected_set.contains(&global_idx);
            // In full mode, include details for all files. In brief mode, only for cursor.
            let include_details = is_full_mode || is_cursor;
            let formatted = format_file_compact(file, global_idx, is_cursor, is_selected, include_details);
            lines.push(format!("{}  - \"{}\"", indent, formatted));
        }
    }

    lines.join("\n")
}

/// Format a tab entry in compact format.
/// Format: `i:INDEX id:TAB_ID [active] [pinned] FolderName (/full/path)`
pub(crate) fn format_tab_compact(tab: &TabInfo, index: usize) -> String {
    let folder_name = tab.path.rsplit('/').find(|s| !s.is_empty()).unwrap_or(&tab.path);

    let mut markers = Vec::new();
    if tab.active {
        markers.push("[active]");
    }
    if tab.pinned {
        markers.push("[pinned]");
    }

    if markers.is_empty() {
        format!("i:{} id:{} {} ({})", index, tab.id, folder_name, tab.path)
    } else {
        format!(
            "i:{} id:{} {} {} ({})",
            index,
            tab.id,
            markers.join(" "),
            folder_name,
            tab.path
        )
    }
}

/// Extract the file path from a viewer window's URL.
/// Viewer URLs look like: http://localhost:PORT/viewer?path=%2FUsers%2F...
fn extract_viewer_path<R: Runtime>(window: &WebviewWindow<R>) -> Option<String> {
    let url = window.url().ok()?;
    url.query_pairs()
        .find(|(key, _)| key == "path")
        .map(|(_, value)| value.into_owned())
}

/// Build YAML for the "available dialogs" resource.
/// Combines window-based types (hardcoded, stable) with soft dialog types
/// registered by the frontend at startup.
fn build_available_dialogs_yaml<R: Runtime>(app: &tauri::AppHandle<R>) -> String {
    let mut yaml = String::new();

    // Window-based dialog types (managed on the Rust side)
    yaml.push_str("- type: settings\n  sections: [general, appearance, shortcuts, advanced]\n");
    yaml.push_str(
        "- type: file-viewer\n  description: Opens for file under cursor, or specify path. Multiple can be open.\n",
    );

    // Soft dialog types (registered by the frontend)
    if let Some(tracker) = app.try_state::<SoftDialogTracker>() {
        for dialog in tracker.get_known_dialogs() {
            yaml.push_str(&format!("- type: {}\n", dialog.id));
            if let Some(ref desc) = dialog.description {
                yaml.push_str(&format!("  description: {}\n", desc));
            }
        }
    }

    yaml
}

/// Emit an event to the frontend and wait for a response containing data.
///
/// Similar to `mcp_round_trip` in executor.rs, but returns the `data` field from the response
/// instead of a fixed success message. The frontend must emit `mcp-response` with
/// `{ requestId, ok, data?, error? }`. Times out after 5 seconds.
async fn resource_round_trip<R: Runtime>(
    app: &tauri::AppHandle<R>,
    event: &str,
    mut payload: Value,
) -> Result<String, String> {
    let request_id = uuid::Uuid::new_v4().to_string();
    payload["requestId"] = json!(request_id);

    let (tx, rx) = tokio::sync::oneshot::channel::<Result<String, String>>();
    let expected_id = request_id.clone();

    let tx = std::sync::Mutex::new(Some(tx));
    let listener_id = app.listen("mcp-response", move |event| {
        if let Ok(resp) = serde_json::from_str::<Value>(event.payload())
            && resp.get("requestId").and_then(|v| v.as_str()) == Some(&expected_id)
            && let Some(tx) = tx.lock_ignore_poison().take()
        {
            let result = if resp.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
                let data = resp.get("data").and_then(|v| v.as_str()).unwrap_or("").to_string();
                Ok(data)
            } else {
                let err = resp
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown error")
                    .to_string();
                Err(err)
            };
            let _ = tx.send(result);
        }
    });

    app.emit(event, payload).map_err(|e| e.to_string())?;

    let result = tokio::time::timeout(std::time::Duration::from_secs(5), rx).await;
    app.unlisten(listener_id);

    match result {
        Ok(Ok(data)) => data,
        Ok(Err(_)) => Err("Frontend response channel dropped".to_string()),
        Err(_) => Err("Frontend did not respond within 5 seconds".to_string()),
    }
}

/// Read a resource by URI.
///
/// Supports query parameters on `cmdr://state` (`?include=...&compact=...`) and
/// `cmdr://logs` (`?since=...&filter=...&limit=...`). See the resource entries
/// in [`get_all_resources`] for full syntax.
pub async fn read_resource<R: Runtime>(app: &tauri::AppHandle<R>, uri: &str) -> Result<ResourceContent, String> {
    let (base, query) = split_uri(uri);
    let (content, mime_type) = match base {
        "cmdr://state" => {
            let opts = parse_state_options(query);
            (build_state_yaml(app, &opts).await?, "text/yaml")
        }
        "cmdr://dialogs/available" => {
            let yaml = build_available_dialogs_yaml(app);
            (yaml, "text/yaml")
        }
        "cmdr://indexing" => {
            let q = parse_query(query);
            let now = indexing::now_unix_seconds();
            let text = match q.get("volume") {
                Some(vid) => match indexing::snapshot_volume_indexing(vid) {
                    Some(snap) => indexing::build_volume_debug_text(&snap, now),
                    None => format!("No index found for volume '{vid}'."),
                },
                None => indexing::build_indexing_text(&indexing::snapshot_indexing(), now),
            };
            (text, "text/plain")
        }
        "cmdr://importance" => {
            let data_dir = crate::config::resolved_app_data_dir(app)?;
            let now = indexing::now_unix_seconds();
            (
                importance::build_importance_resource(&data_dir, query, now),
                "text/plain",
            )
        }
        "cmdr://settings" => {
            let text = resource_round_trip(app, "mcp-get-all-settings", json!({})).await?;
            (text, "text/yaml")
        }
        "cmdr://logs" => {
            let opts = parse_log_options(query);
            (read_log_tail(&opts)?, "text/plain")
        }
        _ => return Err(format!("Unknown resource URI: {}", uri)),
    };

    Ok(ResourceContent {
        uri: uri.to_string(),
        mime_type: mime_type.to_string(),
        text: content,
    })
}

/// Build the `cmdr://state` YAML, respecting `include` / `compact` options.
async fn build_state_yaml<R: Runtime>(app: &tauri::AppHandle<R>, opts: &StateOptions) -> Result<String, String> {
    let store = app.try_state::<PaneStateStore>().ok_or("Pane state not available")?;
    let focused = store.get_focused_pane();
    let left = store.get_left();
    let right = store.get_right();

    let generation = store.get_generation();
    let mut yaml = String::new();

    // Always present: anchors for `await` and for orienting the reader.
    yaml.push_str(&format!("generation: {}\n", generation));
    yaml.push_str(&format!("focused: {}\n", focused));
    yaml.push_str(&format!("showHidden: {}\n", left.show_hidden));

    if opts.includes("panes") {
        yaml.push_str("left:\n");
        yaml.push_str(&build_pane_yaml_with_options(&left, "  ", opts.compact));
        yaml.push('\n');

        yaml.push_str("right:\n");
        yaml.push_str(&build_pane_yaml_with_options(&right, "  ", opts.compact));
        yaml.push('\n');
    }

    if opts.includes("volumes") {
        yaml.push_str("volumes:\n");
        #[cfg(target_os = "macos")]
        {
            let mut locations = volumes::list_locations();
            // Enrich with VolumeManager-derived SMB connection state so agents
            // can see whether an SMB share is `direct` (smb2), `os_mount`
            // (fallback through macOS), or `disconnected` (smb2 dropped, FE
            // reconnect cycle running). Non-SMB volumes omit the field.
            volumes::enrich_smb_connection_state(&mut locations);
            for loc in &locations {
                if let Some(state) = loc.smb_connection_state {
                    let state_str = match state {
                        volumes::SmbConnectionState::Direct => "direct",
                        volumes::SmbConnectionState::OsMount => "os_mount",
                        volumes::SmbConnectionState::Disconnected => "disconnected",
                    };
                    yaml.push_str(&format!(
                        "  - name: {}\n    id: {}\n    smbConnectionState: {}\n",
                        loc.name, loc.id, state_str
                    ));
                } else {
                    yaml.push_str(&format!("  - {}\n", loc.name));
                }
            }
            yaml.push_str("  - Network\n");
        }
        #[cfg(not(target_os = "macos"))]
        {
            yaml.push_str("  - root\n");
        }

        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            let devices = crate::mtp::connection::connection_manager()
                .get_all_connected_devices()
                .await;
            for device_info in &devices {
                let has_multiple = device_info.storages.len() > 1;
                let device_name = device_info
                    .device
                    .product
                    .as_deref()
                    .or(device_info.device.manufacturer.as_deref())
                    .unwrap_or(&device_info.device.id);
                for storage in &device_info.storages {
                    let display_name = if has_multiple {
                        format!("{} - {}", device_name, storage.name)
                    } else {
                        device_name.to_string()
                    };
                    let volume_id = format!("{}:{}", device_info.device.id, storage.id);
                    yaml.push_str(&format!("  - name: {}\n    id: {}\n", display_name, volume_id));
                }
            }
        }
    }

    if opts.includes("dialogs") {
        let mut dialog_entries: Vec<String> = Vec::new();
        let windows = app.webview_windows();
        if windows.contains_key("settings") {
            dialog_entries.push("  - type: settings".to_string());
        }
        for (label, window) in &windows {
            if label.starts_with("viewer-") {
                if let Some(path) = extract_viewer_path(window) {
                    dialog_entries.push(format!("  - type: file-viewer\n    path: \"{}\"", path));
                } else {
                    dialog_entries.push("  - type: file-viewer".to_string());
                }
            }
        }
        if let Some(tracker) = app.try_state::<SoftDialogTracker>() {
            for dialog_type in tracker.get_open_types() {
                dialog_entries.push(format!("  - type: {}", dialog_type));
            }
        }
        if dialog_entries.is_empty() {
            yaml.push_str("dialogs: []\n");
        } else {
            yaml.push_str("dialogs:\n");
            for entry in &dialog_entries {
                yaml.push_str(entry);
                yaml.push('\n');
            }
        }
    }

    if opts.includes("listings") {
        let listings = crate::file_system::listing::caching::snapshot_listings();
        if listings.is_empty() {
            yaml.push_str("listings: []\n");
        } else {
            yaml.push_str("listings:\n");
            for l in &listings {
                yaml.push_str(&format!(
                    "  - id: {}\n    volumeId: {}\n    path: {:?}\n    entries: {}\n    ageMs: {}\n",
                    l.listing_id, l.volume_id, l.path, l.entry_count, l.age_ms
                ));
            }
        }
    }

    if opts.includes("favorites") {
        // The user's favorites (id, name, path) so agents can discover the ids
        // the `favorites` tool's rename / remove / reorder actions take. Paths
        // are user-chosen navigation targets shown in the switcher, so — like the
        // `listings:` section — they render unredacted.
        let favorites = crate::favorites::store::list();
        if favorites.is_empty() {
            yaml.push_str("favorites: []\n");
        } else {
            yaml.push_str("favorites:\n");
            for fav in &favorites {
                yaml.push_str(&format!(
                    "  - id: {}\n    name: {:?}\n    path: {:?}\n",
                    fav.id, fav.name, fav.path
                ));
            }
        }
    }

    if opts.includes("operations") {
        let ops = operations::snapshot_operations();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        yaml.push_str(&operations::build_operations_yaml(&ops, now_ms));
    }

    if opts.includes("recentErrors") {
        let errors = super::listing_errors::snapshot();
        if errors.is_empty() {
            yaml.push_str("recentErrors: []\n");
        } else {
            yaml.push_str("recentErrors:\n");
            for e in &errors {
                // `path` / `message` come from failed directory listings and can
                // carry SMB URIs or home paths the user never saw rendered.
                // Redact them so `cmdr://state` matches the same contract as
                // `cmdr://logs` and the crash/error reporters.
                let path = crate::redact::redact_line(&e.path);
                let message = crate::redact::redact_line(&e.message);
                yaml.push_str(&format!(
                    "  - atUnixMs: {}\n    listingId: {}\n    volumeId: {}\n    path: {:?}\n    message: {:?}\n",
                    e.at_unix_ms, e.listing_id, e.volume_id, path, message
                ));
            }
        }
    }

    Ok(yaml)
}
