use std::collections::BTreeSet;

use crate::agent::llm::types::ToolId;
use crate::tools::{agent_tool_declarations, agent_tool_view};

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
