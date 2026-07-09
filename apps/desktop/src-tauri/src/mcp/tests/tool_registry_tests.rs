//! Schema-shape and token-gate tests for the `mcp_tools!` registry table.
//!
//! These live beside the table rather than inline in `tool_registry.rs`, so that
//! authored source stays a lean, single-purpose declaration (the `file-length`
//! scanner flags it otherwise). They drive only the public registry surface
//! (`get_all_tools` / `tool_gate` / `TokenGate`), so no `super` access is needed.

use serde_json::json;

use crate::mcp::tool_registry::{TokenGate, get_all_tools, tool_gate};
use crate::mcp::tools::Tool;

fn tool<'a>(tools: &'a [Tool], name: &str) -> &'a Tool {
    tools.iter().find(|t| t.name == name).expect("tool present")
}

/// The exact set of tool names on the wire. Dispatch (`execute_tool`) is generated from
/// the same table, so it covers exactly this set by construction; this pins the set so a
/// stray add/remove/rename is a hard failure, not a silent one.
const EXPECTED_TOOL_NAMES: &[&str] = &[
    "select_volume",
    "nav_to_path",
    "nav_to_parent",
    "nav_back",
    "nav_forward",
    "scroll_to",
    "move_cursor",
    "open_under_cursor",
    "select",
    "copy",
    "move",
    "compress",
    "delete",
    "rename",
    "mkdir",
    "mkfile",
    "refresh",
    "tag",
    "toggle_hidden",
    "set_view_mode",
    "sort",
    "tab",
    "dialog",
    "open_search_dialog",
    "quit",
    "switch_pane",
    "swap_panes",
    "search",
    "ai_search",
    "set_setting",
    "indexing",
    "queue",
    "favorites",
    "connect_to_server",
    "remove_manual_server",
    "upgrade_smb_to_direct",
    "eject",
    "await",
    "go_to_latest_download",
];

#[test]
fn test_all_tools_count() {
    // 6 nav + 2 cursor + 1 selection + 8 file_op + 1 tag + 3 view + 1 tab + 2 dialog + 3 app
    // + 2 search + 1 settings + 1 indexing + 1 queue + 1 favorites + 3 network + 1 eject + 1
    // await + 1 downloads = 39
    assert_eq!(get_all_tools().len(), 39);
}

#[test]
fn test_tool_names_are_exactly_the_expected_set() {
    use std::collections::BTreeSet;
    let actual: BTreeSet<String> = get_all_tools().into_iter().map(|t| t.name).collect();
    let expected: BTreeSet<String> = EXPECTED_TOOL_NAMES.iter().map(|s| (*s).to_owned()).collect();
    assert_eq!(actual, expected, "tool name set drifted from the expected list");
}

#[test]
fn test_tab_tool_schema() {
    let tools = get_all_tools();
    let schema = &tool(&tools, "tab").input_schema;
    let props = schema.get("properties").unwrap();

    assert!(props.get("action").is_some());
    assert!(props.get("pane").is_some());
    assert!(props.get("tabId").is_some());
    assert!(props.get("pinned").is_some());

    let action_enum = props.get("action").unwrap().get("enum").unwrap().as_array().unwrap();
    assert!(action_enum.contains(&json!("new")));
    assert!(action_enum.contains(&json!("close")));
    assert!(action_enum.contains(&json!("close_others")));
    assert!(action_enum.contains(&json!("activate")));
    assert!(action_enum.contains(&json!("set_pinned")));
    assert!(action_enum.contains(&json!("reopen")));

    let pane_enum = props.get("pane").unwrap().get("enum").unwrap().as_array().unwrap();
    assert!(pane_enum.contains(&json!("left")));
    assert!(pane_enum.contains(&json!("right")));

    let required = schema.get("required").unwrap().as_array().unwrap();
    assert_eq!(required.len(), 2);
    assert!(required.contains(&json!("action")));
    assert!(required.contains(&json!("pane")));
}

