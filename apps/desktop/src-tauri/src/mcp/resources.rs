//! MCP resource definitions.
//!
//! Defines resources for reading app state via the MCP protocol.
//! Resources are read-only state that agents can query.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{Emitter, Listener, Manager, Runtime, WebviewWindow};

use super::dialog_state::SoftDialogTracker;
use super::pane_state::{PaneFileEntry, PaneState, PaneStateStore, TabInfo};
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
            description: "Complete app state (both panes, volumes, dialogs, listings, recent listing errors). \
                          Supports `?include=panes,volumes,dialogs,listings,recentErrors` to project only \
                          listed sections, and `?compact=true` to drop the per-pane file lists. Examples: \
                          `cmdr://state?include=listings,recentErrors` or `cmdr://state?compact=true`."
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
            description: "Current drive indexing phase, timeline history, and database stats".to_string(),
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
struct StateOptions {
    /// Whitelist of top-level sections to include. `None` = include all.
    include: Option<std::collections::HashSet<String>>,
    /// When true, omit `files:` lists in each pane to cut the largest source of
    /// noise. The per-pane summary fields (`path`, `volumeId`, `cursor.index`,
    /// `totalFiles`, etc.) are still rendered.
    compact: bool,
}

/// Options parsed from a `cmdr://logs?...` URI.
#[derive(Debug, Clone, Default)]
struct LogOptions {
    /// Drop lines whose ISO-8601 timestamp is `<=` this value. Lines without a
    /// recognizable timestamp prefix are kept (better to surface noise than to
    /// silently drop a panic line that didn't fit the usual prefix).
    since_iso: Option<String>,
    /// Case-sensitive substring filter.
    filter: Option<String>,
    /// Max lines to return. Defaults to 100, clamped to 1000.
    limit: usize,
}

const LOG_DEFAULT_LIMIT: usize = 100;
const LOG_MAX_LIMIT: usize = 1000;
/// How far back from end-of-file to read. 5 MB easily covers the most recent
/// few thousand lines on a busy session, without slurping the whole rotated
/// log (up to 50 MB per file).
const LOG_TAIL_WINDOW_BYTES: u64 = 5 * 1024 * 1024;

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
fn split_uri(uri: &str) -> (&str, Option<&str>) {
    match uri.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (uri, None),
    }
}

