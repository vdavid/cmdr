//! Window-management event payloads.
//!
//! These events target a specific window via `Event::emit_to(handle, target)`
//! rather than broadcasting via `Event::emit`: the MCP `dialog` tool drives the
//! main window's dialog lifecycle, the native menu pushes per-window actions,
//! and the viewer's restricted settings round-trip back through the main window.
//!
//! The structs live here (always compiled) because `collect_events!` in `ipc.rs`
//! can't `#[cfg]`-gate inline and the emit sites are spread across `mcp/`,
//! `menu/`, and `commands/`. Each emit site just builds the struct and calls
//! `.emit_to(app, target)`. Same always-compiled-module pattern the MTP,
//! network, and system-events modules use.
//!
//! Wire-name discipline: every struct's kebab-cased name must equal the existing
//! string event name, or it pins the name via `#[tauri_specta(event_name = "…")]`.
//! Switching from a raw string emit to a typed `Event` must not change the wire
//! name (the listening windows already have the matching capability permission;
//! see `capabilities/{default,settings,viewer}.json`).

use serde::{Deserialize, Serialize};
use tauri_specta::Event;

/// `execute-command`: the single unified menu/cross-window command relay. The
/// native menu (`menu/menu_handlers.rs`), the MCP dialog/app tools
/// (`mcp/executor/`), and the settings window's License section all emit this to
/// the main window, which narrows `command_id` to a registry `CommandId` and
/// dispatches it. Wire key stays `commandId` via `rename_all`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteCommand {
    pub command_id: String,
}

/// `show-search-result-in-folder`: a snapshot context-menu click. The primary
/// right-clicked path travels with the event so the frontend never rereads a
/// potentially different cursor or selection after the native popup closes.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ShowSearchResultInFolder {
    pub path: String,
}

/// `open-settings`: open the settings window deep-linked to `section` (MCP
/// `dialog open settings --section …`). Emitted to the main window.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct OpenSettings {
    pub section: String,
}

/// `open-file-viewer`: open a viewer window. `path` present → open that file;
/// absent → open the file under the cursor (MCP `dialog open file-viewer`).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct OpenFileViewer {
    pub path: Option<String>,
}

/// `focus-file-viewer`: focus a viewer. `path` present → that file's viewer;
/// absent → the most recently opened viewer (MCP `dialog focus`).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct FocusFileViewer {
    pub path: Option<String>,
}

/// `focus-about`: ensure the (soft, main-window-overlay) about dialog is visible.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
pub struct FocusAbout;

/// `focus-confirmation`: focus the main window so an open confirmation overlay is
/// visible (MCP `dialog focus <confirmation>`).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
pub struct FocusConfirmation;

/// `close-file-viewer`: close one viewer. `path` present → that file's viewer;
/// absent matches the FE's optional-path close path (MCP `dialog close
/// file-viewer`).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct CloseFileViewer {
    pub path: Option<String>,
}

/// `close-all-file-viewers`: close every open viewer (MCP `dialog close
/// file-viewer` with no path).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
pub struct CloseAllFileViewers;

/// `close-about`: dismiss the about dialog overlay (MCP `dialog close about`).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
pub struct CloseAbout;

/// `close-confirmation`: cancel the open confirmation overlay (MCP `dialog close
/// <confirmation>`).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
pub struct CloseConfirmation;

/// `mcp-settings-close`: ask the settings window to close itself. Emitted via a
/// distinct static `emit_to("settings", …)` (NOT through the generic `mcp-*`
/// runtime relay), so it's cleanly typeable. The settings window's `+page.svelte`
/// listens and closes (MCP `dialog close settings`).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
pub struct McpSettingsClose;

/// `viewer-word-wrap-toggled`: the View > Word wrap menu item was clicked while a
/// viewer window had focus. Emitted to that specific viewer's label.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
pub struct ViewerWordWrapToggled;

/// Which item of the viewer menu bar's Edit submenu was picked. A typed variant rather
/// than the menu id as a string: the viewer frontend dispatches on it, and an id is a
/// backend detail nothing across IPC should be matching on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ViewerEditActionKind {
    /// Edit > Copy (⌘C).
    Copy,
    /// Edit > Select all (⌘A).
    SelectAll,
}

