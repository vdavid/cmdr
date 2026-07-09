//! Pane state storage for MCP context tools.
//!
//! Stores the current state of both panes so MCP tools can access it.

use crate::ignore_poison::RwLockIgnorePoison;
use serde::{Deserialize, Serialize};
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Manager};

/// Represents a tab in a pane (for MCP state reporting).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TabInfo {
    pub id: String,
    pub path: String,
    pub pinned: bool,
    pub active: bool,
}

/// Represents a file entry in a pane (simplified subset of the main FileEntry).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PaneFileEntry {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub size: Option<u64>,
    pub recursive_size: Option<u64>,
    pub modified: Option<String>,
    /// `Some(true)` while the indexer still has unprocessed writes affecting
    /// this directory or a descendant, so its recursive size is mid-update.
    /// Surfaced in `cmdr://state` as a `[size-pending]` marker so agents can
    /// observe the "size updating" hourglass without DOM access. `None`/`false`
    /// once the writer drains. Only meaningful for directories.
    pub recursive_size_pending: Option<bool>,
    /// macOS Finder tags on the entry, mirrored from the FE listing (filled
    /// visible-range-first by the deferred `enrich_tags` pass). Surfaced in
    /// `cmdr://state` as a `[tags:red,blue]` marker only when non-empty, so an
    /// agent sees the same colored dots the UI shows. Empty in the common case.
    #[serde(default)]
    pub tags: Vec<crate::file_system::listing::metadata::TagRef>,
}

/// State of a single pane.
#[derive(Debug, Clone, Serialize, Deserialize, Default, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PaneState {
    pub path: String,
    pub volume_id: Option<String>,
    pub volume_name: Option<String>,
    pub files: Vec<PaneFileEntry>,
    /// 0-based.
    pub cursor_index: usize,
    pub view_mode: String,
    #[serde(default)]
    pub selected_indices: Vec<usize>,
    #[serde(default)]
    pub sort_field: String,
    #[serde(default)]
    pub sort_order: String,
    #[serde(default)]
    pub total_files: usize,
    #[serde(default)]
    pub loaded_start: usize,
    #[serde(default)]
    pub loaded_end: usize,
    #[serde(default)]
    pub show_hidden: bool,
    #[serde(default)]
    pub tabs: Vec<TabInfo>,
    /// Type-to-jump state mirror. `None` when no buffer is active; populated
    /// while the user is typing for in-directory navigation. Lets MCP-driven
    /// E2E tests drive and assert the feature without poking at the DOM.
    ///
    /// Note: cannot use `skip_serializing_if` here, because specta's unified-phase
    /// serde validator rejects conditional omission. The field is always
    /// present in the wire format (as `null` when inactive); the YAML
    /// resource layer suppresses the section when it's `None`.
    #[serde(default)]
    pub type_to_jump: Option<TypeToJumpInfo>,
}

/// Snapshot of a pane's type-to-jump state for MCP exposure.
///
/// `bufferActive` and `indicatorVisible` track the asymmetric timeout model
/// (buffer resets at 1 s by default, indicator stays visible until 5 s) so
/// agents can distinguish "actively typing" from "still on screen but stale".
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TypeToJumpInfo {
    /// Current buffer the user has typed. Empty string once the 1 s reset
    /// fires while the indicator is still visible (stale).
    pub buffer: String,
    /// Indicator chip is visible (in either active or stale state).
    pub indicator_visible: bool,
    /// Indicator is in the dimmed "stale" state: buffer cleared but the
    /// chip hasn't hidden yet. Next keystroke starts fresh.
    pub indicator_stale: bool,
    /// Name of the file the last successful match landed on, if any. Lets
    /// tests assert where the cursor jumped to without re-deriving it from
    /// `cursor_index` + `files`.
    #[serde(default)]
    pub last_matched_name: Option<String>,
}

/// Shared state for both panes.
#[derive(Debug)]
pub struct PaneStateStore {
    pub left: RwLock<PaneState>,
    pub right: RwLock<PaneState>,
    pub focused_pane: RwLock<String>,
    /// Monotonically increasing counter, bumped on every pane state update.
    /// Used by the `await` tool to detect stale state.
    pub generation: AtomicU64,
}

impl Default for PaneStateStore {
    fn default() -> Self {
        Self {
            left: RwLock::new(PaneState::default()),
            right: RwLock::new(PaneState::default()),
            focused_pane: RwLock::new("left".to_string()),
            generation: AtomicU64::new(0),
        }
    }
}

impl PaneStateStore {
    pub fn new() -> Self {
        Self {
            left: RwLock::new(PaneState::default()),
            right: RwLock::new(PaneState::default()),
            focused_pane: RwLock::new("left".to_string()),
            generation: AtomicU64::new(0),
        }
    }