fn parse_state_options(query: Option<&str>) -> StateOptions {
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

fn parse_log_options(query: Option<&str>) -> LogOptions {
    let q = parse_query(query);
    let since_iso = q.get("since").cloned().filter(|s| !s.is_empty());
    let filter = q.get("filter").cloned().filter(|s| !s.is_empty());
    let limit = q
        .get("limit")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(LOG_DEFAULT_LIMIT)
        .clamp(1, LOG_MAX_LIMIT);
    LogOptions {
        since_iso,
        filter,
        limit,
    }
}

impl StateOptions {
    fn includes(&self, section: &str) -> bool {
        match &self.include {
            None => true,
            Some(set) => set.contains(section),
        }
    }
}

/// Format a file entry in compact format.
/// Format: `i:INDEX TYPE NAME [SIZE] [DATES] [MARKERS]`
fn format_file_compact(
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

    parts.join(" ")
}

/// Build YAML for a single pane.
///
/// When `compact` is true, omits the `files:` list (the largest source of YAML
/// volume in the default state read) while keeping every summary field. The
/// per-pane `cursor`, `totalFiles`, and `loadedRange` still show, so callers
/// can still tell where the cursor is without paying for 100 file lines.
fn build_pane_yaml_with_options(state: &PaneState, indent: &str, compact: bool) -> String {
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

    // Cursor info
    lines.push(format!("{}cursor:", indent));
    lines.push(format!("{}  index: {}", indent, state.cursor_index));
    if state.view_mode == "brief" && state.cursor_index < state.files.len() {
        let cursor_file = &state.files[state.cursor_index];
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
fn format_tab_compact(tab: &TabInfo, index: usize) -> String {
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
            && let Some(tx) = tx.lock().unwrap().take()
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
            let text = build_indexing_status_text();
            (text, "text/plain")
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

/// Read the tail of the live `cmdr.log`, respecting `since` / `filter` / `limit`.
///
/// Reads up to [`LOG_TAIL_WINDOW_BYTES`] from the end of the file (5 MB), which
/// fits several thousand lines on a normal session — way more than the default
/// limit of 100. If the user passes a `since` older than the start of the
/// window, lines beyond the window are silently dropped; we keep the read
/// bounded so a 50 MB rotated log doesn't blow up MCP memory.
fn read_log_tail(opts: &LogOptions) -> Result<String, String> {
    use std::io::{Read, Seek, SeekFrom};

    let log_dir = crate::logging::log_dir().ok_or("Log directory is not configured yet")?;
    let log_path = log_dir.join("cmdr.log");

    let mut file = std::fs::File::open(&log_path).map_err(|e| format!("Can't open {}: {}", log_path.display(), e))?;
    let file_size = file
        .metadata()
        .map_err(|e| format!("Can't stat {}: {}", log_path.display(), e))?
        .len();

    let window = LOG_TAIL_WINDOW_BYTES.min(file_size);
    let start_pos = file_size.saturating_sub(window);
    file.seek(SeekFrom::Start(start_pos))
        .map_err(|e| format!("Can't seek log file: {}", e))?;
    let mut buf = Vec::with_capacity(window as usize);
    file.read_to_end(&mut buf)
        .map_err(|e| format!("Can't read log file: {}", e))?;
    // Drop a possibly-partial leading line so the first surviving line is
    // structurally intact. Only do this if we didn't read from byte 0.
    let text = String::from_utf8_lossy(&buf);
    Ok(select_log_lines(&text, start_pos == 0, opts))
}

/// Apply the `since` / `filter` / `limit` selection and per-line redaction to a
/// raw log-tail chunk, returning the joined, oldest-first result.
///
/// `skip_partial_first` is `false` when the chunk starts at byte 0 (the whole
/// file fit in the window, so the first line is intact) and `true` otherwise
/// (we read mid-file, so the leading line may be truncated).
///
/// **Redaction is mandatory.** The MCP logs resource is a third consumer of the
/// same log data the crash + error reporters scrub, so it must honor the same
/// contract: a loopback caller without filesystem read shouldn't be able to
/// exfiltrate home paths, SMB URIs, emails, or device names through
/// `cmdr://logs`. `redact_line` is a per-line `Cow` hot path (zero alloc on the
/// no-PII case), built for exactly this. Pure (no I/O), so it's unit-testable.
fn select_log_lines(text: &str, skip_partial_first: bool, opts: &LogOptions) -> String {
    let mut lines: Vec<&str> = if skip_partial_first {
        text.lines().skip(1).collect()
    } else {
        text.lines().collect()
    };

    if let Some(since) = opts.since_iso.as_deref() {
        lines.retain(|line| line_timestamp_passes_since(line, since));
    }
    if let Some(filter) = opts.filter.as_deref() {
        lines.retain(|line| line.contains(filter));
    }

    let take = opts.limit.min(lines.len());
    let start = lines.len() - take;
    let redacted: Vec<String> = lines[start..]
        .iter()
        .map(|line| crate::redact::redact_line(line).into_owned())
        .collect();
    redacted.join("\n")
}

/// Returns true when `line`'s leading ISO-8601 timestamp is strictly greater
/// than `since`. Lexicographic comparison works because both sides are
/// ISO-8601 with the same precision (millisecond) and a constant zone suffix
/// for the live log. Lines without a recognizable timestamp prefix are kept
/// (we'd rather over-include a panic line than silently drop one).
fn line_timestamp_passes_since(line: &str, since: &str) -> bool {
    // The fern logger writes lines like `2026-05-19T08:30:02.000+02:00 INFO ...`.
    // The timestamp is everything up to the first space.
    let Some(ts) = line.split_whitespace().next() else {
        return true;
    };
    if !ts.starts_with(|c: char| c.is_ascii_digit()) {
        return true;
    }
    ts.as_bytes() > since.as_bytes()
}

/// Format a duration in milliseconds as a human-readable string.
fn format_duration_human(ms: u64) -> String {
    if ms < 1_000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        let secs = ms as f64 / 1000.0;
        format!("{:.1}s", secs)
    } else if ms < 3_600_000 {
        let mins = ms / 60_000;
        let secs = (ms % 60_000) / 1000;
        if secs == 0 {
            format!("{}m", mins)
        } else {
            format!("{}m {:02}s", mins, secs)
        }
    } else {
        let hours = ms / 3_600_000;
        let mins = (ms % 3_600_000) / 60_000;
        format!("{}h {:02}m", hours, mins)
    }
}

/// Build a plain-text summary of the indexing status for the MCP resource.
fn build_indexing_status_text() -> String {
    let status = match crate::indexing::get_debug_status() {
        Ok(s) => s,
        Err(e) => return format!("Couldn't read indexing status: {e}"),
    };

    let mut lines = Vec::new();

    // Current phase
    let duration_str = format_duration_human(status.phase_duration_ms);
    lines.push(format!("Phase: {} ({})", status.activity_phase, duration_str));

    // Trigger
    if let Some(last) = status.phase_history.last()
        && !last.trigger.is_empty()
    {
        lines.push(format!("Trigger: {}", last.trigger));
    }

    // Verifying
    lines.push(format!("Verifying: {}", if status.verifying { "yes" } else { "no" }));

    // Watcher + live events
    lines.push(format!(
        "Watcher: {}, {} live events",
        if status.watcher_active { "on" } else { "off" },
        status.live_event_count,
    ));

    // DB stats
    if let (Some(entries), Some(dirs)) = (status.live_entry_count, status.live_dir_count) {
        let total_size_str = status
            .base
            .db_file_size
            .map(|s| format!(", {}", format_size(s)))
            .unwrap_or_default();
        lines.push(format!(
            "DB: {} entries, {} dirs{}",
            format_number(entries),
            format_number(dirs),
            total_size_str
        ));

        // Breakdown: main + WAL + pages
        let mut breakdown = Vec::new();
        if let Some(main) = status.db_main_size {
            breakdown.push(format!("main: {}", format_size(main)));
        }
        if let Some(wal) = status.db_wal_size
            && wal > 0
        {
            breakdown.push(format!("WAL: {}", format_size(wal)));
        }
        if let (Some(pages), Some(free)) = (status.db_page_count, status.db_freelist_count)
            && free > 0
        {
            breakdown.push(format!("{} pages, {} free", format_number(pages), format_number(free)));
        }
        if !breakdown.is_empty() {
            lines.push(format!("    ({})", breakdown.join(", ")));
        }
    }

    // Phase history
    if status.phase_history.len() > 1
        || (status.phase_history.len() == 1 && status.phase_history[0].duration_ms.is_some())
    {
        lines.push(String::new());
        lines.push("History:".to_string());
        for (i, record) in status.phase_history.iter().enumerate() {
            let is_current = i == status.phase_history.len() - 1 && record.duration_ms.is_none();
            let duration_str = match record.duration_ms {
                Some(ms) => format!("{:>8}", format_duration_human(ms)),
                None => format!("{:>8}", format_duration_human(status.phase_duration_ms)),
            };
            let phase_name = format!("{:<14}", record.phase.to_string());
            let mut line = format!("  {}  {} {}", record.started_at, phase_name, duration_str);

            // Append stats summary
            if !record.stats.is_empty() {
                let stats_str: Vec<String> = record.stats.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
                line.push_str(&format!("  {}", stats_str.join(", ")));
            }

            if is_current {
                line.push_str("  <- now");
            }

            lines.push(line);
        }
    }

    lines.join("\n")
}

/// Format a number with comma separators.
fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_count() {
        let resources = get_all_resources();
        assert_eq!(resources.len(), 5);
    }

    /// The `cmdr://logs` resource must redact PII before returning, matching the
    /// crash + error reporters. A loopback caller with no filesystem read
    /// shouldn't be able to lift home paths, emails, or SMB URIs out of the log.
    #[test]
    fn select_log_lines_redacts_pii() {
        let opts = LogOptions {
            since_iso: None,
            filter: None,
            limit: LOG_DEFAULT_LIMIT,
        };
        let raw = "2026-05-31T08:30:02.000+02:00 INFO listing /Users/dorka/SecretProject/budget.pdf\n\
                   2026-05-31T08:30:03.000+02:00 WARN contact jane.doe@example.com about smb://nas.local/share/private/file.txt";

        let out = select_log_lines(raw, false, &opts);

        // Raw PII must be gone.
        assert!(!out.contains("/Users/dorka/"), "home path leaked: {out}");
        assert!(!out.contains("SecretProject"), "custom dir name leaked: {out}");
        assert!(!out.contains("jane.doe@example.com"), "email leaked: {out}");
        assert!(!out.contains("/share/private/"), "SMB share tail leaked: {out}");
        // Redaction tokens present (path-shape preserved).
        assert!(out.contains("$HOME/"), "expected redacted home token: {out}");
        assert!(out.contains("<email>"), "expected redacted email token: {out}");
        // Non-PII log structure survives.
        assert!(out.contains("INFO listing"), "log structure dropped: {out}");
        assert!(out.contains("WARN contact"), "log structure dropped: {out}");
    }

    #[test]
    fn parse_state_options_defaults() {
        let opts = parse_state_options(None);
        assert!(opts.include.is_none());
        assert!(!opts.compact);
        assert!(opts.includes("panes"));
        assert!(opts.includes("anything"));
    }

    #[test]
    fn parse_state_options_include_filters_sections() {
        let opts = parse_state_options(Some("include=panes,listings"));
        let inc = opts.include.as_ref().unwrap();
        assert_eq!(inc.len(), 2);
        assert!(opts.includes("panes"));
        assert!(opts.includes("listings"));
        assert!(!opts.includes("volumes"));
        assert!(!opts.includes("recentErrors"));
    }

    #[test]
    fn parse_state_options_compact_truthy() {
        assert!(parse_state_options(Some("compact=true")).compact);
        assert!(parse_state_options(Some("compact=1")).compact);
        assert!(!parse_state_options(Some("compact=false")).compact);
        assert!(!parse_state_options(Some("compact=")).compact);
    }

    #[test]
    fn parse_log_options_defaults_and_clamping() {
        let opts = parse_log_options(None);
        assert_eq!(opts.limit, LOG_DEFAULT_LIMIT);
        assert!(opts.since_iso.is_none());
        assert!(opts.filter.is_none());

        let opts = parse_log_options(Some("limit=99999"));
        assert_eq!(opts.limit, LOG_MAX_LIMIT, "limit should clamp to max");

        let opts = parse_log_options(Some("limit=0"));
        assert_eq!(opts.limit, 1, "limit should floor at 1 (zero is meaningless)");
    }

    #[test]
    fn parse_log_options_decodes_percent() {
        let opts = parse_log_options(Some("filter=hello%20world&since=2026-05-19T08%3A30%3A00.000%2B02%3A00"));
        assert_eq!(opts.filter.as_deref(), Some("hello world"));
        assert_eq!(opts.since_iso.as_deref(), Some("2026-05-19T08:30:00.000+02:00"));
    }

    #[test]
    fn split_uri_no_query() {
        let (base, q) = split_uri("cmdr://state");
        assert_eq!(base, "cmdr://state");
        assert!(q.is_none());
    }

    #[test]
    fn split_uri_with_query() {
        let (base, q) = split_uri("cmdr://state?include=panes&compact=true");
        assert_eq!(base, "cmdr://state");
        assert_eq!(q, Some("include=panes&compact=true"));
    }

    #[test]
    fn line_timestamp_passes_since_basic() {
        let line = "2026-05-19T08:30:02.000+02:00 INFO foo";
        assert!(line_timestamp_passes_since(line, "2026-05-19T08:30:01.000+02:00"));
        assert!(!line_timestamp_passes_since(line, "2026-05-19T08:30:02.000+02:00"));
        assert!(!line_timestamp_passes_since(line, "2026-05-19T08:30:03.000+02:00"));
    }

    #[test]
    fn line_timestamp_passes_since_keeps_lines_without_timestamp() {
        // A panic line that doesn't start with a timestamp must not be dropped.
        assert!(line_timestamp_passes_since(
            "thread main panicked at ...",
            "2026-05-19T08:30:00.000+02:00"
        ));
        // Empty line: keep.
        assert!(line_timestamp_passes_since("", "2026-05-19T08:30:00.000+02:00"));
    }

    #[test]
    fn test_resource_uris_are_valid() {
        let resources = get_all_resources();
        for resource in resources {
            assert!(
                resource.uri.starts_with("cmdr://"),
                "Resource URI should start with cmdr://"
            );
            assert!(!resource.name.is_empty(), "Resource name should not be empty");
            assert!(
                !resource.description.is_empty(),
                "Resource description should not be empty"
            );
        }
    }

    #[test]
    fn test_all_resources_have_valid_mime_type() {
        let resources = get_all_resources();
        for resource in resources {
            assert!(
                resource.mime_type == "text/yaml" || resource.mime_type == "text/plain",
                "Resource '{}' has unexpected mime type: {}",
                resource.uri,
                resource.mime_type
            );
        }
    }

    #[test]
    fn test_no_duplicate_resource_uris() {
        let resources = get_all_resources();
        let mut uris: Vec<&str> = resources.iter().map(|r| r.uri.as_str()).collect();
        uris.sort();
        let original_len = uris.len();
        uris.dedup();
        assert_eq!(uris.len(), original_len, "Duplicate resource URIs detected");
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1024), "1 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(1048576), "1 MB");
        assert_eq!(format_size(1073741824), "1 GB");
    }

    #[test]
    fn test_format_file_compact() {
        let file = PaneFileEntry {
            name: "test.txt".to_string(),
            path: "/tmp/test.txt".to_string(),
            is_directory: false,
            size: Some(1024),
            recursive_size: None,
            modified: Some("2024-01-15".to_string()),
            recursive_size_pending: None,
        };

        // Without details
        let formatted = format_file_compact(&file, 0, false, false, false);
        assert_eq!(formatted, "i:0 f test.txt");

        // With cursor marker
        let formatted = format_file_compact(&file, 0, true, false, false);
        assert_eq!(formatted, "i:0 f test.txt [cur]");

        // With selected marker
        let formatted = format_file_compact(&file, 0, false, true, false);
        assert_eq!(formatted, "i:0 f test.txt [sel]");

        // With details
        let formatted = format_file_compact(&file, 0, true, true, true);
        assert_eq!(formatted, "i:0 f test.txt 1 KB 2024-01-15 [cur] [sel]");

        // Directory
        let dir = PaneFileEntry {
            name: "docs".to_string(),
            path: "/tmp/docs".to_string(),
            is_directory: true,
            size: None,
            recursive_size: None,
            modified: None,
            recursive_size_pending: None,
        };
        let formatted = format_file_compact(&dir, 1, false, false, false);
        assert_eq!(formatted, "i:1 d docs");

        // Directory with recursive size
        let dir_with_size = PaneFileEntry {
            name: "src".to_string(),
            path: "/tmp/src".to_string(),
            is_directory: true,
            size: None,
            recursive_size: Some(169),
            modified: Some("2026-03-19T17:33:53.000Z".to_string()),
            recursive_size_pending: None,
        };
        let formatted = format_file_compact(&dir_with_size, 5, false, false, true);
        assert_eq!(formatted, "i:5 d src 169 B 2026-03-19T17:33:53.000Z");

        // Directory whose recursive size is mid-update gets a [size-pending] marker
        // (the "size updating" hourglass, observable without DOM access).
        let pending_dir = PaneFileEntry {
            name: "target".to_string(),
            path: "/tmp/target".to_string(),
            is_directory: true,
            size: None,
            recursive_size: Some(4096),
            modified: None,
            recursive_size_pending: Some(true),
        };
        let formatted = format_file_compact(&pending_dir, 2, false, false, true);
        assert_eq!(formatted, "i:2 d target 4 KB [size-pending]");
        // The marker shows even without details (it's a status, not a detail).
        let formatted = format_file_compact(&pending_dir, 2, false, false, false);
        assert_eq!(formatted, "i:2 d target [size-pending]");
    }

    #[test]
    fn test_build_pane_yaml() {
        let state = PaneState {
            path: "/Users/test".to_string(),
            volume_id: Some("root".to_string()),
            volume_name: Some("Macintosh HD".to_string()),
            files: vec![
                PaneFileEntry {
                    name: "file1.txt".to_string(),
                    path: "/Users/test/file1.txt".to_string(),
                    is_directory: false,
                    size: Some(100),
                    recursive_size: None,
                    modified: Some("2024-01-15".to_string()),
                    recursive_size_pending: None,
                },
                PaneFileEntry {
                    name: "folder".to_string(),
                    path: "/Users/test/folder".to_string(),
                    is_directory: true,
                    size: None,
                    recursive_size: None,
                    modified: None,
                    recursive_size_pending: None,
                },
            ],
            cursor_index: 0,
            view_mode: "brief".to_string(),
            selected_indices: vec![1],
            sort_field: "name".to_string(),
            sort_order: "asc".to_string(),
            total_files: 2,
            loaded_start: 0,
            loaded_end: 2,
            show_hidden: false,
            tabs: vec![
                TabInfo {
                    id: "tab-1".to_string(),
                    path: "/Users/test".to_string(),
                    pinned: false,
                    active: true,
                },
                TabInfo {
                    id: "tab-2".to_string(),
                    path: "/Users/test/Downloads".to_string(),
                    pinned: true,
                    active: false,
                },
            ],
            type_to_jump: None,
        };

        let yaml = build_pane_yaml_with_options(&state, "  ", false);

        assert!(yaml.contains("volume: Macintosh HD"));
        assert!(yaml.contains("volumeId: root"));
        assert!(yaml.contains("path: /Users/test"));
        assert!(yaml.contains("view: brief"));
        assert!(yaml.contains("sort: \"name:asc\""));
        assert!(yaml.contains("totalFiles: 2"));
        assert!(yaml.contains("loadedRange: [0, 2]"));
        assert!(yaml.contains("selected: 1"));
        assert!(yaml.contains("[cur]")); // Cursor on first file
        assert!(yaml.contains("[sel]")); // Second file selected
        assert!(yaml.contains("tabs:"));
        assert!(yaml.contains("i:0 id:tab-1 [active] test (/Users/test)"));
        assert!(yaml.contains("i:1 id:tab-2 [pinned] Downloads (/Users/test/Downloads)"));
    }

    #[test]
    fn test_format_tab_compact_active() {
        let tab = TabInfo {
            id: "t1".to_string(),
            path: "/Users/foo/Documents".to_string(),
            pinned: false,
            active: true,
        };
        assert_eq!(
            format_tab_compact(&tab, 0),
            "i:0 id:t1 [active] Documents (/Users/foo/Documents)"
        );
    }

    #[test]
    fn test_format_tab_compact_pinned() {
        let tab = TabInfo {
            id: "t2".to_string(),
            path: "/Users/foo/Downloads".to_string(),
            pinned: true,
            active: false,
        };
        assert_eq!(
            format_tab_compact(&tab, 1),
            "i:1 id:t2 [pinned] Downloads (/Users/foo/Downloads)"
        );
    }

    #[test]
    fn test_format_tab_compact_active_and_pinned() {
        let tab = TabInfo {
            id: "t3".to_string(),
            path: "/Users/foo/Projects".to_string(),
            pinned: true,
            active: true,
        };
        assert_eq!(
            format_tab_compact(&tab, 2),
            "i:2 id:t3 [active] [pinned] Projects (/Users/foo/Projects)"
        );
    }

    #[test]
    fn test_format_tab_compact_plain() {
        let tab = TabInfo {
            id: "t4".to_string(),
            path: "/Users/foo/Desktop".to_string(),
            pinned: false,
            active: false,
        };
        assert_eq!(format_tab_compact(&tab, 3), "i:3 id:t4 Desktop (/Users/foo/Desktop)");
    }

    #[test]
    fn test_format_tab_compact_root_path() {
        let tab = TabInfo {
            id: "t5".to_string(),
            path: "/".to_string(),
            pinned: false,
            active: true,
        };
        // Root path has no non-empty segment after splitting by '/', so falls back to full path
        assert_eq!(format_tab_compact(&tab, 0), "i:0 id:t5 [active] / (/)");
    }

    #[test]
    fn test_pane_yaml_no_tabs_when_empty() {
        let state = PaneState {
            path: "/tmp".to_string(),
            volume_name: Some("Disk".to_string()),
            tabs: vec![],
            ..PaneState::default()
        };
        let yaml = build_pane_yaml_with_options(&state, "  ", false);
        assert!(!yaml.contains("tabs:"));
    }

    #[test]
    fn test_format_duration_human() {
        assert_eq!(format_duration_human(0), "0ms");
        assert_eq!(format_duration_human(500), "500ms");
        assert_eq!(format_duration_human(1_000), "1.0s");
        assert_eq!(format_duration_human(47_100), "47.1s");
        assert_eq!(format_duration_human(60_000), "1m");
        assert_eq!(format_duration_human(252_000), "4m 12s");
        assert_eq!(format_duration_human(3_600_000), "1h 00m");
        assert_eq!(format_duration_human(3_723_000), "1h 02m");
    }

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1_000), "1,000");
        assert_eq!(format_number(142_301), "142,301");
        assert_eq!(format_number(1_000_000), "1,000,000");
    }
}