#[test]
fn test_set_setting_tool_schema() {
    let tools = get_all_tools();
    let schema = &tool(&tools, "set_setting").input_schema;
    let props = schema.get("properties").unwrap();
    assert!(props.get("id").is_some());
    assert!(props.get("value").is_some());

    let required = schema.get("required").unwrap().as_array().unwrap();
    assert_eq!(required.len(), 2);
    assert!(required.contains(&json!("id")));
    assert!(required.contains(&json!("value")));
}

#[test]
fn test_open_search_dialog_schema() {
    let tools = get_all_tools();
    let schema = &tool(&tools, "open_search_dialog").input_schema;
    let props = schema.get("properties").unwrap();

    for key in [
        "query",
        "mode",
        "sizeMin",
        "sizeMax",
        "modifiedAfter",
        "modifiedBefore",
        "isDirectory",
        "scope",
        "caseSensitive",
        "excludeSystemDirs",
        "autoRun",
    ] {
        assert!(props.get(key).is_some(), "open_search_dialog schema missing '{key}'");
    }

    let mode_enum = props.get("mode").unwrap().get("enum").unwrap().as_array().unwrap();
    assert!(mode_enum.contains(&json!("ai")));
    assert!(mode_enum.contains(&json!("filename")));
    assert!(mode_enum.contains(&json!("regex")));

    let required = schema.get("required").unwrap().as_array().unwrap();
    assert!(required.is_empty(), "open_search_dialog should have no required fields");
}

#[test]
fn test_select_tool_schema() {
    let tools = get_all_tools();
    let schema = &tool(&tools, "select").input_schema;
    let props = schema.get("properties").unwrap();

    assert!(props.get("pane").is_some());
    assert!(props.get("start").is_some());
    assert!(props.get("count").is_some());
    assert!(props.get("all").is_some());
    assert!(props.get("mode").is_some());

    // count should be a plain integer, not oneOf (schemars would break this)
    assert_eq!(props["count"]["type"], "integer");
    assert_eq!(props["all"]["type"], "boolean");

    let required = schema.get("required").unwrap().as_array().unwrap();
    assert_eq!(required.len(), 1);
    assert!(required.contains(&json!("pane")));
}

#[test]
fn test_move_cursor_tool_schema() {
    let tools = get_all_tools();
    let schema = &tool(&tools, "move_cursor").input_schema;
    let props = schema.get("properties").unwrap();

    assert!(props.get("pane").is_some());
    assert_eq!(props["index"]["type"], "integer");
    assert_eq!(props["filename"]["type"], "string");

    let required = schema.get("required").unwrap().as_array().unwrap();
    assert_eq!(required.len(), 1);
    assert!(required.contains(&json!("pane")));

    // move_cursor normalizes index/filename in the handler; no "to" property on the wire
    assert!(props.get("to").is_none());
}

#[test]
fn test_dialog_tool_schema() {
    let tools = get_all_tools();
    let schema = &tool(&tools, "dialog").input_schema;
    let props = schema.get("properties").unwrap();

    assert!(props.get("action").is_some());
    assert!(props.get("type").is_some());
    assert!(props.get("section").is_some());
    assert!(props.get("path").is_some());
    assert!(props.get("onConflict").is_some());

    let action_enum = props.get("action").unwrap().get("enum").unwrap().as_array().unwrap();
    assert!(action_enum.contains(&json!("open")));
    assert!(action_enum.contains(&json!("focus")));
    assert!(action_enum.contains(&json!("close")));
    assert!(action_enum.contains(&json!("confirm")));

    let type_enum = props.get("type").unwrap().get("enum").unwrap().as_array().unwrap();
    assert!(type_enum.contains(&json!("settings")));
    assert!(type_enum.contains(&json!("file-viewer")));
    assert!(type_enum.contains(&json!("about")));
    assert!(type_enum.contains(&json!("transfer-confirmation")));
    assert!(type_enum.contains(&json!("copy-confirmation")));
    assert!(type_enum.contains(&json!("mkdir-confirmation")));
    assert!(type_enum.contains(&json!("new-file-confirmation")));
    assert!(type_enum.contains(&json!("delete-confirmation")));

    let required = schema.get("required").unwrap().as_array().unwrap();
    assert_eq!(required.len(), 2);
    assert!(required.contains(&json!("action")));
    assert!(required.contains(&json!("type")));
}

