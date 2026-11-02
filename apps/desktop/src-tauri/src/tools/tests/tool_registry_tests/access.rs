//! One table, one axis: every tool's class, and what `admit` makes of it across the table. The matrix
//! itself, cell by cell, is in `tools/policy.rs`; the fence per origin is in `agent/tool_gate.rs`.
//!
//! **The agent can start anything. A person finishes anything sensitive.** No structural check can
//! prove what a handler does, so each class that changes something is authored by hand below: adding
//! a name is a deliberate act by someone who read the handler.

use std::collections::BTreeSet;

use crate::agent::types::TurnOrigin;
use crate::tools::policy::{Caller, Refusal, Verdict, admit};
use crate::tools::{Access, Load, ToolId, get_all_tools, tool_access, tool_load};

const USER_TURN: Caller = Caller::Agent {
    origin: TurnOrigin::Text,
};
const DIRECT: Caller = Caller::External { direct: true };
const OFF_DIRECT: Caller = Caller::External { direct: false };

/// A `Propose` tool stages a proposal and opens a review surface; it mutates nothing. A `Propose` tool
/// must also cap its payload the way `image_facts` caps at 200 paths: a proposal the user can't review
/// is a proposal they can only rubber-stamp. That contract can't be enforced generically; see
/// `apps/desktop/src-tauri/src/tools/DETAILS.md` § The rule.
///
/// `multi_rename` stages its ready rows as a rename group on the same spine, and its handler
/// (`actions/multi_rename.rs`) only reads the pane's cached listing and writes the proposal rows.
///
/// `propose_server` (`tools/suggestions/server.rs`) stages one zero-op server group: it reads the
/// saved-server stores in memory, writes the proposal rows, takes no secret, contacts no server, and
/// refuses on a turn the user didn't start. The user's click is what saves.
const EXPECTED_PROPOSE_TOOL_NAMES: &[&str] = &[
    "multi_rename",
    "propose_rename_plan",
    "propose_suggestions",
    "propose_server",
];

/// Writes the agent's own memory folder. The containment is `agent::memory`'s jail rather than this
/// tag, so a human puts each name here on purpose, having read the handler.
const EXPECTED_MEMORY_TOOL_NAMES: &[&str] = &["memory_write", "memory_edit"];

/// Each opens a session to a place the user saved, with credentials the user stored.
///
/// `connect_volume` (`tools/connect.rs`): dials only a saved, unregistered place through
/// `saved_servers::connect_saved_place` with no secret and no account, hands a dropped one to the
/// frontend's reconnect manager, and takes no address, secret, or account parameter.
///
/// `upgrade_smb_to_direct` (`tools/actions/async_tools.rs`): moves a share that's already mounted
/// to a direct session, with credentials the user stored.
const EXPECTED_SESSION_TOOL_NAMES: &[&str] = &["connect_volume", "upgrade_smb_to_direct"];

/// Tools that read inside files, or image-derived text. Hand-authored, because this class is what
/// keeps an outside client from reading through Cmdr's Full Disk Access (direct mode only), and a
/// content tool classified `Read` would slip past that with no other signal.
const EXPECTED_READ_CONTENTS_TOOL_NAMES: &[&str] = &["inspect_file", "image_facts", "search_photos"];

/// Tools that open a dialog or editor a person confirms. Hand-authored, because a handler that
/// starts the operation anyway would quietly break the rule.
const EXPECTED_INITIATE_TOOL_NAMES: &[&str] = &["copy", "move", "compress", "delete", "rename", "operations_rollback"];

/// The ONE written exception to "nothing writes without a person". David accepted it, including
/// that `tag` has no undo, so a new name here is a new decision of his.
const EXPECTED_LIGHT_WRITE_TOOL_NAMES: &[&str] = &["tag", "mkdir", "mkfile"];

/// Tools that run only after a person confirms an Ask.
const EXPECTED_SENSITIVE_TOOL_NAMES: &[&str] = &[
    "open_under_cursor",
    "set_setting",
    "quit",
    "eject",
    "remove_manual_server",
    "indexing",
];

/// Tools that go past a person, or would. Each entry's reason is the one line in the table.
const EXPECTED_DEV_ONLY_TOOL_NAMES: &[&str] = &[
    "ai_search",
    "connect_to_server",
    "dialog_confirm",
    "queue_move_locked",
    "resolve_conflict",
];

