//! Single source of truth for MCP tools.
//!
//! Each tool is authored exactly once in the `mcp_tools!` table below, bundling its name,
//! description, JSON input schema, bearer-token gate, and handler. The macro expands that one
//! table into every consumer, so the facets can't drift:
//!
//! - `get_all_tools()` — the `tools/list` payload (non-generic; server + tests read it).
//! - `execute_tool()` — the `tools/call` dispatch (generic over `Runtime`).
//! - `tool_gate()` + [`TokenGate`] — the auth classification `auth.rs` reads.
//!
//! Adding a tool means adding one entry: you can't add it without supplying a schema, a gate,
//! and a handler, and you can't add a handler the dispatch doesn't know about. The count and
//! coverage tests are then cheap guards over a property that's true by construction.
//!
//! Wire output must stay byte-identical: each schema is the exact `json!` block moved verbatim
//! here, and the tool order is the historical category concatenation. The `tool_snapshot_tests`
//! fixture pins it. Schema keys serialize alphabetically (serde_json `Map` is a `BTreeMap`;
//! `preserve_order` is off), so authored key order never affects the bytes.
//!
//! Layering: this module depends on the `executor` handlers; `auth` depends on this module.
//! It must not depend on `server` or `auth` (that would cycle).

use serde_json::{Value, json};

use super::executor::{ToolError, ToolResult};
use super::executor::{
    app, async_tools, dialogs, downloads, eject, favorites, file_ops, indexing, nav, operation_log, queue, search,
    tags, view,
};
use super::tools::Tool;

/// How a tool relates to the bearer-token gate. Pure, non-generic, and unit-testable — it
/// reproduces the previous `tool_call_requires_token` classification exactly, and `auth.rs`
/// reads it via [`tool_gate`]. See `mcp/DETAILS.md` § Authentication for the threat model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenGate {
    /// No token needed: reads, nav, search, and destructive ops that still prompt the user.
    Open,
    /// Always gated: config mutation that applies with no user confirmation (`set_setting`).
    Always,
    /// Gated iff `arguments.autoConfirm == true`: `copy` / `move` / `delete`.
    IfAutoConfirm,
    /// Gated iff `arguments.action == "confirm"`: the `dialog` tool.
    IfConfirmAction,
    /// Gated iff `arguments.rollback == true`: the `queue` tool's cancel action.
    /// Plain pause/resume/cancel are transient runtime actions (Open), but a
    /// rollback cancel actively DELETES already-copied files with no confirmation
    /// dialog — the same "auto-confirm a destructive thing" shape the token guards.
    IfRollback,
}