#[test]
fn test_sort_tool_schema() {
    let tools = get_all_tools();
    let schema = &tool(&tools, "sort").input_schema;
    let props = schema.get("properties").unwrap();

    assert!(props.get("pane").is_some());
    assert!(props.get("by").is_some());
    assert!(props.get("order").is_some());

    let by_enum = props.get("by").unwrap().get("enum").unwrap().as_array().unwrap();
    assert!(by_enum.contains(&json!("name")));
    assert!(by_enum.contains(&json!("ext")));
    assert!(by_enum.contains(&json!("size")));
    assert!(by_enum.contains(&json!("modified")));
    assert!(by_enum.contains(&json!("created")));

    let order_enum = props.get("order").unwrap().get("enum").unwrap().as_array().unwrap();
    assert!(order_enum.contains(&json!("asc")));
    assert!(order_enum.contains(&json!("desc")));

    let required = schema.get("required").unwrap().as_array().unwrap();
    assert_eq!(required.len(), 3);
    assert!(required.contains(&json!("pane")));
    assert!(required.contains(&json!("by")));
    assert!(required.contains(&json!("order")));
}

#[test]
fn test_set_view_mode_tool_schema() {
    let tools = get_all_tools();
    let schema = &tool(&tools, "set_view_mode").input_schema;
    let props = schema.get("properties").unwrap();

    assert!(props.get("pane").is_some());
    assert!(props.get("mode").is_some());

    let mode_enum = props.get("mode").unwrap().get("enum").unwrap().as_array().unwrap();
    assert!(mode_enum.contains(&json!("brief")));
    assert!(mode_enum.contains(&json!("full")));

    let required = schema.get("required").unwrap().as_array().unwrap();
    assert_eq!(required.len(), 2);
    assert!(required.contains(&json!("pane")));
    assert!(required.contains(&json!("mode")));
}

#[test]
fn test_indexing_tool_schema() {
    let tools = get_all_tools();
    let schema = &tool(&tools, "indexing").input_schema;
    let props = schema.get("properties").unwrap();

    assert!(props.get("action").is_some());
    assert!(props.get("volumeId").is_some());

    let action_enum = props.get("action").unwrap().get("enum").unwrap().as_array().unwrap();
    for action in ["enable", "disable", "rescan", "forget"] {
        assert!(action_enum.contains(&json!(action)), "missing action '{action}'");
    }

    let required = schema.get("required").unwrap().as_array().unwrap();
    assert_eq!(required.len(), 2);
    assert!(required.contains(&json!("action")));
    assert!(required.contains(&json!("volumeId")));

    // Silent per-drive config mutation with no confirmation dialog → gated.
    assert_eq!(tool_gate("indexing"), Some(TokenGate::Always));
}

#[test]
fn test_await_has_index_status_condition() {
    let tools = get_all_tools();
    let schema = &tool(&tools, "await").input_schema;
    let props = schema.get("properties").unwrap();
    assert!(
        props.get("volumeId").is_some(),
        "await should carry volumeId for index_status"
    );

    let cond_enum = props.get("condition").unwrap().get("enum").unwrap().as_array().unwrap();
    assert!(cond_enum.contains(&json!("index_status")));
    // The operation-queue conditions ride the same tool.
    assert!(cond_enum.contains(&json!("operation_complete")));
    assert!(cond_enum.contains(&json!("operations_idle")));

    // Only `condition` is required now: pane is scoped to the pane conditions and
    // `value` is unused by `operations_idle`, so both are validated per-condition
    // in the handler, not by the schema.
    let required = schema.get("required").unwrap().as_array().unwrap();
    assert!(required.contains(&json!("condition")));
    assert!(!required.contains(&json!("value")));
    assert!(!required.contains(&json!("pane")));
}

