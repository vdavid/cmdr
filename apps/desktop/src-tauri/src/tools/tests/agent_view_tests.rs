use std::collections::BTreeSet;

use crate::agent::llm::types::ToolId;
use crate::tools::{agent_tool_declarations, agent_tool_view, get_all_tools};

/// `ToolId` is generated from the table, so `KNOWN` is every entry, whichever caller it's listed
/// for. `Unrecognized` is excluded by design (it's the refusal case).
#[test]
fn tool_id_known_is_every_table_entry() {
    let table: BTreeSet<String> = get_all_tools()
        .into_iter()
        .chain(agent_tool_view())
        .map(|t| t.name)
        .collect();
    let known: BTreeSet<String> = ToolId::KNOWN.iter().map(|t| t.as_wire_name().to_string()).collect();
    assert_eq!(known, table);
    assert_eq!(known.len(), ToolId::KNOWN.len(), "two entries share a wire name");
    for tool in ToolId::KNOWN {
        assert_eq!(ToolId::from_wire_name(tool.as_wire_name()), tool);
    }
}

#[test]
fn a_table_name_is_known_and_anything_else_stays_unrecognized() {
    assert_eq!(ToolId::from_wire_name("delete"), ToolId::Delete);
    assert!(ToolId::from_wire_name("delete").is_known());
    let made_up = ToolId::from_wire_name("definitely_not_a_tool");
    assert_eq!(made_up, ToolId::Unrecognized("definitely_not_a_tool".into()));
    assert!(!made_up.is_known());
    assert_eq!(made_up.as_wire_name(), "definitely_not_a_tool");
    assert_eq!(made_up.subject_spec(), None);
}

/// An old thread's stored call is a bare wire name. One that named a tool the agent couldn't run,
/// which reloaded as `Unrecognized`, reloads as that tool now, and writes back the same bytes.
#[test]
fn a_stored_name_reloads_as_known_and_round_trips_unchanged() {
    let stored = "\"delete\"";
    let reloaded: ToolId = serde_json::from_str(stored).expect("a bare string");
    assert_eq!(reloaded, ToolId::Delete);
    assert_eq!(serde_json::to_string(&reloaded).expect("a string"), stored);

    let stored = "\"format_drive\"";
    let reloaded: ToolId = serde_json::from_str(stored).expect("a bare string");
    assert_eq!(reloaded, ToolId::Unrecognized("format_drive".into()));
    assert_eq!(serde_json::to_string(&reloaded).expect("a string"), stored);
}

/// The one line a model reads for a tool it hasn't loaded in full.
#[test]
fn every_tool_has_a_brief_of_at_most_100_characters_on_one_line() {
    for tool in ToolId::KNOWN {
        let brief = tool.brief().expect("a table entry has a brief");
        let name = tool.as_wire_name();
        assert!(!brief.trim().is_empty(), "{name} has an empty brief");
        assert!(brief.chars().count() <= 100, "{name}'s brief is too long: {brief}");
        assert!(!brief.contains('\n'), "{name}'s brief spans lines");
    }
    assert_eq!(ToolId::Unrecognized("format_drive".into()).brief(), None);
}

/// Every tool in the table needs a rail label, or its tool line silently falls back to the
/// generic "Working" / "Used a tool" copy — the transparent tool line is how the user
/// sees what the agent touched, so a missing label quietly costs transparency rather
/// than breaking anything. The map is authored in TypeScript, so this reads it as text
/// (there's no shared wire-name binding to assert against).
#[test]
fn every_known_tool_has_an_ask_cmdr_rail_label() {
    let labels_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../src/lib/ask-cmdr/ask-cmdr-tool-label-keys.ts")
        .canonicalize()
        .expect("ask-cmdr-tool-label-keys.ts sits at a fixed path relative to src-tauri/");
    let source = std::fs::read_to_string(&labels_path).expect("ask-cmdr-tool-label-keys.ts is readable");
    // Take the record BODY, not the `Record<string, { … }>` type annotation that
    // precedes it: split past the `= {` opener, then stop at the closing brace in
    // column 0.
    let map = source
        .split_once("const TOOL_LABEL_KEYS")
        .and_then(|(_, rest)| rest.split_once("= {"))
        .and_then(|(_, body)| body.split_once("\n}"))
        .map(|(entries, _)| entries.to_string())
        .expect("TOOL_LABEL_KEYS is a literal record in ask-cmdr-tool-label-keys.ts");

    let unlabeled: Vec<&str> = ToolId::KNOWN
        .iter()
        .map(|t| t.as_wire_name())
        .filter(|name| !map.contains(&format!("\n  {name}:")))
        .collect();
    assert!(
        unlabeled.is_empty(),
        "these tools have no rail label in ask-cmdr-tool-label-keys.ts, so they'd show the generic \
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
