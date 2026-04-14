use crate::mcp::tools::get_all_tools;

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
    // 6 nav + 2 cursor + 1 select + 6 file_op + 3 view + 1 tab + 1 dialog + 3 app + 2 search + 1 settings + 2 network + 1 await = 29
    assert_eq!(
        tools.len(),
        29,
        "Expected 29 tools, got {}. Did you add/remove tools?",
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