#[test]
fn test_downloads_tool_present() {
    let tools = get_all_tools();
    assert_eq!(tool(&tools, "go_to_latest_download").name, "go_to_latest_download");
}

// ── Token gate (auth classification) ──────────────────────────────────────

#[test]
fn test_tool_gate_per_name() {
    assert_eq!(tool_gate("copy"), Some(TokenGate::IfAutoConfirm));
    assert_eq!(tool_gate("move"), Some(TokenGate::IfAutoConfirm));
    assert_eq!(tool_gate("compress"), Some(TokenGate::IfAutoConfirm));
    assert_eq!(tool_gate("delete"), Some(TokenGate::IfAutoConfirm));
    assert_eq!(tool_gate("set_setting"), Some(TokenGate::Always));
    assert_eq!(tool_gate("dialog"), Some(TokenGate::IfConfirmAction));
    assert_eq!(tool_gate("nav_to_path"), Some(TokenGate::Open));
    assert_eq!(tool_gate("bogus"), None);
}

/// Anti-footgun backstop: any tool whose schema takes `autoConfirm` (i.e. can bypass the
/// user's confirmation dialog) MUST be gated `IfAutoConfirm`, never left `Open`. Adding a
/// destructive auto-confirm tool and forgetting its gate fails here.
#[test]
fn test_autoconfirm_tools_are_gated() {
    for t in get_all_tools() {
        let has_auto_confirm = t
            .input_schema
            .get("properties")
            .and_then(|p| p.get("autoConfirm"))
            .is_some();
        if has_auto_confirm {
            assert_eq!(
                tool_gate(&t.name),
                Some(TokenGate::IfAutoConfirm),
                "tool '{}' exposes autoConfirm but isn't gated IfAutoConfirm",
                t.name
            );
        }
    }
}

#[test]
fn test_queue_tool_schema_and_gate() {
    let tools = get_all_tools();
    let schema = &tool(&tools, "queue").input_schema;
    let props = schema.get("properties").unwrap();

    assert!(props.get("action").is_some());
    assert!(props.get("operationId").is_some());
    assert!(props.get("operationIds").is_some());
    assert!(props.get("rollback").is_some());

    let action_enum = props.get("action").unwrap().get("enum").unwrap().as_array().unwrap();
    for action in ["pause", "resume", "cancel", "pause_all", "resume_all"] {
        assert!(action_enum.contains(&json!(action)), "missing action '{action}'");
    }

    let required = schema.get("required").unwrap().as_array().unwrap();
    // Only `action` is required; the per-op actions validate operationId in the handler
    // (pause_all / resume_all need no id).
    assert_eq!(required.len(), 1);
    assert!(required.contains(&json!("action")));

    // A rollback cancel deletes already-copied files → gated by the token.
    assert_eq!(tool_gate("queue"), Some(TokenGate::IfRollback));
}

#[test]
fn test_rename_tool_schema_and_gate() {
    let tools = get_all_tools();
    let schema = &tool(&tools, "rename").input_schema;
    let props = schema.get("properties").unwrap();
    for key in ["pane", "name", "newName", "autoConfirm"] {
        assert!(props.get(key).is_some(), "rename schema missing '{key}'");
    }
    let required = schema.get("required").unwrap().as_array().unwrap();
    assert_eq!(required.len(), 1);
    assert!(required.contains(&json!("newName")));

    // autoConfirm bypasses the review editor → gated (also pinned structurally
    // by `test_autoconfirm_tools_are_gated`).
    assert_eq!(tool_gate("rename"), Some(TokenGate::IfAutoConfirm));
}

