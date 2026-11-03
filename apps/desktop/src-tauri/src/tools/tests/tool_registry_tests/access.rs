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

/// The ONE written exception to "nothing writes without a person". David accepted it (`tag` is
/// undone through `operations_rollback`, which a person confirms), so a new name here is a new
/// decision of his.
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

/// The agent is told about everything a user-started turn can reach, whatever origin the call serves:
/// the core and the pair in full, the rest in one line each, nothing twice. The prefix is byte-identical
/// across a thread, so nothing per-origin can move it.
#[test]
fn the_agent_is_told_about_every_tool_it_can_reach_on_a_turn_it_was_asked() {
    let declared: BTreeSet<String> = crate::tools::agent_tool_declarations()
        .into_iter()
        .map(|d| d.name.as_wire_name().to_owned())
        .collect();
    let lines = crate::tools::deferred_tool_lines();
    let listed: BTreeSet<String> = lines
        .lines()
        .map(|line| {
            let (name, _) = line
                .strip_prefix("- ")
                .and_then(|line| line.split_once(": "))
                .unwrap_or_else(|| panic!("'{line}' isn't one tool's line"));
            name.to_owned()
        })
        .collect();
    let reachable: BTreeSet<String> = ToolId::KNOWN
        .iter()
        .map(|tool| tool.as_wire_name())
        .filter(|name| !matches!(admit(USER_TURN, tool_access(name).expect("listed")), Verdict::Refuse(_)))
        .map(str::to_owned)
        .collect();
    assert!(declared.is_disjoint(&listed), "a tool is either declared or listed");
    assert_eq!(declared.union(&listed).cloned().collect::<BTreeSet<_>>(), reachable);

    let mut expected_declared = names_of_load(Load::Core);
    expected_declared.extend(meta_names());
    assert_eq!(
        declared, expected_declared,
        "the core plus the pair, and nothing else in full"
    );
}

/// The one line a model gets for a deferred tool comes from the table, in table order: what makes two
/// renders byte-identical, and a new tool one entry.
#[test]
fn the_deferred_lines_come_from_the_table_in_its_order() {
    let lines = crate::tools::deferred_tool_lines();
    let names: Vec<&str> = lines
        .lines()
        .filter_map(|line| line.strip_prefix("- ")?.split(':').next())
        .collect();
    let in_table_order: Vec<&str> = ToolId::KNOWN
        .iter()
        .map(|tool| tool.as_wire_name())
        .filter(|name| names.contains(name))
        .collect();
    assert_eq!(names, in_table_order);
    let line = lines
        .lines()
        .find(|line| line.starts_with("- read_resource:"))
        .expect("listed");
    assert_eq!(
        line,
        format!("- read_resource: {}", ToolId::ReadResource.brief().expect("brief"))
    );
}

/// The pair is the in-process agent's: an outside client lists neither and can call neither. The rest
/// of that promise, over the real transport: `mcp/tests/transport_sdk_answers.rs`.
#[tokio::test]
async fn an_outside_client_cant_reach_the_pair_in_or_out_of_direct_mode() {
    let app = tauri::test::mock_app();
    let listed: BTreeSet<String> = get_all_tools().into_iter().map(|tool| tool.name).collect();
    for name in meta_names() {
        assert!(!listed.contains(&name), "'{name}' is listed");
        for caller in [OFF_DIRECT, DIRECT] {
            let ctx = crate::tools::ToolCtx {
                caller,
                cancel: tokio_util::sync::CancellationToken::new(),
                conversation_id: None,
                asked: None,
            };
            let params = serde_json::json!({ "names": ["copy"], "name": "copy" });
            let answer = crate::tools::execute_tool(app.handle(), &ctx, &name, &params).await;
            assert!(answer.is_err(), "'{name}' answered {caller:?}: {answer:?}");
        }
    }
}

/// llama-server refuses the whole request over one declared property with no `type`, which would take
/// Cmdr's own local model down on every call. `set_setting`'s free-form `value` has none, and it's
/// fine only while it stays out of the declarations.
#[test]
fn every_declared_property_has_a_type_a_local_server_can_read() {
    fn untyped(schema: &serde_json::Value, path: &str, found: &mut Vec<String>) {
        let Some(properties) = schema.get("properties").and_then(serde_json::Value::as_object) else {
            return;
        };
        for (key, property) in properties {
            let here = format!("{path}.{key}");
            if property.get("type").is_none() && property.get("anyOf").is_none() {
                found.push(here.clone());
            }
            untyped(property, &here, found);
            if let Some(items) = property.get("items") {
                untyped(items, &here, found);
            }
        }
    }
    let mut found = Vec::new();
    for declaration in crate::tools::agent_tool_declarations() {
        untyped(&declaration.schema, declaration.name.as_wire_name(), &mut found);
    }
    assert!(found.is_empty(), "{found:?}");
}