impl TokenGate {
    /// Whether a call with these `arguments` (the JSON-RPC `params.arguments` object) requires
    /// the bearer token. `IfConfirmAction` reads the tool's own typed `action` enum, not a
    /// message substring, so it's not a `no-string-matching` violation.
    pub fn requires_token(self, arguments: Option<&Value>) -> bool {
        match self {
            TokenGate::Open => false,
            TokenGate::Always => true,
            TokenGate::IfAutoConfirm => arguments
                .and_then(|a| a.get("autoConfirm"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            TokenGate::IfConfirmAction => {
                arguments.and_then(|a| a.get("action")).and_then(|v| v.as_str()) == Some("confirm")
            }
            TokenGate::IfRollback => arguments
                .and_then(|a| a.get("rollback"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        }
    }
}

/// Declarative tool table → the three consumers (`get_all_tools`, `execute_tool`, `tool_gate`).
///
/// Entry form: `"name" => { desc, schema, gate, run: <shape> <handler-path> }`.
///
/// The `run` shape tag selects how the generated dispatch calls the handler, sidestepping
/// `macro_rules!` hygiene (call-site idents from the table can't bind to the def-site fn params,
/// so the shape helper passes the macro's own `app`/`params`/`name` positionally):
///
/// - `app_params` — `handler(app, params).await` (async; most tools).
/// - `app_only` — `handler(app).await` (async; no params: `toggle_hidden`, `mkdir`, …).
/// - `params_only` — `handler(params).await` (async; no `app`: `search`, `ai_search`).
/// - `sync_app` — `handler(app)` (sync; `quit`, `switch_pane`, `swap_panes`).
/// - `sync_app_params` — `handler(app, params)` (sync; `remove_manual_server`).
/// - `nav` / `nav_params` — `handler(app, name)` / `handler(app, name, params)` for the nav
///   family, which routes several tools through one handler by passing the tool name as a literal.
///
/// Sync arms deliberately don't `.await` (the handlers are sync), matching the hand-written
/// dispatch this replaced.
macro_rules! mcp_tools {
    ( $( $name:literal => { desc: $desc:expr, schema: $schema:expr, gate: $gate:expr, run: $shape:tt $path:path } ),* $(,)? ) => {
        /// The `tools/list` payload: every tool in wire order.
        pub fn get_all_tools() -> Vec<Tool> {
            vec![ $( Tool { name: $name.into(), description: $desc.into(), input_schema: $schema } ),* ]
        }

        /// The bearer-token classification for a tool, or `None` for an unknown name. The
        /// single source `auth::tool_call_requires_token` reads.
        pub fn tool_gate(name: &str) -> Option<TokenGate> {
            match name {
                $( $name => Some($gate), )*
                _ => None,
            }
        }

        /// The `tools/call` dispatch. An unknown name is the same `INVALID_PARAMS`
        /// "Unknown tool" error the hand-written match returned. Generic over `Runtime`.
        pub async fn execute_tool<R: tauri::Runtime>(
            app: &tauri::AppHandle<R>,
            name: &str,
            params: &Value,
        ) -> ToolResult {
            match name {
                $( $name => mcp_tools!(@call $shape $path, $name, app, params), )*
                _ => Err(ToolError::invalid_params(format!("Unknown tool: {name}"))),
            }
        }
    };

    // Handler-shape arms: each evaluates to a `ToolResult`. Sync handlers deliberately
    // don't `.await`. `app`/`params`/`name` are passed positionally from the generated
    // dispatch (the macro's own body context) so `macro_rules!` hygiene never bites.
    (@call app_params      $p:path, $name:literal, $app:ident, $params:ident) => { $p($app, $params).await };
    (@call app_only        $p:path, $name:literal, $app:ident, $params:ident) => { $p($app).await };
    (@call params_only     $p:path, $name:literal, $app:ident, $params:ident) => { $p($params).await };
    (@call sync_app        $p:path, $name:literal, $app:ident, $params:ident) => { $p($app) };
    (@call sync_app_params $p:path, $name:literal, $app:ident, $params:ident) => { $p($app, $params) };
    (@call nav             $p:path, $name:literal, $app:ident, $params:ident) => { $p($app, $name).await };
    (@call nav_params      $p:path, $name:literal, $app:ident, $params:ident) => { $p($app, $name, $params).await };
}

/// The empty input schema shared by every no-parameter tool.
fn no_params_schema() -> Value {
    json!({
        "type": "object",
        "properties": {},
        "required": []
    })
}

mcp_tools! {
    // ── Navigation ──────────────────────────────────────────────────────────
    "select_volume" => {
        desc: "Switch a pane to a volume by name (as listed in cmdr://state volumes): a disk, SMB share, MTP device, or Network. To move within the current volume, use nav_to_path instead.",
        schema: json!({
            "type": "object",
            "properties": {
                "pane": {
                    "type": "string",
                    "enum": ["left", "right"],
                    "description": "Which pane to switch"
                },
                "name": {
                    "type": "string",
                    "description": "Volume name to select"
                }
            },
            "required": ["pane", "name"]
        }),
        gate: TokenGate::Open,
        run: nav_params nav::execute_nav_command_with_params
    },
    "nav_to_path" => {
        desc: "Navigate a pane to a path: absolute, ~-relative, or virtual (mtp://, smb://). Prefer this over stepping with nav_to_parent when you know the target. Archive paths are transparent, so a path through foo.zip/inner navigates inside the archive.",
        schema: json!({
            "type": "object",
            "properties": {
                "pane": {
                    "type": "string",
                    "enum": ["left", "right"],
                    "description": "Which pane to navigate"
                },
                "path": {
                    "type": "string",
                    "description": "Path to navigate to: absolute, ~-relative, or virtual (mtp://, smb://)"
                }
            },
            "required": ["pane", "path"]
        }),
        gate: TokenGate::Open,
        run: nav_params nav::execute_nav_command_with_params
    },
    "nav_to_parent" => {
        desc: "Navigate the focused pane up to its parent folder.",
        schema: no_params_schema(),
        gate: TokenGate::Open,
        run: nav nav::execute_nav_command
    },
    "nav_back" => {
        desc: "Go back to the focused pane's previous folder in its navigation history.",
        schema: no_params_schema(),
        gate: TokenGate::Open,
        run: nav nav::execute_nav_command
    },
    "nav_forward" => {
        desc: "Go forward again (undo a nav_back) in the focused pane's navigation history.",
        schema: no_params_schema(),
        gate: TokenGate::Open,
        run: nav nav::execute_nav_command
    },
    "scroll_to" => {
        desc: "Load the file window around an index in a large (paginated) directory so those rows appear in cmdr://state. Needed before move_cursor / select can reach a row outside the currently loaded range.",
        schema: json!({
            "type": "object",
            "properties": {
                "pane": {
                    "type": "string",
                    "enum": ["left", "right"],
                    "description": "Which pane to scroll"
                },
                "index": {
                    "type": "integer",
                    "description": "Zero-based index to scroll to"
                }
            },
            "required": ["pane", "index"]
        }),
        gate: TokenGate::Open,
        run: nav_params nav::execute_nav_command_with_params
    },

    // ── Cursor ──────────────────────────────────────────────────────────────
    "move_cursor" => {
        desc: "Focus a pane and move its cursor to a row, by zero-based index or by filename (give one). Flushes pane state, so a following copy / move / delete / rename acts on this row. A missing filename or out-of-range index is an honest error, never a silent no-op.",
        schema: json!({
            "type": "object",
            "properties": {
                "pane": {
                    "type": "string",
                    "enum": ["left", "right"],
                    "description": "Which pane to move cursor in"
                },
                "index": {
                    "type": "integer",
                    "description": "Zero-based index to move cursor to"
                },
                "filename": {
                    "type": "string",
                    "description": "Filename to move cursor to"
                }
            },
            "required": ["pane"]
        }),
        gate: TokenGate::Open,
        run: nav_params nav::execute_nav_command_with_params
    },
    "open_under_cursor" => {
        desc: "Open the item under the cursor, like pressing Enter: enter a folder, open a file, or connect a network host / share.",
        schema: no_params_schema(),
        gate: TokenGate::Open,
        run: nav nav::execute_nav_command
    },

    // ── Selection ───────────────────────────────────────────────────────────
    "select" => {
        desc: "Select files in a pane by names, by an index range (start + count), or all; count=0 clears. Focuses the pane and flushes state, so a following copy / move / delete / compress acts on this selection. names errors if any name isn't in the listing.",
        schema: json!({
            "type": "object",
            "properties": {
                "pane": {
                    "type": "string",
                    "enum": ["left", "right"],
                    "description": "Which pane to select in"
                },
                "names": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Filenames to select. Errors if any name isn't in the listing."
                },
                "start": {
                    "type": "integer",
                    "description": "Zero-based start index"
                },
                "count": {
                    "type": "integer",
                    "description": "Number of items from start. 0 clears selection"
                },
                "all": {
                    "type": "boolean",
                    "description": "Select all files"
                },
                "mode": {
                    "type": "string",
                    "enum": ["replace", "add", "subtract"],
                    "description": "Selection mode (default: replace)"
                }
            },
            "required": ["pane"]
        }),
        gate: TokenGate::Open,
        run: app_params file_ops::execute_select_command
    },

    // ── File operations ─────────────────────────────────────────────────────
    "copy" => {
        desc: "Copy the selection (else the cursor item) to the other pane. Without autoConfirm, opens the confirm dialog. With autoConfirm, starts at once and returns the operationId (await operation_complete, or steer with queue). onConflict resolves file clashes.",
        schema: json!({
            "type": "object",
            "properties": {
                "autoConfirm": {
                    "type": "boolean",
                    "description": "When true, dialog opens and immediately confirms without waiting for user interaction. Returns once the operation starts."
                },
                "onConflict": {
                    "type": "string",
                    "enum": ["skip_all", "overwrite_all", "rename_all"],
                    "description": "Conflict resolution policy for clashing FILES (only when autoConfirm is true). Folders always merge: a source folder landing on a same-named dest folder merges into it, and this policy governs the files inside. Default: skip_all"
                }
            },
            "required": []
        }),
        gate: TokenGate::IfAutoConfirm,
        run: app_params file_ops::execute_copy
    },
    "move" => {
        desc: "Move the selection (else the cursor item) to the other pane. Without autoConfirm, opens the confirm dialog. With autoConfirm, starts at once and returns the operationId (await operation_complete, or steer with queue). onConflict resolves file clashes.",
        schema: json!({
            "type": "object",
            "properties": {
                "autoConfirm": {
                    "type": "boolean",
                    "description": "When true, dialog opens and immediately confirms without waiting for user interaction. Returns once the operation starts."
                },
                "onConflict": {
                    "type": "string",
                    "enum": ["skip_all", "overwrite_all", "rename_all"],
                    "description": "Conflict resolution policy for clashing FILES (only when autoConfirm is true). Folders always merge: a source folder landing on a same-named dest folder merges into it, and this policy governs the files inside. Default: skip_all"
                }
            },
            "required": []
        }),
        gate: TokenGate::IfAutoConfirm,
        run: app_params file_ops::execute_move
    },
    "compress" => {
        desc: "Zip the selection into a new archive in the other pane. Without autoConfirm, opens the confirm dialog. With autoConfirm, starts and returns the operationId — unless the target archive exists, where the dialog stays open to confirm the overwrite.",
        schema: json!({
            "type": "object",
            "properties": {
                "autoConfirm": {
                    "type": "boolean",
                    "description": "When true, the dialog opens and immediately confirms without waiting for user interaction, returning once the compress starts. Exception: if the target archive already exists, the dialog stays open for the user to confirm the overwrite rather than replacing it silently."
                }
            },
            "required": []
        }),
        gate: TokenGate::IfAutoConfirm,
        run: app_params file_ops::execute_compress
    },
    "delete" => {
        desc: "Delete the selection (else the cursor item). Without autoConfirm, opens the confirm dialog. With autoConfirm, starts at once and returns the operationId (await operation_complete on it). mode presets trash vs permanent; omit for the pane's default.",
        schema: json!({
            "type": "object",
            "properties": {
                "autoConfirm": {
                    "type": "boolean",
                    "description": "When true, dialog opens and immediately confirms without waiting for user interaction. Returns once the operation starts."
                },
                "mode": {
                    "type": "string",
                    "enum": ["trash", "delete"],
                    "description": "trash = move to Trash; delete = permanent. Omit for the pane's default (a volume without a trash forces permanent)."
                }
            },
            "required": []
        }),
        gate: TokenGate::IfAutoConfirm,
        run: app_params file_ops::execute_delete
    },
    "rename" => {
        desc: "Rename an item (the named item, else the cursor item) in a pane. Without autoConfirm, \
               opens the inline rename editor prefilled with newName for the user to confirm. With \
               autoConfirm, renames directly (errors if the name already exists).",
        schema: json!({
            "type": "object",
            "properties": {
                "pane": {
                    "type": "string",
                    "enum": ["left", "right"],
                    "description": "Which pane to rename in. Defaults to the focused pane."
                },
                "name": {
                    "type": "string",
                    "description": "Current filename to rename. Defaults to the cursor item. Errors if it isn't in the listing."
                },
                "newName": {
                    "type": "string",
                    "description": "The proposed new name (a name, not a path)."
                },
                "autoConfirm": {
                    "type": "boolean",
                    "description": "When true, renames directly without the review editor. Returns once the rename lands."
                }
            },
            "required": ["newName"]
        }),
        gate: TokenGate::IfAutoConfirm,
        run: app_params file_ops::execute_rename
    },
    "mkdir" => {
        desc: "Create a folder in the focused pane, or pass pane to target the other. No name opens the naming \
               dialog (user confirms, not MCP); a name prefills it; name + autoConfirm creates directly (errors on \
               a name conflict).",
        schema: json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Folder name. Omit to open the dialog with the default prefill."
                },
                "autoConfirm": {
                    "type": "boolean",
                    "description": "With a name, create directly without the dialog. Returns once created."
                },
                "pane": {
                    "type": "string",
                    "enum": ["left", "right"],
                    "description": "Target pane. Defaults to the focused pane."
                }
            },
            "required": []
        }),
        gate: TokenGate::IfAutoConfirm,
        run: app_params file_ops::execute_mkdir
    },
    "mkfile" => {
        desc: "Create an empty file in the focused pane, or pass pane to target the other. No name opens the naming \
               dialog (user confirms, not MCP); a name prefills it; name + autoConfirm creates directly (errors on \
               a name conflict).",
        schema: json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "File name. Omit to open the dialog with the default prefill."
                },
                "autoConfirm": {
                    "type": "boolean",
                    "description": "With a name, create directly without the dialog. Returns once created."
                },
                "pane": {
                    "type": "string",
                    "enum": ["left", "right"],
                    "description": "Target pane. Defaults to the focused pane."
                }
            },
            "required": []
        }),
        gate: TokenGate::IfAutoConfirm,
        run: app_params file_ops::execute_mkfile
    },
    "refresh" => {
        desc: "Force a re-read of the focused pane's listing (from disk on local volumes; the watcher cache short-circuits on MTP / SMB). Use after an out-of-band change; navigation and file ops already refresh on their own.",
        schema: no_params_schema(),
        gate: TokenGate::Open,
        run: app_only file_ops::execute_refresh
    },
    "tag" => {
        desc: "Set macOS Finder color tags on files by name (else selection, else cursor). set: \
               make the colors exactly (keeps colorless tags). toggle: flip each color. clear: \
               remove all. macOS only; tags show in cmdr://state as [tags:red,blue].",
        schema: json!({
            "type": "object",
            "properties": {
                "pane": {
                    "type": "string",
                    "enum": ["left", "right"],
                    "description": "Which pane to act in. Defaults to the focused pane."
                },
                "action": {
                    "type": "string",
                    "enum": ["set", "toggle", "clear"],
                    "description": "set | toggle | clear"
                },
                "names": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Filenames to tag. Defaults to the selection, else the cursor item. Errors if a name isn't in the listing."
                },
                "colors": {
                    "type": "array",
                    "items": {
                        "type": "string",
                        "enum": ["red", "orange", "yellow", "green", "blue", "purple", "gray"]
                    },
                    "description": "Finder color names. Required for set and toggle; ignored for clear."
                }
            },
            "required": ["action"]
        }),
        gate: TokenGate::Always,
        run: app_params tags::execute_tag
    },

    // ── View ────────────────────────────────────────────────────────────────
    "toggle_hidden" => {
        desc: "Toggle whether hidden (dotfile) files show in the file lists (the showHidden flag in cmdr://state).",
        schema: no_params_schema(),
        gate: TokenGate::Open,
        run: app_only view::execute_toggle_hidden
    },
    "set_view_mode" => {
        desc: "Set a pane's view mode: brief (names, only the cursor row detailed) or full (size and date on every row). full makes cmdr://state carry those details for all rows, not just the cursor.",
        schema: json!({
            "type": "object",
            "properties": {
                "pane": {
                    "type": "string",
                    "enum": ["left", "right"],
                    "description": "Which pane to set view mode for"
                },
                "mode": {
                    "type": "string",
                    "enum": ["brief", "full"],
                    "description": "View mode to set"
                }
            },
            "required": ["pane", "mode"]
        }),
        gate: TokenGate::Open,
        run: app_params view::execute_set_view_mode
    },
    "sort" => {
        desc: "Sort a pane by a field (name, ext, size, modified, created) and order (asc / desc).",
        schema: json!({
            "type": "object",
            "properties": {
                "pane": {
                    "type": "string",
                    "enum": ["left", "right"],
                    "description": "Which pane to sort"
                },
                "by": {
                    "type": "string",
                    "enum": ["name", "ext", "size", "modified", "created"],
                    "description": "Field to sort by"
                },
                "order": {
                    "type": "string",
                    "enum": ["asc", "desc"],
                    "description": "Sort order"
                }
            },
            "required": ["pane", "by", "order"]
        }),
        gate: TokenGate::Open,
        run: app_params view::execute_sort
    },

    // ── Tabs ────────────────────────────────────────────────────────────────
    "tab" => {
        desc: "Manage a pane's tabs: new, close, close_others, activate, set_pinned, or reopen (restore the last-closed tab). tabId defaults to the active tab where it applies; see each pane's tabs in cmdr://state.",
        schema: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["new", "close", "close_others", "activate", "set_pinned", "reopen"],
                    "description": "Action to perform on the tab"
                },
                "pane": {
                    "type": "string",
                    "enum": ["left", "right"],
                    "description": "Which pane to operate on"
                },
                "tabId": {
                    "type": "string",
                    "description": "Tab ID. Defaults to active tab for close, close_others, set_pinned. Required for activate. Not used for new or reopen."
                },
                "pinned": {
                    "type": "boolean",
                    "description": "Pin state (only for set_pinned action)"
                }
            },
            "required": ["action", "pane"]
        }),
        gate: TokenGate::Open,
        run: app_params app::execute_tab
    },

    // ── Dialogs ─────────────────────────────────────────────────────────────
    "dialog" => {
        desc: "Open, focus, close, or confirm a dialog. Open/focus: settings, file-viewer, about, onboarding. Close: any id from cmdr://dialogs/available. confirm (token-gated) accepts an open confirmation. cmdr://state lists what's open.",
        schema: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["open", "focus", "close", "confirm"],
                    "description": "Action to perform. 'confirm' triggers the confirm button on an already-open dialog."
                },
                "type": {
                    "type": "string",
                    "description": "Dialog type. Openable/focusable: settings, file-viewer, about, onboarding. Closable: any dialog id from cmdr://dialogs/available (also settings, file-viewer). Confirmable: transfer-confirmation (covers copy and move; 'copy-confirmation' is an alias) and delete-confirmation."
                },
                "section": {
                    "type": "string",
                    "description": "For settings: which section to open (e.g., 'shortcuts')"
                },
                "path": {
                    "type": "string",
                    "description": "For file-viewer: file path. On open without path, uses cursor file. On close without path, closes all."
                },
                "onConflict": {
                    "type": "string",
                    "enum": ["skip_all", "overwrite_all", "rename_all"],
                    "description": "For confirm action on transfer-confirmation: conflict resolution policy for clashing FILES. Folders always merge (a source folder landing on a same-named dest folder merges into it), and this policy governs the files inside. Default: skip_all"
                }
            },
            "required": ["action", "type"]
        }),
        gate: TokenGate::IfConfirmAction,
        run: app_params dialogs::execute_dialog_command
    },
    "open_search_dialog" => {
        desc: "Open the search dialog with optional pre-filled query and filters. If autoRun (default true), runs the search immediately. Acks once the dialog has mounted; does not wait for results to render.",
        schema: json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query to pre-fill in the search bar"
                },
                "mode": {
                    "type": "string",
                    "enum": ["ai", "filename", "regex"],
                    "description": "Search mode. Defaults to 'ai' if AI is enabled, otherwise 'filename'."
                },
                "sizeMin": {
                    "type": "integer",
                    "description": "Minimum file size in bytes"
                },
                "sizeMax": {
                    "type": "integer",
                    "description": "Maximum file size in bytes"
                },
                "modifiedAfter": {
                    "type": "string",
                    "description": "ISO date string (for example, '2025-01-01')"
                },
                "modifiedBefore": {
                    "type": "string",
                    "description": "ISO date string"
                },
                "isDirectory": {
                    "type": "boolean",
                    "description": "Type filter: true = folders only, false = files only, omit for both"
                },
                "scope": {
                    "type": "string",
                    "description": "Scope string, same syntax as the scope chip: comma-separated paths, ! prefix for excludes"
                },
                "caseSensitive": {
                    "type": "boolean",
                    "description": "Case-sensitive matching"
                },
                "excludeSystemDirs": {
                    "type": "boolean",
                    "description": "Exclude system/build/cache folders (node_modules, .git, Caches, etc.)"
                },
                "autoRun": {
                    "type": "boolean",
                    "description": "Default true: open and run the search. False: open and prefill without running."
                }
            },
            "required": []
        }),
        gate: TokenGate::Open,
        run: app_params dialogs::execute_open_search_dialog
    },

    // ── App ─────────────────────────────────────────────────────────────────
    "quit" => {
        desc: "Quit the application",
        schema: no_params_schema(),
        gate: TokenGate::Open,
        run: sync_app app::execute_quit
    },
    "switch_pane" => {
        desc: "Toggle focus to the other pane. Takes no parameters (a pane arg is ignored). To focus a SPECIFIC pane, use select (with count 0 to clear) or select_volume / nav_to_path on that pane, which focus it.",
        schema: no_params_schema(),
        gate: TokenGate::Open,
        run: sync_app app::execute_switch_pane
    },
    "swap_panes" => {
        desc: "Swap left and right pane directories, view modes, sort orders, and selections",
        schema: no_params_schema(),
        gate: TokenGate::Open,
        run: sync_app app::execute_swap_panes
    },

    // ── Search ──────────────────────────────────────────────────────────────
    "search" => {
        desc: "Search the drive index by filename pattern, size, date, or type; returns paths (no UI). Prefer over ai_search when the query is a plain pattern or filter (no LLM call), and over open_search_dialog for a programmatic lookup. Needs an indexed volume.",
        schema: json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob or regex filename pattern (for example, \"*.pdf\", \"report*\")"
                },
                "patternType": {
                    "type": "string",
                    "enum": ["glob", "regex"],
                    "description": "Pattern type. Default: glob"
                },
                "sizeMin": {
                    "type": "string",
                    "description": "Minimum file size, human-readable (for example, \"1 MB\", \"500 KB\")"
                },
                "sizeMax": {
                    "type": "string",
                    "description": "Maximum file size, human-readable"
                },
                "modifiedAfter": {
                    "type": "string",
                    "description": "ISO date, for example \"2025-01-01\""
                },
                "modifiedBefore": {
                    "type": "string",
                    "description": "ISO date"
                },
                "type": {
                    "type": "string",
                    "enum": ["file", "dir"],
                    "description": "Filter by type. Omit for both."
                },
                "scope": {
                    "type": "string",
                    "description": "Scope string: comma-separated paths, ! for excludes (for example, \"~/projects, !node_modules\")"
                },
                "caseSensitive": {
                    "type": "boolean",
                    "description": "Case-sensitive matching. Default: false on macOS, true on Linux"
                },
                "excludeSystemDirs": {
                    "type": "boolean",
                    "description": "Exclude system/build/cache folders (node_modules, .git, Caches, etc). Default: true"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max results to return. Default: 30"
                }
            },
            "required": []
        }),
        gate: TokenGate::Open,
        run: params_only search::execute_search
    },
    "ai_search" => {
        desc: "Search with a natural-language query; the configured LLM turns it into a structured search over the drive index and returns matching paths. Use search instead when you can express the query as a pattern or filter (it skips the LLM call).",
        schema: json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Natural language search query (for example, \"recent invoices marked rymd\")"
                },
                "scope": {
                    "type": "string",
                    "description": "Scope string: comma-separated paths, ! for excludes (for example, \"~/projects, !node_modules\"). Merged with AI-inferred scope."
                },
                "limit": {
                    "type": "integer",
                    "description": "Max results to return. Default: 30"
                }
            },
            "required": ["query"]
        }),
        gate: TokenGate::Open,
        run: params_only search::execute_ai_search
    },

    // ── Settings ────────────────────────────────────────────────────────────
    "set_setting" => {
        desc: "Set a setting value. Use the cmdr://settings resource to discover available settings and their constraints.",
        schema: json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Setting ID, for example 'appearance.appColor'"
                },
                "value": {
                    "description": "New value for the setting"
                }
            },
            "required": ["id", "value"]
        }),
        gate: TokenGate::Always,
        run: app_params async_tools::execute_set_setting
    },

    // ── Indexing ────────────────────────────────────────────────────────────
    "indexing" => {
        desc: "Control one volume's drive indexing. Actions: enable (on, starts first scan), \
               disable (off, keeps DB), rescan (fresh full scan), forget (delete DB). enable/rescan \
               return once scanning starts; poll await index_status fresh for done. See cmdr://indexing.",
        schema: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["enable", "disable", "rescan", "forget"],
                    "description": "enable | disable | rescan | forget"
                },
                "volumeId": {
                    "type": "string",
                    "description": "Volume ID to control (for example 'root', 'smb-…', 'mtp-…:1'). See cmdr://state volumes."
                }
            },
            "required": ["action", "volumeId"]
        }),
        gate: TokenGate::Always,
        run: params_only indexing::execute_indexing
    },

    // ── Queue ───────────────────────────────────────────────────────────────
    "queue" => {
        desc: "Control the operation queue: pause / resume / cancel one operationId, or \
               pause_all / resume_all. cancel also takes operationIds (array) for several; \
               rollback: true deletes already-copied files (single op, token-gated). See \
               cmdr://state operations for ids.",
        schema: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["pause", "resume", "cancel", "pause_all", "resume_all"],
                    "description": "pause | resume | cancel | pause_all | resume_all"
                },
                "operationId": {
                    "type": "string",
                    "description": "Operation to act on (required for pause / resume / cancel unless operationIds is given). See cmdr://state operations."
                },
                "operationIds": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "For cancel only: several operations to cancel at once (keeps already-copied files)."
                },
                "rollback": {
                    "type": "boolean",
                    "description": "For cancel with a single operationId: delete already-copied files instead of keeping them. Requires the bearer token."
                }
            },
            "required": ["action"]
        }),
        gate: TokenGate::IfRollback,
        run: params_only queue::execute_queue
    },

    // ── Favorites ───────────────────────────────────────────────────────────
    "favorites" => {
        desc: "Manage the user's favorites (the switcher's Favorites section). add: path (+ \
               optional name). rename: id + name. remove: id. reorder: orderedIds, the COMPLETE \
               new ordering. Discover ids in cmdr://state favorites.",
        schema: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["add", "rename", "remove", "reorder"],
                    "description": "add | rename | remove | reorder"
                },
                "path": {
                    "type": "string",
                    "description": "For add: the folder path to favorite (~ expands to home)."
                },
                "id": {
                    "type": "string",
                    "description": "For rename / remove: the favorite id. See cmdr://state favorites."
                },
                "name": {
                    "type": "string",
                    "description": "For add (optional, defaults to the path's name) / rename (required): the display label."
                },
                "orderedIds": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "For reorder: the complete new ordering of favorite ids."
                }
            },
            "required": ["action"]
        }),
        gate: TokenGate::Always,
        run: params_only favorites::execute_favorites
    },

    // ── Network ─────────────────────────────────────────────────────────────
    "connect_to_server" => {
        desc: "Add a manual SMB server by address. Checks TCP reachability then adds to the host list.",
        schema: json!({
            "type": "object",
            "properties": {
                "address": {
                    "type": "string",
                    "description": "Server address: hostname, IP, IP:port, or smb:// URL"
                }
            },
            "required": ["address"]
        }),
        gate: TokenGate::Open,
        run: app_params async_tools::execute_connect_to_server
    },
    "remove_manual_server" => {
        desc: "Remove a manually-added server from the host list.",
        schema: json!({
            "type": "object",
            "properties": {
                "hostId": {
                    "type": "string",
                    "description": "Host ID to remove (for example, manual-192-168-1-100-9445)"
                }
            },
            "required": ["hostId"]
        }),
        gate: TokenGate::Open,
        run: sync_app_params async_tools::execute_remove_manual_server
    },
    "upgrade_smb_to_direct" => {
        desc: "Upgrade an OS-mounted SMB volume to a direct smb2 session for faster I/O. Uses \
               Keychain creds. Returns OK, NeedsCredentials, or NetworkError. See \
               cmdr://state volumes for each SMB share's smbConnectionState.",
        schema: json!({
            "type": "object",
            "properties": {
                "volumeId": {
                    "type": "string",
                    "description": "Volume ID of the SMB share (e.g. smb-192-168-1-111-445-naspi). See cmdr://state volumes."
                }
            },
            "required": ["volumeId"]
        }),
        gate: TokenGate::Open,
        run: app_params async_tools::execute_upgrade_smb_to_direct
    },
    "eject" => {
        desc: "Eject an ejectable volume by id (disk or MTP). Refuses honestly while an operation \
               is reading from or writing to the volume, and for non-ejectable volumes. See \
               cmdr://state volumes for ids.",
        schema: json!({
            "type": "object",
            "properties": {
                "volumeId": {
                    "type": "string",
                    "description": "Volume ID to eject (for example 'smb-…' or 'mtp-…:1'). See cmdr://state volumes."
                }
            },
            "required": ["volumeId"]
        }),
        gate: TokenGate::Open,
        run: params_only eject::execute_eject
    },

    // ── Async ───────────────────────────────────────────────────────────────
    "await" => {
        desc: "Wait until a condition is met, after fire-and-forget actions or async events. Pane conditions watch a pane; index_status watches a volume's indexing freshness; operation_complete / operations_idle watch the write-operation queue.",
        schema: json!({
            "type": "object",
            "properties": {
                "pane": {
                    "type": "string",
                    "enum": ["left", "right"],
                    "description": "Which pane to watch. Required for the pane conditions; ignored for index_status / operation_complete / operations_idle."
                },
                "condition": {
                    "type": "string",
                    "enum": ["has_item", "not_has_item", "item_count_gte", "item_count_lte", "path", "path_contains", "index_status", "operation_complete", "operations_idle"],
                    "description": "Condition to wait for: has_item / not_has_item (file list contains / no longer contains item named value — use not_has_item after a delete), item_count_gte / item_count_lte (file list has >= / <= value items), path (pane path equals value), path_contains (pane path contains value), index_status (volumeId's indexing freshness equals value: fresh / scanning / stale), operation_complete (the operation whose id is value settled — completed / cancelled / failed, reported in the result), operations_idle (no operation is running or queued; takes no value)"
                },
                "volumeId": {
                    "type": "string",
                    "description": "For index_status: the volume whose indexing freshness to watch (for example 'root', 'smb-…', 'mtp-…:1')."
                },
                "value": {
                    "type": "string",
                    "description": "Value for the condition (item name, count, path, substring, an index_status status fresh / scanning / stale, or for operation_complete the operationId). Not used by operations_idle."
                },
                "timeoutSeconds": {
                    "type": "integer",
                    "description": "Timeout in seconds (default 15, max 60)"
                },
                "afterGeneration": {
                    "type": "integer",
                    "description": "Only consider state updates after this generation number. Prevents matching stale state from before an action. Get the current generation from cmdr://state or a previous await result. Pane conditions only."
                }
            },
            "required": ["condition"]
        }),
        gate: TokenGate::Open,
        run: app_params async_tools::execute_await
    },

    // ── Downloads ───────────────────────────────────────────────────────────
    "go_to_latest_download" => {
        desc: "Navigate the focused pane to the most recently observed eligible file in ~/Downloads and select it. Errors if no eligible file exists or Cmdr lacks Full Disk Access.",
        schema: no_params_schema(),
        gate: TokenGate::Open,
        run: app_only downloads::execute_go_to_latest_download
    },

    // ── Operation log ─────────────────────────────────────────────────────────
    "operations_list" => {
        desc: "List past operations from the durable operation log (copy, move, delete, trash, rename, create, compress), newest first. Filter by time, item name, kind, initiator, status; paged. In-flight ops live in cmdr://state operations + the queue tool.",
        schema: json!({
            "type": "object",
            "properties": {
                "since": {
                    "type": "integer",
                    "description": "Inclusive lower bound on the operation's start time (Unix milliseconds)"
                },
                "until": {
                    "type": "integer",
                    "description": "Inclusive upper bound on the operation's start time (Unix milliseconds)"
                },
                "name": {
                    "type": "string",
                    "description": "Match operations that touched an item with this name (folded: case- and Unicode-normalized). Exact or prefix match on the item's source name (see nameMatch), NOT a substring search."
                },
                "nameMatch": {
                    "type": "string",
                    "enum": ["exact", "prefix"],
                    "description": "How 'name' matches: exact folded-name equality, or folded-name prefix. Default: prefix."
                },
                "kind": {
                    "type": "string",
                    "enum": ["copy", "move", "delete", "trash", "rename", "createFolder", "createFile", "archiveEdit"],
                    "description": "Filter by operation kind"
                },
                "initiator": {
                    "type": "string",
                    "enum": ["user", "aiClient", "agent"],
                    "description": "Filter by who initiated the operation"
                },
                "executionStatus": {
                    "type": "string",
                    "enum": ["queued", "running", "done", "failed", "canceled"],
                    "description": "Filter by lifecycle status"
                },
                "rollbackState": {
                    "type": "string",
                    "enum": ["notRollbackable", "rollbackable", "rollingBack", "rolledBack", "partiallyRolledBack"],
                    "description": "Filter by rollback state"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max operations to return. Default 50, max 1000."
                },
                "offset": {
                    "type": "integer",
                    "description": "Number of operations to skip, for paging"
                }
            },
            "required": []
        }),
        gate: TokenGate::Open,
        run: app_params operation_log::execute_operations_list
    },
    "operations_get" => {
        desc: "Get one operation's header plus a page of its item rows (full source/dest paths, per-item outcome). Use after operations_list; poll this to watch a rollback settle (rollbackState leaves 'rollingBack').",
        schema: json!({
            "type": "object",
            "properties": {
                "operationId": {
                    "type": "string",
                    "description": "The operation's id. The same id everywhere: from operations_list, a copy/move/delete response, cmdr://state operations, or the queue tool."
                },
                "limit": {
                    "type": "integer",
                    "description": "Max item rows to return. Default 200, max 1000."
                },
                "offset": {
                    "type": "integer",
                    "description": "Number of item rows to skip, for paging"
                }
            },
            "required": ["operationId"]
        }),
        gate: TokenGate::Open,
        run: app_params operation_log::execute_operations_get
    },
    "operations_rollback" => {
        desc: "Reverse a logged operation (delete the copies, move back, restore from trash). Rechecks each item and never overwrites; a drifted or occupied item is skipped. Returns after dispatch: poll operations_get until rollbackState leaves 'rollingBack'.",
        schema: json!({
            "type": "object",
            "properties": {
                "operationId": {
                    "type": "string",
                    "description": "The operation to reverse. Same id as operations_list, a copy/move/delete response, or cmdr://state operations."
                },
                "autoConfirm": {
                    "type": "boolean",
                    "description": "Must be true to roll back: a rollback writes to disk, so (like copy/move/delete) it requires the bearer token. Returns once the reversal is dispatched; poll operations_get until rollbackState leaves 'rollingBack'."
                }
            },
            "required": ["operationId"]
        }),
        gate: TokenGate::IfAutoConfirm,
        run: app_params operation_log::execute_operations_rollback
    },
}
