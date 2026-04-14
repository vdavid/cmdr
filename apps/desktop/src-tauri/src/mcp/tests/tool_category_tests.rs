use crate::mcp::resources::get_all_resources;
use crate::mcp::tools::get_all_tools;

#[test]
fn test_app_tools_exist() {
    let tools = get_all_tools();

    // App tools: quit, switch_pane, swap_panes
    let expected = ["quit", "switch_pane", "swap_panes"];
    for name in expected {
        assert!(tools.iter().any(|t| t.name == name), "Missing app tool: {}", name);
    }
}

#[test]
fn test_nav_tools_exist() {
    let tools = get_all_tools();

    // Navigation tools: select_volume, nav_to_path, nav_to_parent, nav_back, nav_forward, scroll_to
    let expected = [
        "select_volume",
        "nav_to_path",
        "nav_to_parent",
        "nav_back",
        "nav_forward",
        "scroll_to",
    ];
    for name in expected {
        assert!(tools.iter().any(|t| t.name == name), "Missing nav tool: {}", name);
    }
}

#[test]
fn test_cursor_tools_exist() {
    let tools = get_all_tools();

    // Cursor tools: move_cursor, open_under_cursor
    let expected = ["move_cursor", "open_under_cursor"];
    for name in expected {
        assert!(tools.iter().any(|t| t.name == name), "Missing cursor tool: {}", name);
    }
}

#[test]
fn test_view_tools_exist() {
    let tools = get_all_tools();

    // View tools: toggle_hidden, set_view_mode, sort
    let expected = ["toggle_hidden", "set_view_mode", "sort"];
    for name in expected {
        assert!(tools.iter().any(|t| t.name == name), "Missing view tool: {}", name);
    }
}

#[test]
fn test_file_op_tools_exist() {
    let tools = get_all_tools();

    // File operation tools: copy, delete, mkdir, refresh
    let expected = ["copy", "delete", "mkdir", "refresh"];
    for name in expected {
        assert!(tools.iter().any(|t| t.name == name), "Missing file op tool: {}", name);
    }
}

#[test]
fn test_context_tools_removed() {
    let tools = get_all_tools();
    let context_tools: Vec<_> = tools.iter().filter(|t| t.name.starts_with("context_")).collect();
    assert!(
        context_tools.is_empty(),
        "Context tools should be removed (moved to resources), but found: {:?}",
        context_tools.iter().map(|t| &t.name).collect::<Vec<_>>()
    );
}

#[test]
fn test_select_volume_tool_schema() {
    let tools = get_all_tools();
    let tool = tools.iter().find(|t| t.name == "select_volume").expect("select_volume");

    let required = tool.input_schema.get("required").and_then(|r| r.as_array());
    assert!(
        required.is_some_and(|r| r.iter().any(|v| v == "pane")),
        "select_volume should require 'pane' parameter"
    );
    assert!(
        required.is_some_and(|r| r.iter().any(|v| v == "name")),
        "select_volume should require 'name' parameter"
    );
}

#[test]
fn test_volume_tools_removed() {
    let tools = get_all_tools();
    let volume_tools: Vec<_> = tools.iter().filter(|t| t.name.starts_with("volume_")).collect();

    // volume_selectLeft/Right are removed, replaced by select_volume
    assert!(
        volume_tools.is_empty(),
        "volume_* tools should be removed (replaced by select_volume), but found: {:?}",
        volume_tools.iter().map(|t| &t.name).collect::<Vec<_>>()
    );
}

#[test]
fn test_state_resource_includes_volumes() {
    // Volumes list is now part of the cmdr://state resource, not a separate resource
    let resources = get_all_resources();
    assert!(
        resources.iter().any(|r| r.uri == "cmdr://state"),
        "cmdr://state resource should exist (includes volumes)"
    );
}

#[test]
fn test_nav_to_path_schema() {
    let tools = get_all_tools();
    let tool = tools.iter().find(|t| t.name == "nav_to_path").expect("nav_to_path");

    let properties = tool.input_schema.get("properties").expect("properties");

    // Check pane property
    let pane_prop = properties.get("pane").expect("pane property");
    assert_eq!(
        pane_prop.get("type").and_then(|t| t.as_str()),
        Some("string"),
        "nav_to_path pane should be type string"
    );

    // Check path property
    let path_prop = properties.get("path").expect("path property");
    assert_eq!(
        path_prop.get("type").and_then(|t| t.as_str()),
        Some("string"),
        "nav_to_path path should be type string"
    );

    // Check required params
    let required = tool.input_schema.get("required").and_then(|r| r.as_array());
    assert!(required.is_some_and(|r| r.iter().any(|v| v == "pane")));
    assert!(required.is_some_and(|r| r.iter().any(|v| v == "path")));
}

#[test]
fn test_no_params_tools_have_empty_required() {
    let tools = get_all_tools();
    let no_param_prefixes = ["app.", "view.", "pane.", "nav.", "sort.", "file.", "context."];

    for tool in &tools {
        // Skip volume tools which may have params
        if tool.name.starts_with("volume.") {
            continue;
        }

        if no_param_prefixes.iter().any(|p| tool.name.starts_with(p)) {
            let required = tool
                .input_schema
                .get("required")
                .and_then(|r| r.as_array())
                .map(|r| r.len())
                .unwrap_or(0);

            assert_eq!(required, 0, "Tool {} should have no required params", tool.name);
        }
    }
}
