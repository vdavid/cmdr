//! The two view dimensions: the agent's consumer view, and the no-write access gate.

use crate::mcp::tool_registry::{
    Access, Consumer, TokenGate, agent_tool_view, get_all_tools, tool_access, tool_available_to, tool_gate,
};

// ── Consumer + access dimensions (the no-write gate) ──────
//
// One authored registry, two consumer views (agent-spec D49/D59). `consumers` is the exposure
// axis; `access` is a stronger guarantee than `TokenGate::Open` can give (Open covers
// destructive-but-prompting ops). These tests pin the agent view to exactly its authored
// `[agent]` entries AND require every one to be `Access::Read` or `Access::Propose`, never
// `Access::Write`.
//
// The agent can propose; only the user can approve. `Propose` widens what the agent may ASK for,
// never what it may DO — so these tests also pin the hand-authored Propose allowlist and the
// absence of any confirmation bypass in the agent's view. The access axis of the runtime gate is
// exercised per-variant in `agent/tools/view.rs`, since with no Propose tool authored yet the
// registry-level assertions would cover `Propose` vacuously.

/// The exact set of tool names in the agent's read-only view. Pins the set so a stray
/// agent-visible tool (or a dropped one) is a hard failure, mirroring `EXPECTED_TOOL_NAMES`
/// for the ai_client view. `operations_list` / `operations_get` are shared with the
/// ai_client view (`consumers: [AiClient, Agent]`); the rest are agent-only read entries.
const EXPECTED_AGENT_TOOL_NAMES: &[&str] = &[
    "app_state",
    "list_dir",
    "list_pane_files",
    "important_folders",
    "folder_importance",
    "inspect_file",
    "list_volumes",
    "operations_list",
    "operations_get",
    "search_photos",
    "image_facts",
    "propose_rename_plan",
    "list_suggestions",
    "get_suggestion_group",
    "propose_suggestions",
    "nothing_to_suggest",
    "memory_write",
    "memory_edit",
];

/// Set-equality: the agent view equals exactly its authored `consumers:[agent]` entries. This is
/// D59's mechanism — a new destructive tool can't ship agent-visible by accident, because adding
/// it to the view without adding it here fails.
#[test]
fn test_agent_tool_view_is_exactly_expected_set() {
    use std::collections::BTreeSet;
    let actual: BTreeSet<String> = agent_tool_view().into_iter().map(|t| t.name).collect();
    let expected: BTreeSet<String> = EXPECTED_AGENT_TOOL_NAMES.iter().map(|s| (*s).to_owned()).collect();
    assert_eq!(actual, expected, "agent tool view drifted from the expected set");
}

/// The agent's `Propose` tools, authored by hand. A `Propose` tool stages a proposal and opens a
/// review surface; it mutates nothing. No structural check can PROVE a handler doesn't mutate, so
/// this allowlist is the deliberate act: adding a `Propose` tool means a human puts its name here
/// on purpose, having read the handler.
///
/// A `Propose` tool must also cap its payload the way `image_facts` caps at 200 paths — a proposal
/// the user can't review is a proposal they can only rubber-stamp. That contract can't be enforced
/// generically; see `mcp/DETAILS.md` § Consumer and access views.
const EXPECTED_PROPOSE_TOOL_NAMES: &[&str] = &["propose_rename_plan", "propose_suggestions"];

/// The agent's `Memory` tools, authored by hand for the same reason the `Propose` list is.
///
/// ⚠️ **This list is where the app's central agent-safety invariant was deliberately widened.**
/// "Ask Cmdr never changes anything" became "Ask Cmdr writes only into its own memory folder",
/// and the containment is `agent::memory`'s jail rather than this tag. No structural check can
/// prove a handler stays inside that jail, so a human puts each name here on purpose, having
/// read the handler. Tagging an entry `access: Memory` without listing it here fails.
const EXPECTED_MEMORY_TOOL_NAMES: &[&str] = &["memory_write", "memory_edit"];

/// The agent can propose; only the user can approve; and the only thing it writes is its own
/// notes. Structurally: every tool in the agent's view is `Access::Read`, `Access::Propose`, or
/// `Access::Memory`, and NEVER `Access::Write`. This is the guarantee `TokenGate::Open` cannot
/// give — `Open` covers destructive ops that still prompt the user (`copy`/`move`/`delete` with
/// `autoConfirm` absent carry `IfAutoConfirm`), so a gate-based filter would let a `Write` tool
/// into the agent's view. The regression anchor for "the agent still can't touch the user's
/// files, and can now ask and remember".
#[test]
fn test_agent_tool_view_never_writes() {
    for tool in agent_tool_view() {
        let access = tool_access(&tool.name);
        assert!(
            matches!(
                access,
                Some(Access::Read) | Some(Access::Propose) | Some(Access::Memory)
            ),
            "agent-visible tool '{}' is {access:?} — the agent view admits Read, Propose, and Memory, never Write",
            tool.name
        );
    }
}

