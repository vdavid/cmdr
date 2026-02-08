//! MCP resource definitions.
//!
//! Defines resources for reading app state via the MCP protocol.
//! Resources are read-only state that agents can query.

use serde::{Deserialize, Serialize};
use tauri::{Manager, Runtime, WebviewWindow};

use super::dialog_state::SoftDialogTracker;
use super::pane_state::{FileEntry, PaneState, PaneStateStore};
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
        _ => return Err(format!("Unknown resource URI: {}", uri)),
    };

    Ok(ResourceContent {
        uri: uri.to_string(),
        mime_type: mime_type.to_string(),
        text: content,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_count() {
        let resources = get_all_resources();
        assert_eq!(resources.len(), 2);
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
    fn test_all_resources_have_yaml_mime_type() {
        let resources = get_all_resources();
        for resource in resources {
            assert_eq!(resource.mime_type, "text/yaml");
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
    }
}