#[test]
fn test_tag_tool_schema_and_gate() {
    let tools = get_all_tools();
    let schema = &tool(&tools, "tag").input_schema;
    let props = schema.get("properties").unwrap();

    assert!(props.get("pane").is_some());
    assert!(props.get("action").is_some());
    assert!(props.get("names").is_some());
    assert!(props.get("colors").is_some());

    let action_enum = props.get("action").unwrap().get("enum").unwrap().as_array().unwrap();
    for action in ["set", "toggle", "clear"] {
        assert!(action_enum.contains(&json!(action)), "missing action '{action}'");
    }
    let color_enum = props["colors"]["items"]["enum"].as_array().unwrap();
    for color in ["red", "orange", "yellow", "green", "blue", "purple", "gray"] {
        assert!(color_enum.contains(&json!(color)), "missing color '{color}'");
    }

    let required = schema.get("required").unwrap().as_array().unwrap();
    assert_eq!(required.len(), 1);
    assert!(required.contains(&json!("action")));

    // Silent metadata mutation on user files, no confirmation dialog → gated.
    assert_eq!(tool_gate("tag"), Some(TokenGate::Always));
}

#[test]
fn test_favorites_tool_schema_and_gate() {
    let tools = get_all_tools();
    let schema = &tool(&tools, "favorites").input_schema;
    let props = schema.get("properties").unwrap();

    for key in ["action", "path", "id", "name", "orderedIds"] {
        assert!(props.get(key).is_some(), "favorites schema missing '{key}'");
    }
    let action_enum = props.get("action").unwrap().get("enum").unwrap().as_array().unwrap();
    for action in ["add", "rename", "remove", "reorder"] {
        assert!(action_enum.contains(&json!(action)), "missing action '{action}'");
    }
    let required = schema.get("required").unwrap().as_array().unwrap();
    assert_eq!(required.len(), 1);
    assert!(required.contains(&json!("action")));

    // Persistent app-config mutation with no confirmation dialog → gated.
    assert_eq!(tool_gate("favorites"), Some(TokenGate::Always));
}

#[test]
fn test_eject_tool_schema_and_gate() {
    let tools = get_all_tools();
    let schema = &tool(&tools, "eject").input_schema;
    let props = schema.get("properties").unwrap();
    assert!(props.get("volumeId").is_some());

    let required = schema.get("required").unwrap().as_array().unwrap();
    assert_eq!(required.len(), 1);
    assert!(required.contains(&json!("volumeId")));

    // Reversible one-click runtime action with an honest busy refusal → open.
    assert_eq!(tool_gate("eject"), Some(TokenGate::Open));
}

/// Anti-footgun backstop mirroring `test_autoconfirm_tools_are_gated`: any tool whose schema
/// exposes a `rollback` property (a destructive, file-deleting bypass) MUST declare the
/// `IfRollback` gate, never `Open`. Adding such a tool and forgetting its gate fails here.
#[test]
fn test_rollback_tools_are_gated() {
    for t in get_all_tools() {
        let has_rollback = t
            .input_schema
            .get("properties")
            .and_then(|p| p.get("rollback"))
            .is_some();
        if has_rollback {
            assert_eq!(
                tool_gate(&t.name),
                Some(TokenGate::IfRollback),
                "tool '{}' exposes rollback but isn't gated IfRollback",
                t.name
            );
        }
    }
}

