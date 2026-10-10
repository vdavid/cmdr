//! MCP resource definitions.
//!
//! Defines resources for reading app state via the MCP protocol.
//! Resources are read-only state that agents can query.
//!
//! The registry ([`get_all_resources`]), URI/query parsing, and the
//! `cmdr://state` builder live here as the shared spine. The two
//! independently-evolving plain-text builders live in their own modules:
//! [`logs`] (`cmdr://logs`) and [`indexing`] (`cmdr://indexing`).

pub(crate) mod favorites;
pub(crate) mod importance;
pub(crate) mod indexing;
pub(crate) mod logs;
pub(crate) mod operations;
pub(crate) mod panes;
pub(crate) mod volumes;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{Emitter, Listener, Manager, Runtime, WebviewWindow};

use logs::{parse_log_options, read_log_tail};

use super::dialog_state::SoftDialogTracker;
use super::pane_state::PaneStateStore;
use crate::ignore_poison::IgnorePoison;

use panes::build_pane_yaml_with_options;

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

/// Extract the file path from a viewer window's URL.
/// Viewer URLs look like: http://localhost:PORT/viewer?path=%2FUsers%2F...
fn extract_viewer_path<R: Runtime>(window: &WebviewWindow<R>) -> Option<String> {
    let url = window.url().ok()?;
    url.query_pairs()
        .find(|(key, _)| key == "path")
        .map(|(_, value)| value.into_owned())
}

/// The `archive-password` dialog entry: everything an answer needs, and the one
/// thing that says whether the last answer was wrong.
///
/// ❌ The password is never here. This block carries the QUESTION; the answer
/// goes straight to the archive volume through `unlock_archive`.
///
/// `mode` is what a reader has to branch on: unlocking a `browse` finishes it
/// (a listing is a read), while unlocking a `transfer` stores the password and
/// nothing more — the copy that raised it is already settled, so extracting
/// means starting a copy again, through the gate every copy goes through.
pub(crate) fn format_archive_password_dialog(prompt: &crate::mcp::archive_password::ArchivePasswordPrompt) -> String {
    let mut yaml = String::from("  - type: archive-password\n");
    yaml.push_str(&format!("    archive: {:?}\n", prompt.archive_name));
    yaml.push_str(&format!("    archivePath: {:?}\n", prompt.archive_path));
    yaml.push_str(&format!("    mode: {}\n", prompt.mode.token()));
    yaml.push_str(&format!("    wrongAttempt: {}\n", prompt.wrong_attempt));
    if let Some(operation_id) = &prompt.operation_id {
        // Already settled — a handle for correlating with the copy you started,
        // not a row you'll find under `operations:`.
        yaml.push_str(&format!("    settledOperationId: {}\n", operation_id));
    }
    yaml.push_str("    answerWith: unlock_archive\n");
    // Trailing newline is added by the caller, which joins entries with one.
    yaml.pop();
    yaml
}

/// Build YAML for the "available dialogs" resource.
/// Combines window-based types (hardcoded, stable) with soft dialog types
/// registered by the frontend at startup.
fn build_available_dialogs_yaml<R: Runtime>(app: &tauri::AppHandle<R>) -> String {
    let known = app
        .try_state::<SoftDialogTracker>()
        .map(|tracker| tracker.get_known_dialogs())
        .unwrap_or_default();
    format_available_dialogs_yaml(&known)
}

/// Pure YAML builder for `cmdr://dialogs/available`: the two hardcoded window-based
/// types followed by every FE-registered soft dialog, each carrying its
/// `dialog-registry.ts` description when present. Split out so the description
/// round-trip is unit-testable without a live tracker.
pub(crate) fn format_available_dialogs_yaml(known: &[super::dialog_state::KnownDialog]) -> String {
    let mut yaml = String::new();

    // Window-based dialog types (managed on the Rust side)
    yaml.push_str("- type: settings\n  sections: [general, appearance, shortcuts, advanced]\n");
    yaml.push_str(
        "- type: file-viewer\n  description: Opens for file under cursor, or specify path. Multiple can be open.\n",
    );

    // Soft dialog types (registered by the frontend)
    for dialog in known {
        yaml.push_str(&format!("- type: {}\n", dialog.id));
        if let Some(ref desc) = dialog.description {
            yaml.push_str(&format!("  description: {desc}\n"));
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
        yaml.push_str(&build_pane_yaml_with_options(&left, "  ", opts));
        yaml.push('\n');

        yaml.push_str("right:\n");
        yaml.push_str(&build_pane_yaml_with_options(&right, "  ", opts));
        yaml.push('\n');
    }

    // One completed listing for both sections that read it.
    let listing = if opts.includes("volumes") || opts.includes("favorites") {
        Some(volumes::current_listing().await)
    } else {
        None
    };

    if let Some((rows, _)) = listing.as_ref().filter(|_| opts.includes("volumes")) {
        let snapshot = volumes::snapshot_volumes_from(rows).await;
        yaml.push_str(&volumes::build_volumes_yaml(&snapshot));
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
            let prompt = app
                .try_state::<crate::mcp::ArchivePasswordPromptStore>()
                .and_then(|store| store.get());
            for dialog_type in tracker.get_open_types() {
                // The archive-password prompt is the one soft dialog whose id
                // isn't enough to act on: an answer has to name the archive.
                if dialog_type == "archive-password"
                    && let Some(prompt) = prompt.as_ref()
                {
                    dialog_entries.push(format_archive_password_dialog(prompt));
                } else {
                    dialog_entries.push(format!("  - type: {}", dialog_type));
                }
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

    if let Some((rows, timed_out)) = listing.as_ref().filter(|_| opts.includes("favorites")) {
        // Each favorite with its volume and reach, off the same listing the app shows,
        // or the store's own list when that listing's discovery came up short.
        let favorites = if *timed_out {
            favorites::from_store()
        } else {
            favorites::from_listing(rows)
        };
        yaml.push_str(&favorites::build_favorites_yaml(&favorites));
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
        yaml.push_str(&build_recent_errors_yaml(&errors));
    }

    Ok(yaml)
}

/// Pure YAML builder for the `recentErrors:` section. The identifiers remain functional
/// MCP values; only the path and diagnostic prose pass through the redactor.
pub(crate) fn build_recent_errors_yaml(errors: &[super::listing_errors::RecentListingError]) -> String {
    if errors.is_empty() {
        return "recentErrors: []\n".to_string();
    }

    let mut yaml = String::from("recentErrors:\n");
    for error in errors {
        // `path` / `message` come from failed directory listings and can carry SMB URIs or
        // home paths the user never saw rendered. Unsalted: bare tokens, no report-local key.
        let path = crate::redact::redact_line(&error.path);
        let message = crate::redact::redact_line(&error.message);
        yaml.push_str(&format!(
            "  - atUnixMs: {}\n    listingId: {}\n    volumeId: {}\n    path: {:?}\n    message: {:?}\n",
            error.at_unix_ms, error.listing_id, error.volume_id, path, message
        ));
    }
    yaml
}
