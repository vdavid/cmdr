//! The Ask Cmdr agent's in-process read-only tool layer.
//!
//! One authored registry, two consumer views (agent-spec D49): the agent's tools
//! are `consumers: [Agent], access: Read` entries in the shared `mcp_tools!` table,
//! and this module is where their handlers, schemas, and typed result shapes live
//! ([`read`]) and where the runtime reaches them:
//!
//! - [`agent_tool_declarations`]: the `ToolDeclaration`s handed to the provider,
//!   built from the registry's `agent_tool_view()`.
//! - [`view::dispatch`] / [`view::refuse_unavailable`]: the gated dispatch — the
//!   read-only choke point (an unknown/write name is refused before `execute_tool`).
//!
//! See `CLAUDE.md` for the must-knows (reuse the core; the honesty contract; the
//! Unrecognized-out-of-view invariant) and `DETAILS.md` for the tool catalog.

pub mod propose;
pub mod read;
pub mod view;

use crate::agent::llm::types::{ToolDeclaration, ToolId};
use crate::mcp::agent_tool_view;

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
