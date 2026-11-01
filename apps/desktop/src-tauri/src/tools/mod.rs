//! Every AI-callable tool, for both callers: the authored table, its handlers, the sensitive-content
//! gate, and the `cmdr://` resources. `mcp/` carries them to outside clients, and `agent/` runs them
//! in-process.
//!
//! - [`registry`] and `table.rs`: the mechanism and the data. One `mcp_tools!` table generates
//!   every view and the dispatch.
//! - `actions/`: the handlers behind most tools. `read/`, `propose/`, `suggestions/`, `memory.rs`,
//!   `connect.rs`, and `quiet.rs`: the handlers written for the agent.
//! - [`sensitive`]: the one gate every path yielding file contents passes.
//! - [`resources`]: the read-only `cmdr://` views.
//! - [`agent_tool_declarations`]: the `ToolDeclaration`s handed to the provider, built from the
//!   table's `agent_tool_view()`.
//!
//! See `CLAUDE.md` for the must-knows and `DETAILS.md` for the tool catalog.

mod actions;
pub mod connect;
pub mod error_codes;
pub mod memory;
pub mod params;
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
pub use registry::{Access, Consumer, Tool, tool_available_to, validate_params};
pub use table::{agent_tool_view, execute_tool, get_all_tools, tool_access, tool_consumers, tool_schema};

use crate::agent::llm::types::{ToolDeclaration, ToolId};

/// The tool declarations the agent hands the provider, one per `agent_tool_view()`
/// entry. Each name resolves to a typed [`ToolId`] (the 1:1 test pins that every
/// view entry maps to a known variant). Schemas are never `strict: true`
/// ([`ToolDeclaration`] carries no strict flag — spike Gap D).
pub fn agent_tool_declarations() -> Vec<ToolDeclaration> {
    agent_tool_view()
        .into_iter()
        .map(|t| ToolDeclaration {
            name: ToolId::from_wire_name(&t.name),
            description: t.description,
            schema: t.input_schema,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// The typed `ToolId` enum and the registry's agent view are authored in two
    /// places; this pins them 1:1 so no variant is orphaned and no view entry is
    /// unmapped. `Unrecognized` is excluded by design (it's the refusal case).
    #[test]
    fn tool_id_known_maps_one_to_one_onto_agent_view() {
        let view: BTreeSet<String> = agent_tool_view().into_iter().map(|t| t.name).collect();
        let known: BTreeSet<String> = ToolId::KNOWN.iter().map(|t| t.as_wire_name().to_string()).collect();
        assert_eq!(known, view, "ToolId::KNOWN and agent_tool_view() must be exactly 1:1");
        assert!(
            !ToolId::from_wire_name("delete").is_known(),
            "Unrecognized stays out of the known set and the view"
        );
    }

    /// Every agent tool needs a rail label, or its tool line silently falls back to the
    /// generic "Working" / "Used a tool" copy — the transparent tool line is how the user
    /// sees what the agent touched, so a missing label quietly costs transparency rather
    /// than breaking anything. The map is authored in TypeScript, so this reads it as text
    /// (there's no shared wire-name binding to assert against).
    #[test]
    fn every_known_tool_has_an_ask_cmdr_rail_label() {
        let labels_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../src/lib/ask-cmdr/ask-cmdr-labels.ts")
            .canonicalize()
            .expect("ask-cmdr-labels.ts sits at a fixed path relative to src-tauri/");
        let source = std::fs::read_to_string(&labels_path).expect("ask-cmdr-labels.ts is readable");
        // Take the record BODY, not the `Record<string, { … }>` type annotation that
        // precedes it: split past the `= {` opener, then stop at the closing brace in
        // column 0.
        let map = source
            .split_once("const TOOL_LABEL_KEYS")
            .and_then(|(_, rest)| rest.split_once("= {"))
            .and_then(|(_, body)| body.split_once("\n}"))
            .map(|(entries, _)| entries.to_string())
            .expect("TOOL_LABEL_KEYS is a literal record in ask-cmdr-labels.ts");

        let unlabeled: Vec<&str> = ToolId::KNOWN
            .iter()
            .map(|t| t.as_wire_name())
            .filter(|name| !map.contains(&format!("{name}:")))
            .collect();
        assert!(
            unlabeled.is_empty(),
            "these agent tools have no rail label in ask-cmdr-labels.ts, so they'd show the generic \
             fallback copy: {unlabeled:?}"
        );
    }

    /// Every view entry becomes a declaration, and every declaration is a known
    /// tool (no `Unrecognized` leaked into the view). `ToolDeclaration` has no
    /// strict flag, so declarations are never `strict: true` by construction.
    #[test]
    fn declarations_cover_the_view_and_resolve_to_known_tools() {
        let decls = agent_tool_declarations();
        let decl_names: BTreeSet<String> = decls.iter().map(|d| d.name.as_wire_name().to_string()).collect();
        let view: BTreeSet<String> = agent_tool_view().into_iter().map(|t| t.name).collect();
        assert_eq!(decl_names, view);
        assert!(decls.iter().all(|d| d.name.is_known()), "no Unrecognized in the view");
    }
}
