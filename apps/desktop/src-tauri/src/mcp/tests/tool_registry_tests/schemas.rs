//! The tool set on the wire, and the JSON schema each tool declares.

use serde_json::json;

use super::{EXPECTED_TOOL_NAMES, tool};
use crate::mcp::tool_registry::get_all_tools;

#[test]
fn test_all_tools_count() {
    // 6 nav + 2 cursor + 1 selection + 8 file_op + 1 tag + 3 view + 1 tab + 2 dialog + 3 app
    // + 2 search + 1 settings + 1 indexing + 1 queue + 1 conflict + 1 archive unlock + 1 favorites
    // + 3 network + 1 eject + 1 await + 1 downloads + 3 operation_log + 2 photo (search + facts)
    // + 1 index listing (list_dir) + 1 diagnostics (memory_diagnostics) = 48
    assert_eq!(get_all_tools().len(), 48);
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
    assert!(props.get("toPane").is_some());
    assert!(props.get("toIndex").is_some());

    let action_enum = props.get("action").unwrap().get("enum").unwrap().as_array().unwrap();
    assert!(action_enum.contains(&json!("move")));
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

    // `type` is a free string (NOT an enum): `close` accepts any dialog id registered
    // by the frontend and listed in cmdr://dialogs/available, validated at runtime, not
    // a fixed schema enum. Open/focus/confirm still validate their subset in the handler.
    let type_prop = props.get("type").unwrap();
    assert_eq!(type_prop.get("type").unwrap(), &json!("string"));
    assert!(type_prop.get("enum").is_none());
    let type_desc = type_prop.get("description").unwrap().as_str().unwrap();
    assert!(type_desc.contains("cmdr://dialogs/available"));

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

#[test]
fn test_operations_list_schema() {
    let tools = get_all_tools();
    let schema = &tool(&tools, "operations_list").input_schema;
    let props = schema.get("properties").unwrap();

    for key in [
        "since",
        "until",
        "name",
        "nameMatch",
        "kind",
        "initiator",
        "executionStatus",
        "rollbackState",
        "limit",
        "offset",
    ] {
        assert!(props.get(key).is_some(), "operations_list schema missing '{key}'");
    }

    // The enum values are the camelCase serde tokens the results also serialize,
    // so an agent round-trips a value it read back verbatim.
    let kind_enum = props.get("kind").unwrap().get("enum").unwrap().as_array().unwrap();
    assert!(kind_enum.contains(&json!("createFolder")));
    assert!(kind_enum.contains(&json!("archiveEdit")));
    let initiator_enum = props.get("initiator").unwrap().get("enum").unwrap().as_array().unwrap();
    assert!(initiator_enum.contains(&json!("aiClient")));

    assert!(schema.get("required").unwrap().as_array().unwrap().is_empty());
}

#[test]
fn test_operations_rollback_schema() {
    let tools = get_all_tools();
    let schema = &tool(&tools, "operations_rollback").input_schema;
    let props = schema.get("properties").unwrap();
    assert!(props.get("operationId").is_some());
    // Rollback has no review dialog, so the caller must opt into it explicitly.
    assert!(props.get("autoConfirm").is_some());

    let required = schema.get("required").unwrap().as_array().unwrap();
    assert_eq!(required.len(), 1);
    assert!(required.contains(&json!("operationId")));
}

/// The instrument has to be reachable from OUTSIDE the app, which is the whole reason the
/// tool exists: `get_memory_diagnostics` ships in release builds and had no caller anywhere,
/// so a running instance could be asked about its panes but never about its own memory.
#[test]
fn test_memory_diagnostics_schema() {
    let tools = get_all_tools();
    let schema = &tool(&tools, "memory_diagnostics").input_schema;
    let props = schema.get("properties").unwrap();

    let sizes = props.get("sizesPerTag").expect("the histogram cap is the one param");
    assert_eq!(sizes.get("type").unwrap(), &json!("integer"));
    assert_eq!(sizes.get("maximum").unwrap(), &json!(24));

    // Nothing is required: the point is that one bare call answers the question.
    assert!(schema.get("required").unwrap().as_array().unwrap().is_empty());
    assert_eq!(schema.get("additionalProperties").unwrap(), &json!(false));
}

#[test]
fn test_queue_tool_schema() {
    let tools = get_all_tools();
    let schema = &tool(&tools, "queue").input_schema;
    let props = schema.get("properties").unwrap();

    for key in ["action", "operationId", "operationIds", "rollback"] {
        assert!(props.get(key).is_some(), "queue schema missing '{key}'");
    }
    let actions = props.get("action").unwrap().get("enum").unwrap().as_array().unwrap();
    for action in ["pause", "resume", "cancel", "pause_all", "resume_all"] {
        assert!(actions.contains(&json!(action)), "missing action '{action}'");
    }
    let required = schema.get("required").unwrap().as_array().unwrap();
    assert_eq!(required, &[json!("action")]);
}

#[test]
fn test_resolve_conflict_schema() {
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
    assert!(!resolutions.contains(&json!("stop")));

    let required = schema.get("required").unwrap().as_array().unwrap();
    for param in ["operationId", "conflictId", "resolution"] {
        assert!(required.contains(&json!(param)), "'{param}' must be required");
    }
}

#[test]
fn test_transfer_tools_offer_the_same_conflict_policies() {
    let tools = get_all_tools();
    for tool_name in ["dialog", "copy", "move"] {
        let policies = tool(&tools, tool_name).input_schema["properties"]["onConflict"]["enum"]
            .as_array()
            .unwrap();
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
fn test_rename_tool_schema() {
    let tools = get_all_tools();
    let schema = &tool(&tools, "rename").input_schema;
    let props = schema.get("properties").unwrap();
    for key in ["pane", "name", "newName", "autoConfirm"] {
        assert!(props.get(key).is_some(), "rename schema missing '{key}'");
    }
    assert_eq!(schema.get("required").unwrap().as_array().unwrap(), &[json!("newName")]);
}

#[test]
fn test_tag_tool_schema() {
    let tools = get_all_tools();
    let schema = &tool(&tools, "tag").input_schema;
    let props = schema.get("properties").unwrap();
    for key in ["pane", "action", "names", "colors"] {
        assert!(props.get(key).is_some(), "tag schema missing '{key}'");
    }
    let actions = props["action"]["enum"].as_array().unwrap();
    for action in ["set", "toggle", "clear"] {
        assert!(actions.contains(&json!(action)), "missing action '{action}'");
    }
    let colors = props["colors"]["items"]["enum"].as_array().unwrap();
    for color in ["red", "orange", "yellow", "green", "blue", "purple", "gray"] {
        assert!(colors.contains(&json!(color)), "missing color '{color}'");
    }
    assert_eq!(schema.get("required").unwrap().as_array().unwrap(), &[json!("action")]);
}

#[test]
fn test_favorites_tool_schema() {
    let tools = get_all_tools();
    let schema = &tool(&tools, "favorites").input_schema;
    let props = schema.get("properties").unwrap();
    for key in ["action", "path", "id", "name", "orderedIds"] {
        assert!(props.get(key).is_some(), "favorites schema missing '{key}'");
    }
    let actions = props["action"]["enum"].as_array().unwrap();
    for action in ["add", "rename", "remove", "reorder"] {
        assert!(actions.contains(&json!(action)), "missing action '{action}'");
    }
    assert_eq!(schema.get("required").unwrap().as_array().unwrap(), &[json!("action")]);
}

#[test]
fn test_eject_tool_schema() {
    let tools = get_all_tools();
    let schema = &tool(&tools, "eject").input_schema;
    assert!(schema["properties"].get("volumeId").is_some());
    assert_eq!(
        schema.get("required").unwrap().as_array().unwrap(),
        &[json!("volumeId")]
    );
}
