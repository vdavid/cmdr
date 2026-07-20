//! Single source of truth for MCP tools.
//!
//! Each tool is authored exactly once in the `mcp_tools!` table below, bundling its name,
//! description, JSON input schema, bearer-token gate, consumer exposure, access class, and
//! handler. The macro expands that one table into every consumer, so the facets can't drift:
//!
//! - [`get_all_tools`] — the AI-client `tools/list` payload (entries whose `consumers` include
//!   [`Consumer::AiClient`]; non-generic; server + tests read it).
//! - [`agent_tool_view`] — the in-process agent's tool set (entries whose `consumers` include
//!   [`Consumer::Agent`]): the read (and, once authored, propose) families the chat agent dispatches.
//! - [`execute_tool`] — the `tools/call` dispatch (generic over `Runtime`), gated to the caller's
//!   consumer view: a name outside the caller's view is refused before dispatch.
//! - [`tool_gate`] + [`TokenGate`] — the auth classification `auth.rs` reads.
//! - [`tool_consumers`] / [`tool_access`] — the two new dimensions, read by the structural tests.
//!
//! Adding a tool means adding one entry: you can't add it without supplying a schema, a gate,
//! consumers, an access class, and a handler, and you can't add a handler the dispatch doesn't
//! know about. The count and coverage tests are then cheap guards over a property that's true by
//! construction.
//!
//! **Two view dimensions, why both (agent-spec D49/D59):** one authored registry feeds two
//! consumers. `consumers` is the exposure axis — the agent's dispatch view physically excludes
//! every tool not tagged `[agent]`, so its write path is absent by construction, not policy.
//! `access` is a stronger guarantee than the gate can give: [`TokenGate::Open`] covers
//! destructive-but-prompting ops (`copy`/`move`/`delete` with `autoConfirm` absent carry
//! `IfAutoConfirm`, effectively open), so a gate-based agent filter would let a destructive tool
//! into the agent's view. The structural tests pin the agent view to exactly its authored
//! `[agent]` entries AND require every one to be [`Access::Read`] or [`Access::Propose`], never
//! [`Access::Write`]. **The agent can propose; only the user can approve** — no tool approves a
//! proposal.
//!
//! Wire output must stay byte-identical: each schema is the exact `json!` block (hoisted into
//! [`schemas`] verbatim), and the tool order is the historical category concatenation. The
//! `tool_snapshot_tests` fixture pins it. Schema keys serialize alphabetically (serde_json `Map`
//! is a `BTreeMap`; `preserve_order` is off), so authored key order never affects the bytes.
//!
//! Layering: this module depends on the `executor` handlers and its own `schemas`; `auth` depends
//! on this module. It must not depend on `server` or `auth` (that would cycle).

mod gate;
mod schemas;

pub use gate::TokenGate;

use serde_json::Value;

use super::executor::{ToolError, ToolResult};
use super::executor::{
    app, async_tools, dialogs, downloads, eject, favorites, file_ops, image_facts, indexing, nav, operation_log,
    photos, queue, search, tags, view,
};
use super::tools::Tool;

/// Which AI consumer a tool is exposed to. One authored registry, per-consumer views (D49):
/// the MCP HTTP server dispatches the [`AiClient`](Consumer::AiClient) view, the in-process agent
/// runtime dispatches the [`Agent`](Consumer::Agent) view, and neither can reach the other's
/// tools ([`execute_tool`] refuses a name outside the caller's view).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consumer {
    /// External MCP clients over the HTTP transport (dev tooling, Claude Code, E2E).
    AiClient,
    /// The in-process Ask Cmdr agent runtime, which dispatches the agent view via
    /// [`execute_tool`] with this identity (`crate::agent::tools`).
    Agent,
}