/// The name of every table entry with this class.
fn names_of(matches_class: impl Fn(Access) -> bool) -> BTreeSet<String> {
    ToolId::KNOWN
        .iter()
        .map(|tool| tool.as_wire_name())
        .filter(|name| tool_access(name).is_some_and(&matches_class))
        .map(str::to_owned)
        .collect()
}

fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

/// Each hand-authored class equals the table's. Tagging an entry without listing it here fails, so a
/// power can't be acquired as a side effect of editing a table line.
#[test]
fn each_hand_authored_class_is_exactly_its_allowlist() {
    for (class, allowlist) in [
        (Access::ReadContents, EXPECTED_READ_CONTENTS_TOOL_NAMES),
        (Access::Propose, EXPECTED_PROPOSE_TOOL_NAMES),
        (Access::Memory, EXPECTED_MEMORY_TOOL_NAMES),
        (Access::Session, EXPECTED_SESSION_TOOL_NAMES),
        (Access::Initiate, EXPECTED_INITIATE_TOOL_NAMES),
        (Access::LightWrite, EXPECTED_LIGHT_WRITE_TOOL_NAMES),
        (Access::Sensitive, EXPECTED_SENSITIVE_TOOL_NAMES),
    ] {
        assert_eq!(
            names_of(|access| access == class),
            set(allowlist),
            "{class:?} drifted from its allowlist"
        );
    }
    assert_eq!(
        names_of(|access| matches!(access, Access::DevOnly(_))),
        set(EXPECTED_DEV_ONLY_TOOL_NAMES),
        "DevOnly drifted from its allowlist"
    );
}

/// No hidden splits: a tool is reachable by both callers, or its table entry says why not.
#[test]
fn every_tool_is_reachable_by_both_callers_or_carries_a_reason() {
    for tool in ToolId::KNOWN {
        let name = tool.as_wire_name();
        let access = tool_access(name).expect("a table entry has a class");
        let load = tool_load(name).expect("a table entry has a load");
        let agent_reaches = !matches!(admit(USER_TURN, access), Verdict::Refuse(_));
        let client_reaches = !matches!(admit(DIRECT, access), Verdict::Refuse(_));
        match (access, load) {
            (Access::DevOnly(reason), _) => {
                assert!(!reason.trim().is_empty(), "'{name}' gives no reason");
                assert!(!reason.contains('\n'), "'{name}' takes more than one line");
                assert!(!agent_reaches, "'{name}' is DevOnly, so never the agent's");
                assert!(client_reaches, "'{name}' is for direct mode");
            }
            (_, Load::Meta(reason)) => {
                assert!(!reason.trim().is_empty(), "'{name}' gives no reason");
                assert!(agent_reaches, "'{name}' is the agent's");
            }
            _ => assert!(agent_reaches && client_reaches, "'{name}' has a hidden split"),
        }
    }
}

/// Every listed tool dispatches, and every non-`Meta` entry is listed: "callable but not listed" is the
/// drift one table exists to prevent.
#[test]
fn an_outside_client_lists_the_whole_table_but_meta() {
    let listed: BTreeSet<String> = get_all_tools().into_iter().map(|tool| tool.name).collect();
    let table: BTreeSet<String> = ToolId::KNOWN
        .iter()
        .map(|tool| tool.as_wire_name())
        .filter(|name| !matches!(tool_load(name), Some(Load::Meta(_))))
        .map(str::to_owned)
        .collect();
    assert_eq!(listed, table);
}

/// The contents class reads as `Read` for the agent, on every origin, and off direct mode an outside
/// client refuses naming the setting.
#[test]
fn contents_are_the_agents_on_every_turn_and_a_clients_only_in_direct_mode() {
    for name in EXPECTED_READ_CONTENTS_TOOL_NAMES {
        let access = tool_access(name).expect("listed");
        for origin in [
            TurnOrigin::Text,
            TurnOrigin::Resume,
            TurnOrigin::Wake,
            TurnOrigin::Outcomes,
        ] {
            assert_eq!(
                admit(Caller::Agent { origin }, access),
                Verdict::Allow,
                "{name} on {origin:?}"
            );
        }
        assert_eq!(admit(DIRECT, access), Verdict::Allow, "{name}");
        assert_eq!(
            admit(OFF_DIRECT, access),
            Verdict::Refuse(Refusal::DirectModeOff),
            "{name}"
        );
    }
}