/// `viewer-edit-action`: Edit > Copy or Edit > Select all was picked while a viewer window
/// had focus. Emitted to that viewer's label.
///
/// ⚠️ These two are Custom items rather than the Predefined ones their Cut / Paste neighbours
/// still are, because AppKit's `copy:` / `selectAll:` selectors reach the DOM and the viewer's
/// text isn't there: `.file-content` is `user-select: none` (the viewer owns an offset-based
/// selection model of its own), so the only selectable text left in the window is the status
/// bar, and a native Select all would highlight the footer. Cut and Paste stay Predefined so
/// ⌘X / ⌘V keep working in the viewer's search box.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ViewerEditAction {
    pub action: ViewerEditActionKind,
}

/// `viewer-context-menu-action`: Copy or Select all was picked from the viewer's right-click menu
/// over the file text. Emitted to that viewer's label.
///
/// Its own event rather than a `ViewerEditAction`: the bar's pair hands both actions to the
/// search box while it has focus, and a right-click on the text leaves focus where it was, so
/// this pair always acts on the file.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ViewerContextMenuAction {
    pub action: ViewerEditActionKind,
}

/// `function-key-bar-hide-requested`: the function key bar's right-click context
/// menu's "Hide function key bar" item was clicked. No payload: the frontend
/// owns both the setting write and the confirmation toast. Emitted to the main
/// window, the only place the bar renders.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
pub struct FunctionKeyBarHideRequested;

/// `tab-context-action`: a tab right-click context-menu item was clicked. The
/// `action` is the raw menu item id (`TAB_PIN_ID` / `TAB_CLOSE_OTHERS_ID` /
/// `TAB_CLOSE_ID`); the FE maps it. Emitted to the main window.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct TabContextAction {
    pub action: String,
}

/// Which way a mouse gesture walks the pane history. A typed direction rather
/// than a raw button number or swipe delta: the frontend dispatches a command
/// from it, and reading either shape is `mouse_nav.rs`'s job alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum MouseNavDirection {
    Back,
    Forward,
}

/// `mouse-nav`: a back / forward navigation gesture finished over the main
/// window — a mouse's X1/X2 side button, or the swipe a Logi Options+ mouse
/// substitutes for it. macOS only, emitted by the AppKit event monitor in
/// `mouse_nav.rs`; on Linux the frontend reads the buttons straight off the DOM.
/// Emitted to the main window, which dispatches `nav.back` / `nav.forward` on
/// the same bus as `⌘[` / `⌘]`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct MouseNav {
    pub direction: MouseNavDirection,
}

/// `foreground-operation`: the operation queue asks the main window to show one
/// operation in its progress dialog (the row's Foreground button). Carries only
/// the id: the registry snapshot both windows already receive is the single
/// source of truth for everything else about that operation. Emitted by the
/// queue window's frontend and listened for by the main window; Rust never emits
/// it, exactly like `execute-command` from the settings window.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ForegroundOperation {
    pub operation_id: String,
}

/// `reveal-path`: show a folder in the main window's focused pane. Two emitters:
/// the settings window's "Open memory folder" button, which knows only that it
/// wants the folder shown (the path comes from Rust's `ask_cmdr_memory_folder`,
/// because it moves with `CMDR_DATA_DIR`), and the Dock tile menu's tab rows
/// (`dock/menu/`), where the path is the row the user clicked.
///
/// ⚠️ The payload is why this isn't `execute-command`, which carries a bare
/// `command_id` and nothing else.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct RevealPath {
    pub path: String,
}

/// `open-favorite`: open a favorite in the main window's focused pane, exactly as
/// picking it in the favorites menu does. Emitted by the Dock tile menu's bookmark
/// rows (`dock/menu/`).
///
/// ⚠️ By id, never by path: the path of a favorite on an unmounted share resolves
/// onto the boot disk, while the favorite's row names its volume, which then dials.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct OpenFavorite {
    /// The favorite's store id, without the `fav-` prefix its volume row carries.
    pub favorite_id: String,
}

/// `persist-restricted-setting`: the viewer (a restricted-capability window with
/// no store access) forwards an allowlisted setting write to the main window,
/// which persists it through the normal store pipeline. Emitted to the main
/// window from `persist_restricted_window_setting`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct PersistRestrictedSetting {
    pub id: String,
    pub value: bool,
}
