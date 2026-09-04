//! The bearer-token classification: which calls `auth.rs` makes a client prove itself for.

use serde_json::json;

use super::tool;
use crate::mcp::tool_registry::{TokenGate, get_all_tools, tool_gate};

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
fn test_resolve_conflict_schema_and_gate() {
    let tools = get_all_tools();
    let schema = &tool(&tools, "resolve_conflict").input_schema;
    let props = schema.get("properties").unwrap();

    let resolutions = props
        .get("resolution")
        .unwrap()
        .get("enum")
        .unwrap()
        .as_array()
        .unwrap();
    for resolution in ["skip", "overwrite", "rename", "overwrite_smaller", "overwrite_older"] {
        assert!(resolutions.contains(&json!(resolution)), "missing '{resolution}'");
    }
    // `stop` is the policy that RAISES the question; offering it as an answer
    // would park the operation on the same clash again.
    assert!(!resolutions.contains(&json!("stop")));

    // The clash's id is required: an answer that doesn't name one can land on
    // whatever the operation has parked on since.
    let required = schema.get("required").unwrap().as_array().unwrap();
    for param in ["operationId", "conflictId", "resolution"] {
        assert!(required.contains(&json!(param)), "'{param}' must be required");
    }

    // It answers, with no dialog, a question that was put to the user — and
    // `overwrite` destroys a file. Same bypass the token guards everywhere else.
    assert_eq!(tool_gate("resolve_conflict"), Some(TokenGate::Always));
}

#[test]
fn test_every_way_to_start_a_transfer_offers_the_same_conflict_policies() {
    // Without `stop`, an agent-driven transfer settles its clashes upfront and
    // the per-file prompt is unreachable from automation — which is how a
    // wedging bug in it survived for months. And ONE list across the three
    // tools: the two frontend maps that predated this had drifted on the
    // conditional names, and neither spelling was reachable, so nobody noticed.
    let tools = get_all_tools();
    for tool_name in ["dialog", "copy", "move"] {
        let policies = tool(&tools, tool_name)
            .input_schema
            .get("properties")
            .unwrap()
            .get("onConflict")
            .unwrap()
            .get("enum")
            .unwrap()
            .as_array()
            .unwrap()
            .clone();
        for policy in [
            "stop",
            "skip_all",
            "overwrite_all",
            "rename_all",
            "overwrite_smaller_all",
            "overwrite_older_all",
        ] {
            assert!(
                policies.contains(&json!(policy)),
                "{tool_name} is missing policy '{policy}'"
            );
        }
        assert_eq!(policies.len(), 6, "{tool_name}: policy list differs from the rest");
    }
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
        ("resolve_conflict", TokenGate::Always),
        // Gated even though it starts nothing: it hands a secret in, and the
        // `wrongAttempt` flag it makes observable would otherwise be a password
        // oracle any loopback process could grind against. DETAILS § Authentication.
        ("unlock_archive", TokenGate::Always),
        ("connect_to_server", TokenGate::Open),
        ("remove_manual_server", TokenGate::Open),
        ("upgrade_smb_to_direct", TokenGate::Open),
        ("await", TokenGate::Open),
        ("go_to_latest_download", TokenGate::Open),
        ("operations_list", TokenGate::Open),
        ("operations_get", TokenGate::Open),
        ("operations_rollback", TokenGate::IfAutoConfirm),
        ("search_photos", TokenGate::Open),
        ("image_facts", TokenGate::Open),
        ("list_dir", TokenGate::Open),
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