/// A client reaching memory off direct mode would be a filesystem write nobody turned on.
#[test]
fn memory_needs_direct_mode_for_an_outside_client() {
    for name in EXPECTED_MEMORY_TOOL_NAMES {
        let access = tool_access(name).expect("listed");
        assert_eq!(
            admit(OFF_DIRECT, access),
            Verdict::Refuse(Refusal::DirectModeOff),
            "{name}"
        );
        assert_eq!(admit(DIRECT, access), Verdict::Allow, "{name}");
    }
}

/// `dialog_confirm` is the one way past a person, so nothing else may declare a way: no schema carries a
/// bypass parameter. And the agent can't call the one that exists, or see it.
#[test]
fn no_schema_has_a_way_past_a_person_and_the_agent_never_gets_the_one_there_is() {
    for tool in get_all_tools() {
        let properties = tool.input_schema.get("properties");
        for bypass in ["autoConfirm", "rollback"] {
            assert!(
                properties.and_then(|p| p.get(bypass)).is_none(),
                "'{}' declares a '{bypass}' parameter",
                tool.name
            );
        }
    }
    let declared: BTreeSet<String> = crate::tools::agent_tool_declarations()
        .into_iter()
        .map(|d| d.name.as_wire_name().to_owned())
        .collect();
    for name in EXPECTED_DEV_ONLY_TOOL_NAMES {
        assert!(!declared.contains(*name), "'{name}' is declared to the agent");
    }
}

/// The agent's declarations are what a user-started turn can reach, ALL of them, whatever origin the
/// call serves: the prefix is byte-identical across a thread, so nothing per-origin can move it.
#[test]
fn the_agent_is_declared_every_tool_it_can_reach_on_a_turn_it_was_asked() {
    let declared: BTreeSet<String> = crate::tools::agent_tool_declarations()
        .into_iter()
        .map(|d| d.name.as_wire_name().to_owned())
        .collect();
    let reachable: BTreeSet<String> = ToolId::KNOWN
        .iter()
        .map(|tool| tool.as_wire_name())
        .filter(|name| !matches!(admit(USER_TURN, tool_access(name).expect("listed")), Verdict::Refuse(_)))
        .map(str::to_owned)
        .collect();
    assert_eq!(declared, reachable);
}

/// An outside client counts as a turn a person started, so `propose_server` runs for it: a store this
/// app doesn't have is what answers. A wake's refusal would have been an `Ok` with no store touched.
#[tokio::test]
async fn propose_server_treats_an_outside_client_as_a_turn_a_person_started() {
    let app = tauri::test::mock_app();
    let ctx = crate::tools::ToolCtx {
        caller: OFF_DIRECT,
        cancel: tokio_util::sync::CancellationToken::new(),
        conversation_id: None,
        asked: None,
    };
    let answer = crate::tools::execute_tool(
        app.handle(),
        &ctx,
        "propose_server",
        &serde_json::json!({ "protocol": "smb" }),
    )
    .await;
    assert!(answer.is_err(), "{answer:?}");
}

/// What `tools/list` tells a client about a tool is its class, in MCP's words.
#[test]
fn every_listed_tool_carries_its_class_as_annotations() {
    for tool in get_all_tools() {
        let access = tool_access(&tool.name).expect("listed");
        assert_eq!(tool.annotations, access.hints(), "{}", tool.name);
    }
}

/// The core is what every call pays for in full, and what an everyday flow can't fumble through
/// indirection. Lower the cap if you can.
#[test]
fn the_core_stays_small_and_holds_what_the_everyday_flows_name() {
    let core = names_of_load(Load::Core);
    assert_eq!(core.len(), 19, "{core:?}");
    for name in [
        "list_pane_files",
        "image_facts",
        "propose_rename_plan",
        "memory_write",
        "nothing_to_suggest",
        "propose_suggestions",
        "nav_to_path",
        "select",
        "copy",
        "move",
        "delete",
        "rename",
        "mkdir",
        "dialog",
    ] {
        assert!(core.contains(name), "'{name}' is named by a flow, so it's core");
    }
}

fn names_of_load(load: Load) -> BTreeSet<String> {
    ToolId::KNOWN
        .iter()
        .map(|tool| tool.as_wire_name())
        .filter(|name| tool_load(name) == Some(load))
        .map(str::to_owned)
        .collect()
}
