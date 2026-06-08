//! Window-management event payloads (Partition 7 of the typed-events migration).
//!
//! These events target a specific window via `Event::emit_to(handle, target)`
//! rather than broadcasting via `Event::emit`: the MCP `dialog` tool drives the
//! main window's dialog lifecycle, the native menu pushes per-window actions,
//! and the viewer's restricted settings round-trip back through the main window.
//!
//! The structs live here (always compiled) because `collect_events!` in `ipc.rs`
//! can't `#[cfg]`-gate inline and the emit sites are spread across `mcp/`,
//! `menu/`, and `commands/`. Each emit site just builds the struct and calls
//! `.emit_to(app, target)`. Same always-compiled-module pattern the MTP /
//! network / system_events partitions use.
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

/// `focus-settings`: bring the settings window forward (MCP `dialog focus`).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
pub struct FocusSettings;

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

/// `tab-context-action`: a tab right-click context-menu item was clicked. The
/// `action` is the raw menu item id (`TAB_PIN_ID` / `TAB_CLOSE_OTHERS_ID` /
/// `TAB_CLOSE_ID`); the FE maps it. Emitted to the main window.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct TabContextAction {
    pub action: String,
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