/// A `Memory` tool only exists if a human authored it into `EXPECTED_MEMORY_TOOL_NAMES`. The
/// twin of `test_propose_tools_are_an_explicit_allowlist`, and the more load-bearing of the
/// two: `Propose` widens what the agent may ASK for, `Memory` widens what it may DO.
#[test]
fn test_memory_tools_are_an_explicit_allowlist() {
    use std::collections::BTreeSet;
    let allowed: BTreeSet<&str> = EXPECTED_MEMORY_TOOL_NAMES.iter().copied().collect();
    let actual: BTreeSet<String> = agent_tool_view()
        .into_iter()
        .filter(|t| tool_access(&t.name) == Some(Access::Memory))
        .map(|t| t.name)
        .collect();
    let actual_refs: BTreeSet<&str> = actual.iter().map(String::as_str).collect();
    assert_eq!(
        actual_refs, allowed,
        "the registry's Memory tools differ from the hand-authored allowlist"
    );
}

/// ⚠️ `Access::Memory` is the agent's ONLY write, and it belongs to the in-process agent alone.
/// An external MCP client reaching one would be a filesystem write through a transport whose
/// whole security story is "agents do only what users can do, no filesystem access".
#[test]
fn test_memory_tools_are_not_exposed_to_external_mcp_clients() {
    for name in EXPECTED_MEMORY_TOOL_NAMES {
        assert!(
            !tool_available_to(name, Consumer::AiClient),
            "'{name}' writes to disk and must not be reachable over the HTTP transport"
        );
    }
}

/// A `Propose` tool only exists if a human authored it into `EXPECTED_PROPOSE_TOOL_NAMES`. Tagging
/// an entry `access: Propose` without listing it here fails: `Propose` is a widened power, so it
/// can't be acquired as a side effect of editing a registry line.
#[test]
fn test_propose_tools_are_an_explicit_allowlist() {
    use std::collections::BTreeSet;
    let allowed: BTreeSet<&str> = EXPECTED_PROPOSE_TOOL_NAMES.iter().copied().collect();
    let actual: BTreeSet<String> = agent_tool_view()
        .into_iter()
        .filter(|t| tool_access(&t.name) == Some(Access::Propose))
        .map(|t| t.name)
        .collect();
    let actual_refs: BTreeSet<&str> = actual.iter().map(String::as_str).collect();
    assert_eq!(
        actual_refs, allowed,
        "the registry's Propose tools differ from the hand-authored allowlist"
    );
}

/// No proposal path inherits the confirmation bypass. `autoConfirm` (and the `queue` tool's
/// `rollback`, and `dialog`'s `action: "confirm"`) let a token-holding MCP client skip the user's
/// confirmation dialog — exactly the approval a proposal must never grant itself. So every tool in
/// the agent's view carries `TokenGate::Open` (it has no bypass to gate) AND declares no bypass
/// parameter in its schema, which is what makes "only the user can approve" true rather than
/// merely intended.
#[test]
fn test_no_agent_tool_reaches_the_confirmation_bypass() {
    let view = agent_tool_view();
    assert!(!view.is_empty(), "an empty agent view would make this vacuous");
    for tool in &view {
        assert_eq!(
            tool_gate(&tool.name),
            Some(TokenGate::Open),
            "agent-visible tool '{}' carries a non-Open gate — the agent view must contain no bypassable tool",
            tool.name
        );
        let properties = tool.input_schema.get("properties");
        for bypass in ["autoConfirm", "rollback"] {
            assert!(
                properties.and_then(|p| p.get(bypass)).is_none(),
                "agent-visible tool '{}' declares a '{bypass}' parameter — a proposal must never carry the confirmation bypass",
                tool.name
            );
        }
    }
}

/// Consumer-identity dispatch: the dispatch view each consumer can reach through `execute_tool`
/// equals exactly its list view — no transport dispatches a name outside its consumer view
/// ("callable but not listed" is the drift D59 exists to prevent). With an empty agent view this
/// proves every ai_client tool is refused to the agent runtime; with entries shared into both
/// views it enforces that no transport dispatches a name outside its consumer view.
#[test]
fn test_dispatch_view_equals_list_view_per_consumer() {
    use std::collections::BTreeSet;
    let ai_names: BTreeSet<String> = get_all_tools().into_iter().map(|t| t.name).collect();
    let agent_names: BTreeSet<String> = agent_tool_view().into_iter().map(|t| t.name).collect();

    for name in ai_names.iter().chain(agent_names.iter()) {
        assert_eq!(
            tool_available_to(name, Consumer::AiClient),
            ai_names.contains(name),
            "ai_client dispatchability of '{name}' disagrees with the ai_client list view"
        );
        assert_eq!(
            tool_available_to(name, Consumer::Agent),
            agent_names.contains(name),
            "agent dispatchability of '{name}' disagrees with the agent list view"
        );
    }

    // An unknown name is refused by both transports (typed refusal, not a string branch).
    assert!(!tool_available_to("bogus", Consumer::AiClient));
    assert!(!tool_available_to("bogus", Consumer::Agent));
}

/// Every ai_client tool declares an access class, and the current registry is entirely ai_client.
/// A future entry shared into `[ai_client, agent]` still needs a correct `access` (the all-Read
/// test above enforces `Read` for its agent side).
#[test]
fn test_every_tool_has_an_access_class() {
    for tool in get_all_tools() {
        assert!(
            tool_access(&tool.name).is_some(),
            "tool '{}' has no access class",
            tool.name
        );
    }
}
