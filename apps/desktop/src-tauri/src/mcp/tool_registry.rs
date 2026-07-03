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
use super::executor::{app, async_tools, dialogs, downloads, file_ops, nav, search, view};
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
        desc: "Switch pane to specified volume by name",
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
        desc: "Navigate pane to specified path",
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
        desc: "Navigate to parent folder",
        schema: no_params_schema(),
        gate: TokenGate::Open,
        run: nav nav::execute_nav_command
    },
    "nav_back" => {
        desc: "Navigate back in history",
        schema: no_params_schema(),
        gate: TokenGate::Open,
        run: nav nav::execute_nav_command
    },
    "nav_forward" => {
        desc: "Navigate forward in history",
        schema: no_params_schema(),
        gate: TokenGate::Open,
        run: nav nav::execute_nav_command
    },
    "scroll_to" => {
        desc: "Load region around specified index for large directories",
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
        desc: "Focuses pane and moves cursor to index or filename. Provide either index or filename",
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
        desc: "Open/enter the item (directory, file, network host, share) under the cursor",
        schema: no_params_schema(),
        gate: TokenGate::Open,
        run: nav nav::execute_nav_command
    },

    // ── Selection ───────────────────────────────────────────────────────────
    "select" => {
        desc: "Select files in pane. Use names for specific files, count for ranges, all for everything, count=0 to clear",
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
        desc: "Copy selected files to other pane (opens confirmation dialog)",
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
        desc: "Move selected files to other pane (opens confirmation dialog)",
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
    "delete" => {
        desc: "Delete selected files (opens confirmation dialog)",
        schema: json!({
            "type": "object",
            "properties": {
                "autoConfirm": {
                    "type": "boolean",
                    "description": "When true, dialog opens and immediately confirms without waiting for user interaction. Returns once the operation starts."
                }
            },
            "required": []
        }),
        gate: TokenGate::IfAutoConfirm,
        run: app_params file_ops::execute_delete
    },
    "mkdir" => {
        desc: "Create folder in focused pane (triggers naming dialog)",
        schema: no_params_schema(),
        gate: TokenGate::Open,
        run: app_only file_ops::execute_mkdir
    },
    "mkfile" => {
        desc: "Create file in focused pane (triggers naming dialog)",
        schema: no_params_schema(),
        gate: TokenGate::Open,
        run: app_only file_ops::execute_mkfile
    },
    "refresh" => {
        desc: "Refresh focused pane",
        schema: no_params_schema(),
        gate: TokenGate::Open,
        run: app_only file_ops::execute_refresh
    },

    // ── View ────────────────────────────────────────────────────────────────
    "toggle_hidden" => {
        desc: "Toggle hidden files visibility",
        schema: no_params_schema(),
        gate: TokenGate::Open,
        run: app_only view::execute_toggle_hidden
    },
    "set_view_mode" => {
        desc: "Set view mode for pane",
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
        desc: "Sort files in pane by field and order",
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
        desc: "Create, close, activate, pin, or reopen tabs",
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
        desc: "Open, focus, close, or confirm dialogs",
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
                    "enum": ["settings", "file-viewer", "about", "transfer-confirmation", "copy-confirmation", "mkdir-confirmation", "new-file-confirmation", "delete-confirmation"],
                    "description": "Dialog type. 'transfer-confirmation' covers both copy and move dialogs (preferred over 'copy-confirmation')."
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
        desc: "Switch focus to the other pane",
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
        desc: "Structured file search across the entire drive index",
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
        desc: "Natural language file search using the configured LLM to translate the query",
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

    // ── Async ───────────────────────────────────────────────────────────────
    "await" => {
        desc: "Wait until a condition is met on a pane. Use after fire-and-forget actions or to wait for async events like network discovery.",
        schema: json!({
            "type": "object",
            "properties": {
                "pane": {
                    "type": "string",
                    "enum": ["left", "right"],
                    "description": "Which pane to watch"
                },
                "condition": {
                    "type": "string",
                    "enum": ["has_item", "not_has_item", "item_count_gte", "item_count_lte", "path", "path_contains"],
                    "description": "Condition to wait for: has_item / not_has_item (file list contains / no longer contains item named value — use not_has_item after a delete), item_count_gte / item_count_lte (file list has >= / <= value items), path (pane path equals value), path_contains (pane path contains value)"
                },
                "value": {
                    "type": "string",
                    "description": "Value for the condition (item name, count, path, or substring)"
                },
                "timeoutSeconds": {
                    "type": "integer",
                    "description": "Timeout in seconds (default 15, max 60)"
                },
                "afterGeneration": {
                    "type": "integer",
                    "description": "Only consider state updates after this generation number. Prevents matching stale state from before an action. Get the current generation from cmdr://state or a previous await result."
                }
            },
            "required": ["pane", "condition", "value"]
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool<'a>(tools: &'a [Tool], name: &str) -> &'a Tool {
        tools.iter().find(|t| t.name == name).expect("tool present")
    }

    /// The exact set of tool names on the wire. Dispatch (`execute_tool`) is generated from
    /// the same table, so it covers exactly this set by construction; this pins the set so a
    /// stray add/remove/rename is a hard failure, not a silent one.
    const EXPECTED_TOOL_NAMES: &[&str] = &[
        "select_volume",
        "nav_to_path",
        "nav_to_parent",
        "nav_back",
        "nav_forward",
        "scroll_to",
        "move_cursor",
        "open_under_cursor",
        "select",
        "copy",
        "move",
        "delete",
        "mkdir",
        "mkfile",
        "refresh",
        "toggle_hidden",
        "set_view_mode",
        "sort",
        "tab",
        "dialog",
        "open_search_dialog",
        "quit",
        "switch_pane",
        "swap_panes",
        "search",
        "ai_search",
        "set_setting",
        "connect_to_server",
        "remove_manual_server",
        "upgrade_smb_to_direct",
        "await",
        "go_to_latest_download",
    ];

    #[test]
    fn test_all_tools_count() {
        // 6 nav + 2 cursor + 1 selection + 6 file_op + 3 view + 1 tab + 2 dialog + 3 app + 2
        // search + 1 settings + 3 network + 1 await + 1 downloads = 32
        assert_eq!(get_all_tools().len(), 32);
    }

    #[test]
    fn test_tool_names_are_exactly_the_expected_set() {
        use std::collections::BTreeSet;
        let actual: BTreeSet<String> = get_all_tools().into_iter().map(|t| t.name).collect();
        let expected: BTreeSet<String> = EXPECTED_TOOL_NAMES.iter().map(|s| (*s).to_owned()).collect();
        assert_eq!(actual, expected, "tool name set drifted from the expected 32");
    }

    #[test]
    fn test_tab_tool_schema() {
        let tools = get_all_tools();
        let schema = &tool(&tools, "tab").input_schema;
        let props = schema.get("properties").unwrap();

        assert!(props.get("action").is_some());
        assert!(props.get("pane").is_some());
        assert!(props.get("tabId").is_some());
        assert!(props.get("pinned").is_some());

        let action_enum = props.get("action").unwrap().get("enum").unwrap().as_array().unwrap();
        assert!(action_enum.contains(&json!("new")));
        assert!(action_enum.contains(&json!("close")));
        assert!(action_enum.contains(&json!("close_others")));
        assert!(action_enum.contains(&json!("activate")));
        assert!(action_enum.contains(&json!("set_pinned")));
        assert!(action_enum.contains(&json!("reopen")));

        let pane_enum = props.get("pane").unwrap().get("enum").unwrap().as_array().unwrap();
        assert!(pane_enum.contains(&json!("left")));
        assert!(pane_enum.contains(&json!("right")));

        let required = schema.get("required").unwrap().as_array().unwrap();
        assert_eq!(required.len(), 2);
        assert!(required.contains(&json!("action")));
        assert!(required.contains(&json!("pane")));
    }

    #[test]
    fn test_set_setting_tool_schema() {
        let tools = get_all_tools();
        let schema = &tool(&tools, "set_setting").input_schema;
        let props = schema.get("properties").unwrap();
        assert!(props.get("id").is_some());
        assert!(props.get("value").is_some());

        let required = schema.get("required").unwrap().as_array().unwrap();
        assert_eq!(required.len(), 2);
        assert!(required.contains(&json!("id")));
        assert!(required.contains(&json!("value")));
    }

    #[test]
    fn test_open_search_dialog_schema() {
        let tools = get_all_tools();
        let schema = &tool(&tools, "open_search_dialog").input_schema;
        let props = schema.get("properties").unwrap();

        for key in [
            "query",
            "mode",
            "sizeMin",
            "sizeMax",
            "modifiedAfter",
            "modifiedBefore",
            "isDirectory",
            "scope",
            "caseSensitive",
            "excludeSystemDirs",
            "autoRun",
        ] {
            assert!(props.get(key).is_some(), "open_search_dialog schema missing '{key}'");
        }

        let mode_enum = props.get("mode").unwrap().get("enum").unwrap().as_array().unwrap();
        assert!(mode_enum.contains(&json!("ai")));
        assert!(mode_enum.contains(&json!("filename")));
        assert!(mode_enum.contains(&json!("regex")));

        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.is_empty(), "open_search_dialog should have no required fields");
    }

    #[test]
    fn test_select_tool_schema() {
        let tools = get_all_tools();
        let schema = &tool(&tools, "select").input_schema;
        let props = schema.get("properties").unwrap();

        assert!(props.get("pane").is_some());
        assert!(props.get("start").is_some());
        assert!(props.get("count").is_some());
        assert!(props.get("all").is_some());
        assert!(props.get("mode").is_some());

        // count should be a plain integer, not oneOf (schemars would break this)
        assert_eq!(props["count"]["type"], "integer");
        assert_eq!(props["all"]["type"], "boolean");

        let required = schema.get("required").unwrap().as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert!(required.contains(&json!("pane")));
    }

    #[test]
    fn test_move_cursor_tool_schema() {
        let tools = get_all_tools();
        let schema = &tool(&tools, "move_cursor").input_schema;
        let props = schema.get("properties").unwrap();

        assert!(props.get("pane").is_some());
        assert_eq!(props["index"]["type"], "integer");
        assert_eq!(props["filename"]["type"], "string");

        let required = schema.get("required").unwrap().as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert!(required.contains(&json!("pane")));

        // move_cursor normalizes index/filename in the handler; no "to" property on the wire
        assert!(props.get("to").is_none());
    }

    #[test]
    fn test_dialog_tool_schema() {
        let tools = get_all_tools();
        let schema = &tool(&tools, "dialog").input_schema;
        let props = schema.get("properties").unwrap();

        assert!(props.get("action").is_some());
        assert!(props.get("type").is_some());
        assert!(props.get("section").is_some());
        assert!(props.get("path").is_some());
        assert!(props.get("onConflict").is_some());

        let action_enum = props.get("action").unwrap().get("enum").unwrap().as_array().unwrap();
        assert!(action_enum.contains(&json!("open")));
        assert!(action_enum.contains(&json!("focus")));
        assert!(action_enum.contains(&json!("close")));
        assert!(action_enum.contains(&json!("confirm")));

        let type_enum = props.get("type").unwrap().get("enum").unwrap().as_array().unwrap();
        assert!(type_enum.contains(&json!("settings")));
        assert!(type_enum.contains(&json!("file-viewer")));
        assert!(type_enum.contains(&json!("about")));
        assert!(type_enum.contains(&json!("transfer-confirmation")));
        assert!(type_enum.contains(&json!("copy-confirmation")));
        assert!(type_enum.contains(&json!("mkdir-confirmation")));
        assert!(type_enum.contains(&json!("new-file-confirmation")));
        assert!(type_enum.contains(&json!("delete-confirmation")));

        let required = schema.get("required").unwrap().as_array().unwrap();
        assert_eq!(required.len(), 2);
        assert!(required.contains(&json!("action")));
        assert!(required.contains(&json!("type")));
    }

    #[test]
    fn test_sort_tool_schema() {
        let tools = get_all_tools();
        let schema = &tool(&tools, "sort").input_schema;
        let props = schema.get("properties").unwrap();

        assert!(props.get("pane").is_some());
        assert!(props.get("by").is_some());
        assert!(props.get("order").is_some());

        let by_enum = props.get("by").unwrap().get("enum").unwrap().as_array().unwrap();
        assert!(by_enum.contains(&json!("name")));
        assert!(by_enum.contains(&json!("ext")));
        assert!(by_enum.contains(&json!("size")));
        assert!(by_enum.contains(&json!("modified")));
        assert!(by_enum.contains(&json!("created")));

        let order_enum = props.get("order").unwrap().get("enum").unwrap().as_array().unwrap();
        assert!(order_enum.contains(&json!("asc")));
        assert!(order_enum.contains(&json!("desc")));

        let required = schema.get("required").unwrap().as_array().unwrap();
        assert_eq!(required.len(), 3);
        assert!(required.contains(&json!("pane")));
        assert!(required.contains(&json!("by")));
        assert!(required.contains(&json!("order")));
    }

    #[test]
    fn test_set_view_mode_tool_schema() {
        let tools = get_all_tools();
        let schema = &tool(&tools, "set_view_mode").input_schema;
        let props = schema.get("properties").unwrap();

        assert!(props.get("pane").is_some());
        assert!(props.get("mode").is_some());

        let mode_enum = props.get("mode").unwrap().get("enum").unwrap().as_array().unwrap();
        assert!(mode_enum.contains(&json!("brief")));
        assert!(mode_enum.contains(&json!("full")));

        let required = schema.get("required").unwrap().as_array().unwrap();
        assert_eq!(required.len(), 2);
        assert!(required.contains(&json!("pane")));
        assert!(required.contains(&json!("mode")));
    }

    #[test]
    fn test_downloads_tool_present() {
        let tools = get_all_tools();
        assert_eq!(tool(&tools, "go_to_latest_download").name, "go_to_latest_download");
    }

    // ── Token gate (auth classification) ──────────────────────────────────────

    #[test]
    fn test_tool_gate_per_name() {
        assert_eq!(tool_gate("copy"), Some(TokenGate::IfAutoConfirm));
        assert_eq!(tool_gate("move"), Some(TokenGate::IfAutoConfirm));
        assert_eq!(tool_gate("delete"), Some(TokenGate::IfAutoConfirm));
        assert_eq!(tool_gate("set_setting"), Some(TokenGate::Always));
        assert_eq!(tool_gate("dialog"), Some(TokenGate::IfConfirmAction));
        assert_eq!(tool_gate("nav_to_path"), Some(TokenGate::Open));
        assert_eq!(tool_gate("bogus"), None);
    }

    /// Anti-footgun backstop: any tool whose schema takes `autoConfirm` (i.e. can bypass the
    /// user's confirmation dialog) MUST be gated `IfAutoConfirm`, never left `Open`. Adding a
    /// destructive auto-confirm tool and forgetting its gate fails here.
    #[test]
    fn test_autoconfirm_tools_are_gated() {
        for t in get_all_tools() {
            let has_auto_confirm = t
                .input_schema
                .get("properties")
                .and_then(|p| p.get("autoConfirm"))
                .is_some();
            if has_auto_confirm {
                assert_eq!(
                    tool_gate(&t.name),
                    Some(TokenGate::IfAutoConfirm),
                    "tool '{}' exposes autoConfirm but isn't gated IfAutoConfirm",
                    t.name
                );
            }
        }
    }

    /// Full-table expectation with set-equality: every tool's gate is pinned, AND the set of
    /// tools in the registry equals the set with a declared gate. Set-equality is load-bearing:
    /// it forces a conscious auth review for any new tool (a 33rd tool left `Open` fails here).
    #[test]
    fn test_gate_table_is_complete_and_correct() {
        use std::collections::BTreeMap;
        let expected: BTreeMap<&str, TokenGate> = [
            ("select_volume", TokenGate::Open),
            ("nav_to_path", TokenGate::Open),
            ("nav_to_parent", TokenGate::Open),
            ("nav_back", TokenGate::Open),
            ("nav_forward", TokenGate::Open),
            ("scroll_to", TokenGate::Open),
            ("move_cursor", TokenGate::Open),
            ("open_under_cursor", TokenGate::Open),
            ("select", TokenGate::Open),
            ("copy", TokenGate::IfAutoConfirm),
            ("move", TokenGate::IfAutoConfirm),
            ("delete", TokenGate::IfAutoConfirm),
            ("mkdir", TokenGate::Open),
            ("mkfile", TokenGate::Open),
            ("refresh", TokenGate::Open),
            ("toggle_hidden", TokenGate::Open),
            ("set_view_mode", TokenGate::Open),
            ("sort", TokenGate::Open),
            ("tab", TokenGate::Open),
            ("dialog", TokenGate::IfConfirmAction),
            ("open_search_dialog", TokenGate::Open),
            ("quit", TokenGate::Open),
            ("switch_pane", TokenGate::Open),
            ("swap_panes", TokenGate::Open),
            ("search", TokenGate::Open),
            ("ai_search", TokenGate::Open),
            ("set_setting", TokenGate::Always),
            ("connect_to_server", TokenGate::Open),
            ("remove_manual_server", TokenGate::Open),
            ("upgrade_smb_to_direct", TokenGate::Open),
            ("await", TokenGate::Open),
            ("go_to_latest_download", TokenGate::Open),
        ]
        .into_iter()
        .collect();

        let actual: std::collections::BTreeSet<String> = get_all_tools().into_iter().map(|t| t.name).collect();
        let expected_names: std::collections::BTreeSet<String> = expected.keys().map(|s| (*s).to_owned()).collect();
        assert_eq!(actual, expected_names, "registry tool set differs from the gate table");

        for (name, gate) in expected {
            assert_eq!(
                tool_gate(name),
                Some(gate),
                "gate for '{name}' differs from expectation"
            );
        }
    }

    #[test]
    fn test_requires_token_arg_logic() {
        // IfAutoConfirm: only when autoConfirm == true
        assert!(TokenGate::IfAutoConfirm.requires_token(Some(&json!({"autoConfirm": true}))));
        assert!(!TokenGate::IfAutoConfirm.requires_token(Some(&json!({"autoConfirm": false}))));
        assert!(!TokenGate::IfAutoConfirm.requires_token(Some(&json!({}))));
        assert!(!TokenGate::IfAutoConfirm.requires_token(None));
        // IfConfirmAction: only when action == "confirm"
        assert!(TokenGate::IfConfirmAction.requires_token(Some(&json!({"action": "confirm"}))));
        assert!(!TokenGate::IfConfirmAction.requires_token(Some(&json!({"action": "open"}))));
        assert!(!TokenGate::IfConfirmAction.requires_token(None));
        // Always / Open
        assert!(TokenGate::Always.requires_token(None));
        assert!(!TokenGate::Open.requires_token(Some(&json!({"autoConfirm": true}))));
    }
}
