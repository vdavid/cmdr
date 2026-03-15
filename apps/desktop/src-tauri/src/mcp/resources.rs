//! MCP resource definitions.
//!
//! Defines resources for reading app state via the MCP protocol.
//! Resources are read-only state that agents can query.

use serde::{Deserialize, Serialize};
use tauri::{Manager, Runtime, WebviewWindow};

use super::dialog_state::SoftDialogTracker;
use super::pane_state::{FileEntry, PaneState, PaneStateStore, TabInfo};
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
            description: "Complete state of the Cmdr app including both panes, volumes, and dialogs".to_string(),
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
    ]
}

/// Format a file entry in compact format.
/// Format: `i:INDEX TYPE NAME [SIZE] [DATES] [MARKERS]`
fn format_file_compact(
    file: &FileEntry,
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

    parts.join(" ")
}

/// Format file size in human-readable format.
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1}G", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1}M", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1}K", bytes as f64 / KB as f64)
    } else {
        format!("{}B", bytes)
    }
}

/// Build YAML for a single pane.
fn build_pane_yaml(state: &PaneState, indent: &str) -> String {
    let mut lines = Vec::new();

    // Tabs (first — gives context for which tab is active before showing its content)
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
        }
        if let Some(ref modified) = cursor_file.modified {
            lines.push(format!("{}  modified: {}", indent, modified));
        }
    }

    // Selected count
    lines.push(format!("{}selected: {}", indent, state.selected_indices.len()));

    // Files list
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

/// Read a resource by URI.
pub fn read_resource<R: Runtime>(app: &tauri::AppHandle<R>, uri: &str) -> Result<ResourceContent, String> {
    let (content, mime_type) = match uri {
        "cmdr://state" => {
            let store = app.try_state::<PaneStateStore>().ok_or("Pane state not available")?;
            let focused = store.get_focused_pane();
            let left = store.get_left();
            let right = store.get_right();

            let mut yaml = String::new();

            // Focused pane
            yaml.push_str(&format!("focused: {}\n", focused));

            // Show hidden (use left pane's setting as the global one)
            yaml.push_str(&format!("showHidden: {}\n", left.show_hidden));

            // Left pane
            yaml.push_str("left:\n");
            yaml.push_str(&build_pane_yaml(&left, "  "));
            yaml.push('\n');

            // Right pane
            yaml.push_str("right:\n");
            yaml.push_str(&build_pane_yaml(&right, "  "));
            yaml.push('\n');

            // Volumes list
            yaml.push_str("volumes:\n");
            #[cfg(target_os = "macos")]
            {
                let locations = volumes::list_locations();
                for loc in locations {
                    yaml.push_str(&format!("  - {}\n", loc.name));
                }
                // Virtual volumes (frontend-only, not from list_locations)
                yaml.push_str("  - Network\n");
            }
            #[cfg(not(target_os = "macos"))]
            {
                yaml.push_str("  - root\n");
            }

            // Dialogs — derived from window manager + soft dialog tracker
            let mut dialog_entries: Vec<String> = Vec::new();

            // Window-based dialogs: derive from Tauri's window manager
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

            // Soft (overlay) dialogs: from tracker
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

            (yaml, "text/yaml")
        }
        "cmdr://dialogs/available" => {
            let yaml = build_available_dialogs_yaml(app);
            (yaml, "text/yaml")
        }
        "cmdr://indexing" => {
            let text = build_indexing_status_text();
            (text, "text/plain")
        }
        _ => return Err(format!("Unknown resource URI: {}", uri)),
    };

    Ok(ResourceContent {
        uri: uri.to_string(),
        mime_type: mime_type.to_string(),
        text: content,
    })
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
        assert_eq!(resources.len(), 3);
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
        assert_eq!(format_size(500), "500B");
        assert_eq!(format_size(1024), "1.0K");
        assert_eq!(format_size(1536), "1.5K");
        assert_eq!(format_size(1048576), "1.0M");
        assert_eq!(format_size(1073741824), "1.0G");
    }

    #[test]
    fn test_format_file_compact() {
        let file = FileEntry {
            name: "test.txt".to_string(),
            path: "/tmp/test.txt".to_string(),
            is_directory: false,
            size: Some(1024),
            modified: Some("2024-01-15".to_string()),
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
        assert_eq!(formatted, "i:0 f test.txt 1.0K 2024-01-15 [cur] [sel]");

        // Directory
        let dir = FileEntry {
            name: "docs".to_string(),
            path: "/tmp/docs".to_string(),
            is_directory: true,
            size: None,
            modified: None,
        };
        let formatted = format_file_compact(&dir, 1, false, false, false);
        assert_eq!(formatted, "i:1 d docs");
    }

    #[test]
    fn test_build_pane_yaml() {
        let state = PaneState {
            path: "/Users/test".to_string(),
            volume_id: Some("root".to_string()),
            volume_name: Some("Macintosh HD".to_string()),
            files: vec![
                FileEntry {
                    name: "file1.txt".to_string(),
                    path: "/Users/test/file1.txt".to_string(),
                    is_directory: false,
                    size: Some(100),
                    modified: Some("2024-01-15".to_string()),
                },
                FileEntry {
                    name: "folder".to_string(),
                    path: "/Users/test/folder".to_string(),
                    is_directory: true,
                    size: None,
                    modified: None,
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
        };

        let yaml = build_pane_yaml(&state, "  ");

        assert!(yaml.contains("volume: Macintosh HD"));
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
        let yaml = build_pane_yaml(&state, "  ");
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
