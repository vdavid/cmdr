//! Structural tests for the `mcp_tools!` registry table, split by the property each
//! group pins:
//!
//! - `schemas.rs` — the tool set and every tool's declared JSON schema.
//! - `gate.rs` — the bearer-token classification (`tool_gate` / `TokenGate`).
//! - `access.rs` — the consumer and access dimensions: the agent's view, and the no-write gate.
//! - `schema_gate.rs` — `validate_params`, which refuses a call its tool's schema never allowed.
//!
//! They live beside the table rather than inline in `tool_registry/`, so that authored source
//! stays a lean, single-purpose declaration (the `file-length` scanner flags it otherwise), and
//! they drive only the public registry surface, so no `super` access is needed.

mod access;
mod gate;
mod schema_gate;
mod schemas;

use crate::mcp::tools::Tool;

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
    "compress",
    "delete",
    "rename",
    "mkdir",
    "mkfile",
    "refresh",
    "tag",
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
    "indexing",
    "queue",
    "resolve_conflict",
    "unlock_archive",
    "favorites",
    "connect_to_server",
    "remove_manual_server",
    "upgrade_smb_to_direct",
    "eject",
    "await",
    "go_to_latest_download",
    "operations_list",
    "operations_get",
    "operations_rollback",
    "search_photos",
    "image_facts",
    "list_dir",
];
