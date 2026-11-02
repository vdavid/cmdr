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
//! - [`agent_tool_declarations`]: the `ToolDeclaration`s handed to the provider.
//!
//! See `CLAUDE.md` for the must-knows and `DETAILS.md` for the tool catalog.

mod actions;
pub mod ask;
mod ask_plans;
pub mod connect;
pub mod error_codes;
pub mod memory;
pub mod params;
pub mod policy;
pub mod propose;
pub mod quiet;
pub mod read;
pub mod resources;
mod schemas;
pub mod sensitive;
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
pub(crate) use actions::nav::{GoToTarget, PaneChoice, go_to_in_pane};
pub(crate) use policy::{Caller, ToolCtx};
#[cfg(test)]
pub(crate) use registry::Load;
pub(crate) use registry::{Access, SubjectSpec, Tool, validate_params};
pub use table::ToolId;
#[cfg(test)]
pub(crate) use table::tool_load;
use table::tool_schema;
pub(crate) use table::{execute_tool, get_all_tools, tool_access};

use crate::agent::llm::types::ToolDeclaration;
use crate::agent::types::TurnOrigin;
use policy::{Verdict, admit};

/// The tool declarations the agent hands the provider: every tool [`admit`] doesn't refuse on a turn
/// the user started, whatever turn this call serves. A user turn and a follow-up can share a thread,
/// and these bytes are the provider's cache key, so a background turn hears `refusedInBackground`
/// from the gate instead of seeing a shorter list. Schemas are never `strict: true`
/// ([`ToolDeclaration`] carries no strict flag: spike Gap D).
pub fn agent_tool_declarations() -> Vec<ToolDeclaration> {
    let agent = Caller::Agent {
        origin: TurnOrigin::Text,
    };
    get_all_tools()
        .into_iter()
        .filter(|t| !tool_access(&t.name).is_some_and(|access| matches!(admit(agent, access), Verdict::Refuse(_))))
        .map(|t| ToolDeclaration {
            name: ToolId::from_wire_name(&t.name),
            description: t.description,
            schema: t.input_schema,
        })
        .collect()
}