/// Weak models read a declaration before they read the prompt's list: with these names in `use_tool`'s
/// description, gpt-4.1-mini and Ministral 3B reach for it, and without them they make do with a tool
/// that's close. A name here that isn't deferred would send a model the wrong way.
#[test]
fn every_tool_use_tool_names_is_one_it_is_for() {
    let declaration = crate::tools::agent_tool_declarations()
        .into_iter()
        .find(|d| d.name == ToolId::UseTool)
        .expect("declared");
    let deferred = names_of_load(Load::Deferred);
    let named: Vec<&str> = declaration
        .description
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|word| word.contains('_') || deferred.contains(*word))
        .filter(|word| !["tool_details", "use_tool"].contains(word))
        .collect();
    assert!(!named.is_empty());
    for name in named {
        assert!(deferred.contains(name), "'{name}' isn't a deferred tool");
    }
}

/// A call asking about more than one result holds says so with the counts, like every list.
#[tokio::test]
async fn tool_details_fits_one_result_and_says_when_it_doesnt() {
    let every: Vec<String> = names_of_load(Load::Deferred).into_iter().collect();
    let params = serde_json::json!({ "names": every });
    let answer = crate::tools::meta::execute_tool_details(&params)
        .await
        .expect("an answer");
    let returned = answer["returned"].as_u64().expect("a count");
    assert_eq!(answer["tools"].as_array().expect("a list").len() as u64, returned);
    // A deferred tool the agent can never run says so in its own list, rather than as a schema.
    let unavailable = answer["unavailable"].as_array().expect("the Dev-only tools").len() as u64;
    let reachable = every.len() as u64 - unavailable;
    assert_eq!(answer["total"].as_u64(), Some(reachable));
    assert_eq!(answer["truncated"], returned < reachable);
    let cost = crate::agent::chat::budget::estimate_tokens_of_value(&answer["tools"]);
    assert!(cost <= crate::agent::chat::budget::MAX_TOOL_RESULT_TOKENS, "{cost}");

    let repeated = serde_json::json!({ "names": ["sort", "sort"] });
    let answer = crate::tools::meta::execute_tool_details(&repeated)
        .await
        .expect("an answer");
    assert_eq!(answer["total"], 1, "one tool, however often it's named");
    assert_eq!(answer["tools"][0]["description"], tool_description("sort"));

    assert!(
        crate::tools::meta::execute_tool_details(&serde_json::json!({ "names": "sort" }))
            .await
            .is_err()
    );
}

/// A refusal carries one tool's schema, and it has to stay a small answer whichever tool got it wrong.
#[test]
fn every_deferred_schema_fits_what_a_refusal_may_carry() {
    for name in names_of_load(Load::Deferred) {
        let carried = crate::tools::meta::schema_for_a_refusal(&name);
        let refused = matches!(
            admit(USER_TURN, tool_access(&name).expect("listed")),
            Verdict::Refuse(_)
        );
        assert_eq!(carried.is_none(), refused, "{name}");
    }
    assert!(crate::tools::meta::schema_for_a_refusal("list_dir").is_none(), "core");
    assert!(
        crate::tools::meta::schema_for_a_refusal("dialog_confirm").is_none(),
        "devOnly"
    );
    assert!(crate::tools::meta::schema_for_a_refusal("no_such_tool").is_none());
}

/// The words of a guess, against the tool names and briefs. The last two are the ones that matter most:
/// a model that doesn't know `read_resource` asks for what it wants in its own words.
#[test]
fn a_guess_finds_the_tool_it_was_reaching_for() {
    for (guess, expected) in [
        ("new_tab", "tab"),
        ("open_tab", "tab"),
        ("create_tab", "tab"),
        ("sort_files", "sort"),
        ("read_logs", "read_resource"),
        ("get_logs", "read_resource"),
        ("logs", "read_resource"),
    ] {
        let closest = crate::tools::meta::closest_tools(guess);
        assert_eq!(
            closest.first().map(String::as_str),
            Some(expected),
            "{guess}: {closest:?}"
        );
        assert!(closest.len() <= 3, "{closest:?}");
    }
    assert!(crate::tools::meta::closest_tools("zzz_qqq").is_empty());
    for guess in ["dialog_confirm_now", "confirm_dialog"] {
        assert!(
            !crate::tools::meta::closest_tools(guess).contains(&"dialog_confirm".to_string()),
            "never point at a tool the agent can't run"
        );
    }
}

fn tool_description(name: &str) -> String {
    get_all_tools()
        .into_iter()
        .find(|tool| tool.name == name)
        .expect("listed")
        .description
}

/// A `Meta` entry's access is `Read`, so it stays usable on a wake, and it can't widen anything: `admit`
/// only ever sees `use_tool`'s effective call.
#[test]
fn the_pair_is_exactly_the_two_read_tools_the_design_names() {
    assert_eq!(
        meta_names(),
        ["tool_details", "use_tool"].map(str::to_owned).into_iter().collect()
    );
    for name in meta_names() {
        assert_eq!(tool_access(&name), Some(Access::Read), "{name}");
    }
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
    assert!(core.len() <= 19, "{core:?}");
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

fn meta_names() -> BTreeSet<String> {
    ToolId::KNOWN
        .iter()
        .map(|tool| tool.as_wire_name())
        .filter(|name| matches!(tool_load(name), Some(Load::Meta(_))))
        .map(str::to_owned)
        .collect()
}

fn names_of_load(load: Load) -> BTreeSet<String> {
    ToolId::KNOWN
        .iter()
        .map(|tool| tool.as_wire_name())
        .filter(|name| tool_load(name) == Some(load))
        .map(str::to_owned)
        .collect()
}