    pub fn get_left(&self) -> PaneState {
        self.left.read_ignore_poison().clone()
    }

    pub fn get_right(&self) -> PaneState {
        self.right.read_ignore_poison().clone()
    }

    pub fn get_focused_pane(&self) -> String {
        self.focused_pane.read_ignore_poison().clone()
    }

    pub fn set_left(&self, state: PaneState) {
        *self.left.write_ignore_poison() = state;
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_right(&self, state: PaneState) {
        *self.right.write_ignore_poison() = state;
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    /// Update the tab list for a single pane. Mirrors `set_left`/`set_right` in
    /// bumping the generation counter so the MCP `tab` action tool's ack signal
    /// (`GenerationAdvanced`) fires when the FE confirms a tab change. Returns
    /// `false` if `pane` is neither `"left"` nor `"right"` (the caller already
    /// dispatched the event, so silent drop matches the prior behavior).
    pub fn set_tabs(&self, pane: &str, tabs: Vec<TabInfo>) -> bool {
        let pane_state = match pane {
            "left" => &self.left,
            "right" => &self.right,
            _ => return false,
        };
        pane_state.write_ignore_poison().tabs = tabs;
        self.generation.fetch_add(1, Ordering::Relaxed);
        true
    }

    pub fn get_generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    pub fn set_focused_pane(&self, pane: String) {
        *self.focused_pane.write_ignore_poison() = pane;
    }
}

/// Tauri command to update left pane state from frontend.
/// Preserves `tabs` (those are synced separately via `update_pane_tabs`).
#[tauri::command]
#[specta::specta]
pub fn update_left_pane_state(app: AppHandle, state: PaneState) {
    if let Some(store) = app.try_state::<PaneStateStore>() {
        let tabs = store.left.read_ignore_poison().tabs.clone();
        let mut state = state;
        state.tabs = tabs;
        store.set_left(state);
    }
}

/// Tauri command to update right pane state from frontend.
/// Preserves `tabs` (those are synced separately via `update_pane_tabs`).
#[tauri::command]
#[specta::specta]
pub fn update_right_pane_state(app: AppHandle, state: PaneState) {
    if let Some(store) = app.try_state::<PaneStateStore>() {
        let tabs = store.right.read_ignore_poison().tabs.clone();
        let mut state = state;
        state.tabs = tabs;
        store.set_right(state);
    }
}

/// Tauri command to update focused pane from frontend.
#[tauri::command]
#[specta::specta]
pub fn update_focused_pane(app: AppHandle, pane: String) {
    if let Some(store) = app.try_state::<PaneStateStore>() {
        store.set_focused_pane(pane);
    }
}

/// Tauri command to update tab list for a pane from frontend (for MCP state reporting).
///
/// Delegates to `PaneStateStore::set_tabs`, which bumps the generation counter so the
/// MCP `tab` action tool's ack signal (`GenerationAdvanced`) fires when the FE confirms
/// a tab change. Without that bump the tab tool would time out on every call: tab
/// pushes bypass `set_left`/`set_right`.
#[tauri::command]
#[specta::specta]
pub fn update_pane_tabs(app: AppHandle, pane: String, tabs: Vec<TabInfo>) {
    if let Some(store) = app.try_state::<PaneStateStore>() {
        store.set_tabs(pane.as_str(), tabs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pane_state_store() {
        let store = PaneStateStore::new();

        let state = PaneState {
            path: "/tmp".to_string(),
            volume_id: None,
            volume_name: None,
            files: vec![PaneFileEntry {
                name: "test.txt".to_string(),
                path: "/tmp/test.txt".to_string(),
                is_directory: false,
                size: Some(100),
                recursive_size: None,
                modified: None,
                recursive_size_pending: None,
                tags: vec![],
            }],
            cursor_index: 0,
            view_mode: "brief".to_string(),
            selected_indices: vec![],
            sort_field: "name".to_string(),
            sort_order: "asc".to_string(),
            total_files: 1,
            loaded_start: 0,
            loaded_end: 1,
            show_hidden: false,
            tabs: vec![],
            type_to_jump: None,
        };

        store.set_left(state.clone());
        let retrieved = store.get_left();

        assert_eq!(retrieved.path, "/tmp");
        assert_eq!(retrieved.files.len(), 1);
        assert_eq!(retrieved.files[0].name, "test.txt");
    }

    #[test]
    fn test_focused_pane() {
        let store = PaneStateStore::new();

        assert_eq!(store.get_focused_pane(), "left");

        store.set_focused_pane("right".to_string());
        assert_eq!(store.get_focused_pane(), "right");
    }
}