/// Full-table expectation with set-equality: every tool's gate is pinned, AND the set of
/// tools in the registry equals the set with a declared gate. Set-equality is load-bearing:
/// it forces a conscious auth review for any new tool (a newly-added tool left `Open` fails here).
#[test]
fn test_gate_table_is_complete_and_correct() {
    use std::collections::BTreeMap;
    let expected: BTreeMap<&str, TokenGate> = [
        ("tag", TokenGate::Always),
        ("favorites", TokenGate::Always),
        ("eject", TokenGate::Open),
        ("select_volume", TokenGate::Open),
        ("nav_to_path", TokenGate::Open),
        ("nav_to_parent", TokenGate::Open),
        ("nav_back", TokenGate::Open),
        ("nav_forward", TokenGate::Open),
        ("scroll_to", TokenGate::Open),
        ("move_cursor", TokenGate::Open),
        ("open_under_cursor", TokenGate::Open),
        ("select", TokenGate::Open),
        ("copy", TokenGate::IfAutoConfirm),
        ("move", TokenGate::IfAutoConfirm),
        ("compress", TokenGate::IfAutoConfirm),
        ("delete", TokenGate::IfAutoConfirm),
        ("rename", TokenGate::IfAutoConfirm),
        ("mkdir", TokenGate::IfAutoConfirm),
        ("mkfile", TokenGate::IfAutoConfirm),
        ("refresh", TokenGate::Open),
        ("toggle_hidden", TokenGate::Open),
        ("set_view_mode", TokenGate::Open),
        ("sort", TokenGate::Open),
        ("tab", TokenGate::Open),
        ("dialog", TokenGate::IfConfirmAction),
        ("open_search_dialog", TokenGate::Open),
        ("quit", TokenGate::Open),
        ("switch_pane", TokenGate::Open),
        ("swap_panes", TokenGate::Open),
        ("search", TokenGate::Open),
        ("ai_search", TokenGate::Open),
        ("set_setting", TokenGate::Always),
        ("indexing", TokenGate::Always),
        ("queue", TokenGate::IfRollback),
        ("connect_to_server", TokenGate::Open),
        ("remove_manual_server", TokenGate::Open),
        ("upgrade_smb_to_direct", TokenGate::Open),
        ("await", TokenGate::Open),
        ("go_to_latest_download", TokenGate::Open),
    ]
    .into_iter()
    .collect();

    let actual: std::collections::BTreeSet<String> = get_all_tools().into_iter().map(|t| t.name).collect();
    let expected_names: std::collections::BTreeSet<String> = expected.keys().map(|s| (*s).to_owned()).collect();
    assert_eq!(actual, expected_names, "registry tool set differs from the gate table");

    for (name, gate) in expected {
        assert_eq!(
            tool_gate(name),
            Some(gate),
            "gate for '{name}' differs from expectation"
        );
    }
}

#[test]
fn test_requires_token_arg_logic() {
    // IfAutoConfirm: only when autoConfirm == true
    assert!(TokenGate::IfAutoConfirm.requires_token(Some(&json!({"autoConfirm": true}))));
    assert!(!TokenGate::IfAutoConfirm.requires_token(Some(&json!({"autoConfirm": false}))));
    assert!(!TokenGate::IfAutoConfirm.requires_token(Some(&json!({}))));
    assert!(!TokenGate::IfAutoConfirm.requires_token(None));
    // IfConfirmAction: only when action == "confirm"
    assert!(TokenGate::IfConfirmAction.requires_token(Some(&json!({"action": "confirm"}))));
    assert!(!TokenGate::IfConfirmAction.requires_token(Some(&json!({"action": "open"}))));
    assert!(!TokenGate::IfConfirmAction.requires_token(None));
    // IfRollback: only when rollback == true (a plain cancel stays open).
    assert!(TokenGate::IfRollback.requires_token(Some(&json!({"action": "cancel", "rollback": true}))));
    assert!(!TokenGate::IfRollback.requires_token(Some(&json!({"action": "cancel", "rollback": false}))));
    assert!(!TokenGate::IfRollback.requires_token(Some(&json!({"action": "cancel"}))));
    assert!(!TokenGate::IfRollback.requires_token(None));
    // Always / Open
    assert!(TokenGate::Always.requires_token(None));
    assert!(!TokenGate::Open.requires_token(Some(&json!({"autoConfirm": true}))));
}
