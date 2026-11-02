//! Every AI-callable tool, for both callers: the authored table, its handlers, the sensitive-content
//! gate, and the `cmdr://` resources. `mcp/` carries them to outside clients, and `agent/` runs them
//! in-process.
//!
//! - [`registry`] and `table.rs`: the mechanism and the data. One `mcp_tools!` table generates
//!   every view and the dispatch.
//! - `actions/`: the handlers behind most tools. `read/`, `propose/`, `suggestions/`, `memory.rs`,
//!   `connect.rs`, and `quiet.rs`: the handlers written for the agent.
//! - [`policy`]: [`policy::admit`], the one decision over who may run what; who's calling
//!   ([`ToolCtx`]); and direct mode, the switch that lets an outside client past a person.
//! - [`sensitive`]: the one gate every path yielding file contents passes.
//! - [`resources`]: the read-only `cmdr://` views.
//! - [`agent_tool_declarations`]: the `ToolDeclaration`s handed to the provider, [`deferred_tool_lines`]:
//!   the one line each for the rest, and [`meta`]: the pair that reaches them.
//!
//! See `CLAUDE.md` for the must-knows and `DETAILS.md` for the tool catalog.

mod actions;
pub mod ask;
mod ask_plans;
pub mod connect;
pub mod error_codes;
pub mod memory;
pub mod meta;
pub mod params;
pub mod policy;
pub mod propose;
pub mod quiet;
pub mod read;
pub mod resources;
mod schemas;
pub mod sensitive;
mod subject;
pub mod suggestions;

// `table.rs` expands the `mcp_tools!` macro that `registry` defines, so `registry` comes first.
#[macro_use]
mod registry;
mod table;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod transcript_179_tests;

// The tool result types every handler returns, and the size fitting they share.
pub(crate) use actions::{ToolError, ToolResult, fit_to_result_budget, is_virtual_path};

// "Go there and point at that" is this module's frontend protocol (`mcp-tab` +
// `mcp-nav-to-path` + `mcp-move-cursor` + `mcp-select-names`), but it isn't only an
// agent's move: an OS reveal and the `cmdr` terminal command (`crate::reveal`,
// `crate::cli`) make exactly the same one. Exported as a named seam rather than by
// opening `actions` up, so the rest of the crate reaches this and nothing else.
//
// Gated because `crate::reveal` and `crate::cli` are the seam's only users and are
// themselves macOS-only: the in-module caller (`actions::downloads`) reaches `nav`
// directly, so off macOS this re-export has nobody, and `-D unused` is an error. CI lints
// on Linux and we lint on macOS, so a missing gate here compiles clean locally and breaks
// the whole build there.
#[cfg(target_os = "macos")]
pub(crate) use actions::nav::{GoToTarget, PaneChoice, go_to_in_pane, open_tab_in_pane};
pub(crate) use policy::{Caller, ToolCtx};
pub(crate) use registry::{Access, Load, Tool};
pub(crate) use subject::SubjectSpec;
pub use table::ToolId;
use table::table_tools;
#[cfg(test)]
use table::tool_schema;
pub(crate) use table::{execute_tool, get_all_tools, tool_access, tool_load, validate_params};

use crate::agent::llm::types::ToolDeclaration;
use policy::{Verdict, admit};

/// Whether the agent can run this tool on a turn someone asked for. Background turns hear
/// `refusedInBackground` from the gate rather than seeing a shorter list.
fn agent_reaches(name: &str) -> bool {
    !tool_access(name).is_some_and(|access| matches!(admit(meta::user_started_agent(), access), Verdict::Refuse(_)))
}

/// The tool declarations the agent hands the provider: the core plus the tool-search pair, whatever
/// turn this call serves. A user turn and a follow-up can share a thread, and these bytes are the
/// provider's cache key. Schemas are never `strict: true` ([`ToolDeclaration`] carries no strict flag:
/// spike Gap D).
pub fn agent_tool_declarations() -> Vec<ToolDeclaration> {
    table_tools()
        .into_iter()
        .filter(|tool| agent_reaches(&tool.name) && tool_load(&tool.name) != Some(Load::Deferred))
        .map(|tool| ToolDeclaration {
            name: ToolId::from_wire_name(&tool.name),
            description: tool.description,
            schema: tool.input_schema,
        })
        .collect()
}

/// One line per tool the agent can reach but isn't declared, in table order: what `{{more_tools}}` in
/// the system prompt says. Part of the cached prefix, so it reads nothing per-call: no origin, no
/// direct mode, no settings.
pub fn deferred_tool_lines() -> String {
    ToolId::KNOWN
        .iter()
        .filter(|tool| {
            let name = tool.as_wire_name();
            agent_reaches(name) && tool_load(name) == Some(Load::Deferred)
        })
        .map(|tool| format!("- {}: {}", tool.as_wire_name(), tool.brief().unwrap_or_default()))
        .collect::<Vec<_>>()
        .join("\n")
}
