//! The two view dimensions: the agent's consumer view, and the no-write access gate.

use crate::tools::{Access, Consumer, agent_tool_view, get_all_tools, tool_access, tool_available_to};

// ── Consumer + access dimensions (the no-write gate) ──────
//
// One authored registry, two consumer views (agent decisions D49/D59). `consumers` is the exposure
// axis; `access` is the capability axis. These tests pin the agent view to exactly its authored
// `[agent]` entries AND require every one to be `Access::Read`, `Access::Propose`, `Access::Memory`,
// or `Access::Session`, never `Access::Write`.
//
// The agent can propose; only the user can approve. `Propose` widens what the agent may ASK for,
// never what it may DO — so these tests also pin the hand-authored Propose allowlist and the
// absence of any confirmation bypass in the agent's view. The access axis of the runtime gate is
// exercised per-variant in `agent/tools/view.rs`, since with no Propose tool authored yet the
// registry-level assertions would cover `Propose` vacuously.

/// The exact set of tool names in the agent's read-only view. Pins the set so a stray
/// agent-visible tool (or a dropped one) is a hard failure, mirroring `EXPECTED_TOOL_NAMES`
/// for the ai_client view. `operations_list` / `operations_get` / `search` / `search_photos` /
/// `image_facts` are shared with the ai_client view (`consumers: [AiClient, Agent]`); the rest are
/// agent-only read entries. `ai_search` is deliberately absent: Ask Cmdr is already an LLM holding
/// the user's prose, so it writes the structured query itself instead of paying a second model
/// call to translate it.
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
    "search",
    "search_photos",
    "image_facts",
    "propose_rename_plan",
    "list_suggestions",
    "get_suggestion_group",
    "propose_suggestions",
    "propose_server",
    "nothing_to_suggest",
    "memory_write",
    "memory_edit",
    "connect_volume",
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
/// generically; see `apps/desktop/src-tauri/src/tools/DETAILS.md` § Consumer and access views.
///
/// `multi_rename` is the one external-client entry: it stages its ready rows as a rename group
/// on the same spine, and its handler (`executor/multi_rename.rs`) only reads the pane's cached
/// listing and writes the proposal rows.
///
/// `propose_server` (`tools/suggestions/server.rs`) stages one zero-op server group: it
/// reads the saved-server stores in memory, writes the proposal rows, takes no secret, contacts no
/// server, and refuses on a turn the user didn't start. The user's click is what saves.
const EXPECTED_PROPOSE_TOOL_NAMES: &[&str] = &[
    "multi_rename",
    "propose_rename_plan",
    "propose_suggestions",
    "propose_server",
];

/// The agent's `Memory` tools, authored by hand for the same reason the `Propose` list is.
///
/// ⚠️ **This list is where the app's central agent-safety invariant was deliberately widened.**
/// "Ask Cmdr never changes anything" became "Ask Cmdr writes only into its own memory folder",
/// and the containment is `agent::memory`'s jail rather than this tag. No structural check can
/// prove a handler stays inside that jail, so a human puts each name here on purpose, having
/// read the handler. Tagging an entry `access: Memory` without listing it here fails.
const EXPECTED_MEMORY_TOOL_NAMES: &[&str] = &["memory_write", "memory_edit"];

/// The `Session` tools, authored by hand: each opens a session to a place the user saved, with
/// credentials the user stored, and only on a turn the user started (agent invariant 7).
///
/// ⚠️ **The third widening.** "Ask Cmdr changes nothing outside its memory folder" gains "and may
/// connect a place you saved". No structural check can prove a handler dials only a saved place
/// and edits no server, credential, pin, or setting, so a human puts each name here on purpose,
/// having read the handler.
///
/// `connect_volume` (`tools/connect.rs`): dials only a saved, unregistered place through
/// `saved_servers::connect_saved_place` with no secret and no account, hands a dropped one to the
/// frontend's reconnect manager, and takes no address, secret, or account parameter.
const EXPECTED_SESSION_TOOL_NAMES: &[&str] = &["connect_volume"];

/// Every registry entry in either view, with its access class.
fn shipped_tools() -> Vec<(String, Option<Access>)> {
    agent_tool_view()
        .into_iter()
        .chain(get_all_tools())
        .map(|tool| {
            let access = tool_access(&tool.name);
            (tool.name, access)
        })
        .collect()
}

/// Whether the entries classified `access` are exactly `allowlist`. Over a plain list, so the
/// check is testable against an entry the shipped registry doesn't carry.
fn matches_allowlist(tools: &[(String, Option<Access>)], access: Access, allowlist: &[&str]) -> bool {
    use std::collections::BTreeSet;
    let actual: BTreeSet<&str> = tools
        .iter()
        .filter(|(_, class)| *class == Some(access))
        .map(|(name, _)| name.as_str())
        .collect();
    actual == allowlist.iter().copied().collect()
}

/// The agent can propose; only the user can approve; it writes only its own notes; and it
/// connects only places the user saved. Structurally: every tool in the agent's view is
/// `Access::Read`, `Access::Propose`, `Access::Memory`, or `Access::Session`, and NEVER
/// `Access::Write`. The regression anchor for "the agent still can't touch the user's files".
#[test]
fn test_agent_tool_view_never_writes() {
    for tool in agent_tool_view() {
        let access = tool_access(&tool.name);
        assert!(
            matches!(
                access,
                Some(Access::Read) | Some(Access::Propose) | Some(Access::Memory) | Some(Access::Session)
            ),
            "agent-visible tool '{}' is {access:?} — the agent view admits Read, Propose, Memory, and Session, never Write",
            tool.name
        );
    }
}

/// A `Session` tool only exists if a human authored it into `EXPECTED_SESSION_TOOL_NAMES`, in
/// either view.
#[test]
fn test_session_tools_are_an_explicit_allowlist() {
    assert!(
        matches_allowlist(&shipped_tools(), Access::Session, EXPECTED_SESSION_TOOL_NAMES),
        "the registry's Session tools differ from the hand-authored allowlist"
    );
}

/// The allowlist check has teeth: a `Session` entry nobody listed fails it. A test-only entry,
/// never in the shipped registry.
#[test]
fn test_an_unlisted_session_tool_fails_the_allowlist() {
    let mut tools = shipped_tools();
    tools.push(("connect_anything".to_owned(), Some(Access::Session)));
    assert!(
        !matches_allowlist(&tools, Access::Session, EXPECTED_SESSION_TOOL_NAMES),
        "an unlisted Session entry must fail the allowlist"
    );
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
    // Both views: an external client's `Propose` tool is the same widening as the agent's.
    let actual: BTreeSet<String> = agent_tool_view()
        .into_iter()
        .chain(get_all_tools())
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
/// `rollback`) lets an external MCP client skip the user's confirmation dialog — exactly the
/// approval a proposal must never grant itself. Every agent tool therefore declares no bypass
/// parameter in its schema, which is what makes "only the user can approve" true rather than
/// merely intended.
#[test]
fn test_no_agent_tool_reaches_the_confirmation_bypass() {
    let view = agent_tool_view();
    assert!(!view.is_empty(), "an empty agent view would make this vacuous");
    for tool in &view {
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