/// Whether a tool reads, asks, or mutates. The agent view admits `Read` and `Propose` and must
/// contain zero `Write` tools — this is the guarantee [`TokenGate`] alone can't give (see the
/// module docs).
///
/// The agent dispatch (`crate::agent::tools`) reads [`tool_access`] as a runtime backstop: it
/// refuses to execute any tool classified `Write`, so "the agent can't act" holds even against a
/// mis-tagged entry. It's registry metadata, not a field on the emitted `Tool`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Reads state and mutates nothing.
    Read,
    /// Stages a proposal and opens a review surface for the user. Mutates nothing: no filesystem
    /// write, no silent config change. **The agent can propose; only the user can approve** —
    /// approval originates in the frontend as a user action, and there is no tool that approves a
    /// proposal. A `Propose` tool is authored by hand into the test allowlist
    /// (`EXPECTED_PROPOSE_TOOL_NAMES`), because no structural check can prove a handler doesn't
    /// mutate.
    ///
    /// No registry entry is tagged `Propose` yet, so `#![deny(unused)]` would reject the variant.
    /// The tier ships before its first tool on purpose: the boundary is reviewed on its own, not
    /// bundled into a feature. Drop this `allow` when the first `Propose` tool is authored.
    #[allow(
        dead_code,
        reason = "no Propose tool is authored yet; the tier lands before its first tool"
    )]
    Propose,
    /// Mutates the filesystem OR app state (nav, cursor, selection, tabs, dialogs, settings,
    /// connect/eject, file ops, rollback-cancel); when in doubt a tool is `Write`. Never reachable
    /// from the agent view.
    Write,
}

/// Whether `name` is listable/dispatchable by `consumer` — its authored `consumers` set includes
/// it. The choke point [`execute_tool`] consults before dispatch, and the invariant the
/// structural tests pin ("no transport dispatches a name outside its consumer view"). The
/// decision is on the typed [`Consumer`] set, never a string (no-string-matching).
pub fn tool_available_to(name: &str, consumer: Consumer) -> bool {
    tool_consumers(name).is_some_and(|cs| cs.contains(&consumer))
}

/// Declarative tool table → the consumers (`get_all_tools`, `agent_tool_view`, `execute_tool`,
/// `tool_gate`, `tool_consumers`, `tool_access`).
///
/// Entry form:
/// `"name" => { desc, schema, gate, consumers: &[..], access: .., run: <shape> <handler-path> }`.
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
    ( $( $name:literal => {
        desc: $desc:expr,
        schema: $schema:expr,
        gate: $gate:expr,
        consumers: $consumers:expr,
        access: $access:expr,
        run: $shape:tt $path:path
    } ),* $(,)? ) => {
        /// The AI-client `tools/list` payload: every `[ai_client]` tool in wire order. Agent-only
        /// entries are filtered out, so this stays byte-identical to the pre-dimension output for
        /// the tools it already contained.
        pub fn get_all_tools() -> Vec<Tool> {
            let mut tools = Vec::new();
            $(
                if $consumers.contains(&Consumer::AiClient) {
                    tools.push(Tool { name: $name.into(), description: $desc.into(), input_schema: $schema });
                }
            )*
            tools
        }

        /// The in-process agent's tool set: every `[agent]` tool in table order. The agent
        /// runtime (`crate::agent::tools`) turns these into `ToolDeclaration`s and dispatches
        /// them; the structural set-equality + all-`Read` tests pin the set.
        pub fn agent_tool_view() -> Vec<Tool> {
            let mut tools = Vec::new();
            $(
                if $consumers.contains(&Consumer::Agent) {
                    tools.push(Tool { name: $name.into(), description: $desc.into(), input_schema: $schema });
                }
            )*
            tools
        }

        /// The bearer-token classification for a tool, or `None` for an unknown name. The
        /// single source `auth::tool_call_requires_token` reads.
        pub fn tool_gate(name: &str) -> Option<TokenGate> {
            match name {
                $( $name => Some($gate), )*
                _ => None,
            }
        }

        /// The consumer exposure for a tool, or `None` for an unknown name.
        pub fn tool_consumers(name: &str) -> Option<&'static [Consumer]> {
            match name {
                $( $name => Some($consumers), )*
                _ => None,
            }
        }

        /// The access class for a tool, or `None` for an unknown name. Read by the structural
        /// tests and by the agent dispatch's runtime read-only backstop (`crate::agent::tools`).
        pub fn tool_access(name: &str) -> Option<Access> {
            match name {
                $( $name => Some($access), )*
                _ => None,
            }
        }

        /// The `tools/call` dispatch, gated to `consumer`'s view. A name outside that view (an
        /// agent-only name over MCP, an `ai_client`-only name through the agent runtime, or an
        /// unknown name) is refused before dispatch with the same `INVALID_PARAMS` "Unknown tool"
        /// error — the refusal is on the typed [`Consumer`] set, not a string. Generic over
        /// `Runtime`.
        pub async fn execute_tool<R: tauri::Runtime>(
            app: &tauri::AppHandle<R>,
            consumer: Consumer,
            name: &str,
            params: &Value,
        ) -> ToolResult {
            if !tool_available_to(name, consumer) {
                return Err(ToolError::invalid_params(format!("Unknown tool: {name}")));
            }
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

mcp_tools! {
    // ── Navigation ──────────────────────────────────────────────────────────
    "select_volume" => {
        desc: "Switch a pane to a volume by name (as listed in cmdr://state volumes): a disk, SMB share, MTP device, or Network. To move within the current volume, use nav_to_path instead.",
        schema: schemas::select_volume_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: nav_params nav::execute_nav_command_with_params
    },
    "nav_to_path" => {
        desc: "Navigate a pane to a path: absolute, ~-relative, or virtual (mtp://, smb://). Prefer this over stepping with nav_to_parent when you know the target. Archive paths are transparent, so a path through foo.zip/inner navigates inside the archive.",
        schema: schemas::nav_to_path_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: nav_params nav::execute_nav_command_with_params
    },
    "nav_to_parent" => {
        desc: "Navigate the focused pane up to its parent folder.",
        schema: schemas::no_params_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: nav nav::execute_nav_command
    },
    "nav_back" => {
        desc: "Go back to the focused pane's previous folder in its navigation history.",
        schema: schemas::no_params_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: nav nav::execute_nav_command
    },
    "nav_forward" => {
        desc: "Go forward again (undo a nav_back) in the focused pane's navigation history.",
        schema: schemas::no_params_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: nav nav::execute_nav_command
    },
    "scroll_to" => {
        desc: "Load the file window around an index in a large (paginated) directory so those rows appear in cmdr://state. Needed before move_cursor / select can reach a row outside the currently loaded range.",
        schema: schemas::scroll_to_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: nav_params nav::execute_nav_command_with_params
    },

    // ── Cursor ──────────────────────────────────────────────────────────────
    "move_cursor" => {
        desc: "Focus a pane and move its cursor to a row, by zero-based index or by filename (give one). Flushes pane state, so a following copy / move / delete / rename acts on this row. A missing filename or out-of-range index is an honest error, never a silent no-op.",
        schema: schemas::move_cursor_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: nav_params nav::execute_nav_command_with_params
    },
    "open_under_cursor" => {
        desc: "Open the item under the cursor, like pressing Enter: enter a folder, open a file, or connect a network host / share.",
        schema: schemas::no_params_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: nav nav::execute_nav_command
    },

    // ── Selection ───────────────────────────────────────────────────────────
    "select" => {
        desc: "Select files in a pane by names, by an index range (start + count), or all; count=0 clears. Focuses the pane and flushes state, so a following copy / move / delete / compress acts on this selection. names errors if any name isn't in the listing.",
        schema: schemas::select_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params file_ops::execute_select_command
    },

    // ── File operations ─────────────────────────────────────────────────────
    "copy" => {
        desc: "Copy the selection (else the cursor item) to the other pane. Without autoConfirm, opens the confirm dialog. With autoConfirm, starts at once and returns the operationId (await operation_complete, or steer with queue). onConflict resolves file clashes.",
        schema: schemas::copy_schema(),
        gate: TokenGate::IfAutoConfirm,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params file_ops::execute_copy
    },
    "move" => {
        desc: "Move the selection (else the cursor item) to the other pane. Without autoConfirm, opens the confirm dialog. With autoConfirm, starts at once and returns the operationId (await operation_complete, or steer with queue). onConflict resolves file clashes.",
        schema: schemas::move_schema(),
        gate: TokenGate::IfAutoConfirm,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params file_ops::execute_move
    },
    "compress" => {
        desc: "Zip the selection into a new archive in the other pane. Without autoConfirm, opens the confirm dialog. With autoConfirm, starts and returns the operationId — unless the target archive exists, where the dialog stays open to confirm the overwrite.",
        schema: schemas::compress_schema(),
        gate: TokenGate::IfAutoConfirm,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params file_ops::execute_compress
    },
    "delete" => {
        desc: "Delete the selection (else the cursor item). Without autoConfirm, opens the confirm dialog. With autoConfirm, starts at once and returns the operationId (await operation_complete on it). mode presets trash vs permanent; omit for the pane's default.",
        schema: schemas::delete_schema(),
        gate: TokenGate::IfAutoConfirm,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params file_ops::execute_delete
    },
    "rename" => {
        desc: "Rename an item (the named item, else the cursor item) in a pane. Without autoConfirm, \
               opens the inline rename editor prefilled with newName for the user to confirm. With \
               autoConfirm, renames directly (errors if the name already exists).",
        schema: schemas::rename_schema(),
        gate: TokenGate::IfAutoConfirm,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params file_ops::execute_rename
    },
    "mkdir" => {
        desc: "Create a folder in the focused pane, or pass pane to target the other. No name opens the naming \
               dialog (user confirms, not MCP); a name prefills it; name + autoConfirm creates directly (errors on \
               a name conflict).",
        schema: schemas::mkdir_schema(),
        gate: TokenGate::IfAutoConfirm,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params file_ops::execute_mkdir
    },
    "mkfile" => {
        desc: "Create an empty file in the focused pane, or pass pane to target the other. No name opens the naming \
               dialog (user confirms, not MCP); a name prefills it; name + autoConfirm creates directly (errors on \
               a name conflict).",
        schema: schemas::mkfile_schema(),
        gate: TokenGate::IfAutoConfirm,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params file_ops::execute_mkfile
    },
    "refresh" => {
        desc: "Force a re-read of the focused pane's listing (from disk on local volumes; the watcher cache short-circuits on MTP / SMB). Use after an out-of-band change; navigation and file ops already refresh on their own.",
        schema: schemas::no_params_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_only file_ops::execute_refresh
    },
    "tag" => {
        desc: "Set macOS Finder color tags on files by name (else selection, else cursor). set: \
               make the colors exactly (keeps colorless tags). toggle: flip each color. clear: \
               remove all. macOS only; tags show in cmdr://state as [tags:red,blue].",
        schema: schemas::tag_schema(),
        gate: TokenGate::Always,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params tags::execute_tag
    },

    // ── View ────────────────────────────────────────────────────────────────
    "toggle_hidden" => {
        desc: "Toggle whether hidden (dotfile) files show in the file lists (the showHidden flag in cmdr://state).",
        schema: schemas::no_params_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_only view::execute_toggle_hidden
    },
    "set_view_mode" => {
        desc: "Set a pane's view mode: brief (names, only the cursor row detailed) or full (size and date on every row). full makes cmdr://state carry those details for all rows, not just the cursor.",
        schema: schemas::set_view_mode_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params view::execute_set_view_mode
    },
    "sort" => {
        desc: "Sort a pane by a field (name, ext, size, modified, created) and order (asc / desc).",
        schema: schemas::sort_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params view::execute_sort
    },

    // ── Tabs ────────────────────────────────────────────────────────────────
    "tab" => {
        desc: "Manage a pane's tabs: new, close, close_others, activate, set_pinned, or reopen (restore the last-closed tab). tabId defaults to the active tab where it applies; see each pane's tabs in cmdr://state.",
        schema: schemas::tab_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params app::execute_tab
    },

    // ── Dialogs ─────────────────────────────────────────────────────────────
    "dialog" => {
        desc: "Open, focus, close, or confirm a dialog. Open/focus: settings, file-viewer, about, onboarding. Close: any id from cmdr://dialogs/available. confirm (token-gated) accepts an open confirmation. cmdr://state lists what's open.",
        schema: schemas::dialog_schema(),
        gate: TokenGate::IfConfirmAction,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params dialogs::execute_dialog_command
    },
    "open_search_dialog" => {
        desc: "Open the search dialog with optional pre-filled query and filters. If autoRun (default true), runs the search immediately. Acks once the dialog has mounted; does not wait for results to render.",
        schema: schemas::open_search_dialog_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params dialogs::execute_open_search_dialog
    },

    // ── App ─────────────────────────────────────────────────────────────────
    "quit" => {
        desc: "Quit the application",
        schema: schemas::no_params_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: sync_app app::execute_quit
    },
    "switch_pane" => {
        desc: "Toggle focus to the other pane. Takes no parameters (a pane arg is ignored). To focus a SPECIFIC pane, use select (with count 0 to clear) or select_volume / nav_to_path on that pane, which focus it.",
        schema: schemas::no_params_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: sync_app app::execute_switch_pane
    },
    "swap_panes" => {
        desc: "Swap left and right pane directories, view modes, sort orders, and selections",
        schema: schemas::no_params_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: sync_app app::execute_swap_panes
    },

    // ── Search ──────────────────────────────────────────────────────────────
    "search" => {
        desc: "Search the drive index by filename pattern, size, date, or type; returns paths (no UI). Set countOnly:true for just the total. Prefer over ai_search for a plain pattern/filter, over open_search_dialog for programmatic lookup. Needs an indexed volume.",
        schema: schemas::search_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Read,
        run: params_only search::execute_search
    },
    "ai_search" => {
        desc: "Search with a natural-language query; the configured LLM turns it into a structured search over the drive index and returns matching paths. Use search instead when you can express the query as a pattern or filter (it skips the LLM call).",
        schema: schemas::ai_search_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Read,
        run: params_only search::execute_ai_search
    },

    // ── Settings ────────────────────────────────────────────────────────────
    "set_setting" => {
        desc: "Set a setting value. Use the cmdr://settings resource to discover available settings and their constraints.",
        schema: schemas::set_setting_schema(),
        gate: TokenGate::Always,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params async_tools::execute_set_setting
    },

    // ── Indexing ────────────────────────────────────────────────────────────
    "indexing" => {
        desc: "Control one volume's drive indexing. Actions: enable (on, starts first scan), \
               disable (off, keeps DB), rescan (fresh full scan), forget (delete DB). enable/rescan \
               return once scanning starts; poll await index_status fresh for done. See cmdr://indexing.",
        schema: schemas::indexing_schema(),
        gate: TokenGate::Always,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: params_only indexing::execute_indexing
    },

    // ── Queue ───────────────────────────────────────────────────────────────
    "queue" => {
        desc: "Control the operation queue: pause / resume / cancel one operationId, or \
               pause_all / resume_all. cancel also takes operationIds (array) for several; \
               rollback: true deletes already-copied files (single op, token-gated). See \
               cmdr://state operations for ids.",
        schema: schemas::queue_schema(),
        gate: TokenGate::IfRollback,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: params_only queue::execute_queue
    },

    // ── Favorites ───────────────────────────────────────────────────────────
    "favorites" => {
        desc: "Manage the user's favorites (the switcher's Favorites section). add: path (+ \
               optional name). rename: id + name. remove: id. reorder: orderedIds, the COMPLETE \
               new ordering. Discover ids in cmdr://state favorites.",
        schema: schemas::favorites_schema(),
        gate: TokenGate::Always,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: params_only favorites::execute_favorites
    },

    // ── Network ─────────────────────────────────────────────────────────────
    "connect_to_server" => {
        desc: "Add a manual SMB server by address. Checks TCP reachability then adds to the host list.",
        schema: schemas::connect_to_server_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params async_tools::execute_connect_to_server
    },
    "remove_manual_server" => {
        desc: "Remove a manually-added server from the host list.",
        schema: schemas::remove_manual_server_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: sync_app_params async_tools::execute_remove_manual_server
    },
    "upgrade_smb_to_direct" => {
        desc: "Upgrade an OS-mounted SMB volume to a direct smb2 session for faster I/O. Uses \
               Keychain creds. Returns OK, NeedsCredentials, or NetworkError. See \
               cmdr://state volumes for each SMB share's smbConnectionState.",
        schema: schemas::upgrade_smb_to_direct_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params async_tools::execute_upgrade_smb_to_direct
    },
    "eject" => {
        desc: "Eject an ejectable volume by id (disk or MTP). Refuses honestly while an operation \
               is reading from or writing to the volume, and for non-ejectable volumes. See \
               cmdr://state volumes for ids.",
        schema: schemas::eject_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: params_only eject::execute_eject
    },

    // ── Async ───────────────────────────────────────────────────────────────
    "await" => {
        desc: "Wait until a condition is met, after fire-and-forget actions or async events. Pane conditions watch a pane; index_status watches a volume's indexing freshness; operation_complete / operations_idle watch the write-operation queue.",
        schema: schemas::await_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Read,
        run: app_params async_tools::execute_await
    },

    // ── Downloads ───────────────────────────────────────────────────────────
    "go_to_latest_download" => {
        desc: "Navigate the focused pane to the most recently observed eligible file in ~/Downloads and select it. Errors if no eligible file exists or Cmdr lacks Full Disk Access.",
        schema: schemas::no_params_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_only downloads::execute_go_to_latest_download
    },

    // ── Operation log ─────────────────────────────────────────────────────────
    "operations_list" => {
        desc: "List past operations from the durable operation log (copy, move, delete, trash, rename, create, compress), newest first. Filter by time, item name, kind, initiator, status; paged. In-flight ops live in cmdr://state operations + the queue tool.",
        schema: schemas::operations_list_schema(),
        gate: TokenGate::Open,
        // Shared read: the agent runtime uses the same core (the schemas fit unchanged).
        consumers: &[Consumer::AiClient, Consumer::Agent],
        access: Access::Read,
        run: app_params operation_log::execute_operations_list
    },
    "operations_get" => {
        desc: "Get one operation's header plus a page of its item rows (full source/dest paths, per-item outcome). Use after operations_list; poll this to watch a rollback settle (rollbackState leaves 'rollingBack').",
        schema: schemas::operations_get_schema(),
        gate: TokenGate::Open,
        // Shared read: the agent runtime uses the same core (the schemas fit unchanged).
        consumers: &[Consumer::AiClient, Consumer::Agent],
        access: Access::Read,
        run: app_params operation_log::execute_operations_get
    },
    "operations_rollback" => {
        desc: "Reverse a logged operation (delete the copies, move back, restore from trash). Rechecks each item and never overwrites; a drifted or occupied item is skipped. Returns after dispatch: poll operations_get until rollbackState leaves 'rollingBack'.",
        schema: schemas::operations_rollback_schema(),
        gate: TokenGate::IfAutoConfirm,
        consumers: &[Consumer::AiClient],
        access: Access::Write,
        run: app_params operation_log::execute_operations_rollback
    },

    // ── Photo search ──────────────────────────────────────────────────────────
    // Shared read (agent-spec D49: one authored entry, both consumer views). The in-app
    // Ask Cmdr agent AND external MCP clients search enriched photos. `access: Read` — it
    // only reads the media index. Handler shapes the `media_index` read API and never emits
    // image bytes (text-only DTO). PRIVACY: paths + the in-image OCR snippet / tag it returns
    // are image-derived text that egresses to the agent's provider — see `executor/photos.rs`.
    "search_photos" => {
        desc: "Find the user's photos by content: a scene description, text inside the image (OCR), or a tag. Returns matching file paths plus a short reason, read from the on-device index (no uploads). Omit mode to combine description + OCR. Needs image indexing on.",
        schema: photos::search_photos_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient, Consumer::Agent],
        access: Access::Read,
        run: app_params photos::execute_search_photos
    },
    // The LOOKUP direction of the same index (`search_photos` is the query direction): the
    // caller already has the paths and needs to know what's IN each image. Same sharing,
    // access, and gate as its sibling. PRIVACY: it returns the FULL stored OCR text, not a
    // snippet — the most sensitive thing either photo tool emits. See
    // `executor/image_facts.rs`.
    "image_facts" => {
        desc: "Look up what Cmdr's image index stored for images you already have: the full recognized text (OCR) plus Vision tags, per path. Use it to name or describe files you already know. Up to 200 paths; each answers indexed or notIndexed. Needs image indexing on.",
        schema: image_facts::image_facts_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::AiClient, Consumer::Agent],
        access: Access::Read,
        run: app_params image_facts::execute_image_facts
    },

    // ── Agent read-only tools ─────────────────────────────────────────────────
    // The Ask Cmdr agent's own read-only surface (agent-spec D49: one authored registry, two
    // consumer views). `consumers: [Agent]`, `access: Read` — filtered out of `get_all_tools()`,
    // so the ai-client wire snapshot is unchanged. Handlers, schemas, and typed result shapes are
    // colocated in `crate::agent::tools::read` (feature-organized). `gate: Open` is inert here (the
    // agent never crosses the MCP auth boundary); it's the honest classification for a read.
    "app_state" => {
        desc: "Snapshot the live app state: both panes (current folder, cursor item, selection, view mode, sort) and the mounted volumes with their index freshness and connectivity. Use this to ground an answer in what the user is looking at right now.",
        schema: crate::agent::tools::read::state::app_state_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::Agent],
        access: Access::Read,
        run: app_params crate::agent::tools::read::state::execute_app_state
    },
    "list_dir" => {
        desc: "List a directory's immediate children (names, folder/file, size, modified) plus its recursive size totals, from the drive index. Reports index freshness honestly (fresh / scanning / stale) and returns a typed 'no index' when the volume isn't indexed, never a wrong zero. Reads the index only — it never touches the disk.",
        schema: crate::agent::tools::read::listing::list_dir_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::Agent],
        access: Access::Read,
        run: app_params crate::agent::tools::read::listing::execute_list_dir
    },
    "largest_dirs" => {
        desc: "Find the largest subdirectories directly under a path, by recursive size (largest first). Batches directory-size lookups over the index and sorts them. Reports freshness and whether each size is an exact total or a lower bound.",
        schema: crate::agent::tools::read::listing::largest_dirs_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::Agent],
        access: Access::Read,
        run: app_params crate::agent::tools::read::listing::execute_largest_dirs
    },
    "important_folders" => {
        desc: "List the most important folders across scored volumes (top-N, or those at or above a score threshold), highest first. Importance is Cmdr's own offline signal, so it answers even for an unmounted-but-scored drive. Each row carries its volume and score.",
        schema: crate::agent::tools::read::importance::important_folders_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::Agent],
        access: Access::Read,
        run: app_params crate::agent::tools::read::importance::execute_important_folders
    },
    "folder_importance" => {
        desc: "Explain one folder's importance: scored (with its 0-1 score, the signal breakdown, and whether the score is stale relative to the latest scan), floored to zero by design (with the reason), or unscored. Offline-capable.",
        schema: crate::agent::tools::read::importance::folder_importance_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::Agent],
        access: Access::Read,
        run: app_params crate::agent::tools::read::importance::execute_folder_importance
    },
    "list_volumes" => {
        desc: "List every volume Cmdr can see (local disks, SMB shares, MTP devices, and the Network root) with each one's kind, index freshness (fresh / scanning / stale / off), and — for SMB — its connection state (direct / os_mount / disconnected).",
        schema: crate::agent::tools::read::volumes::list_volumes_schema(),
        gate: TokenGate::Open,
        consumers: &[Consumer::Agent],
        access: Access::Read,
        run: app_params crate::agent::tools::read::volumes::execute_list_volumes
    },
}
