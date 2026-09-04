//! `validate_params`: a call carrying what its tool's schema never declared is refused.

use serde_json::json;

use crate::mcp::tool_registry::{agent_tool_view, get_all_tools, tool_schema, validate_params};

#[test]
fn a_call_carrying_a_property_the_schema_never_declared_is_refused() {
    // Straight from a live transcript: the model called `list_dir` with `name` and
    // `nameMatch`, which `list_dir` has never declared. The handler plucked `path` off the
    // raw value, the two extra keys vanished, an ordinary listing came back, and the agent
    // told the user four times that it had searched for "penguin" and found nothing. A
    // fabricated zero is the worst thing this product can say, so the call is refused
    // instead, naming both offenders and what `list_dir` does take, so one turn is enough to
    // self-correct.
    let error = validate_params(
        "list_dir",
        &json!({ "path": "/Users/me/career", "name": "penguin", "nameMatch": "prefix" }),
    )
    .expect_err("a call carrying undeclared properties is refused");

    assert_eq!(error.code, crate::mcp::protocol::INVALID_PARAMS);
    for offender in ["name", "nameMatch"] {
        assert!(
            error.message.contains(offender),
            "the refusal must name the offending property '{offender}': {}",
            error.message
        );
    }
    for accepted in ["path", "sortBy", "order", "limit", "offset", "type"] {
        assert!(
            error.message.contains(accepted),
            "the refusal must list the accepted property '{accepted}': {}",
            error.message
        );
    }
    let data = error.data.expect("the refusal carries typed detail");
    assert_eq!(data["unknownProperties"], json!(["name", "nameMatch"]));
}

#[test]
fn a_call_missing_a_required_property_is_refused_by_name() {
    // `path` is `list_dir`'s only required property. A handler's own check says so for the
    // tools that happen to make one; the gate makes it uniform and names the field.
    let error =
        validate_params("list_dir", &json!({ "limit": 10 })).expect_err("a missing required property is refused");
    assert!(
        error.message.contains("path"),
        "the refusal must name the missing property: {}",
        error.message
    );
    assert_eq!(error.data.expect("typed detail")["missingProperties"], json!(["path"]));
}

#[test]
fn a_schema_that_never_closed_itself_still_takes_whatever_it_is_given() {
    // Most ai-client schemas declare no `additionalProperties`, so they stay open by
    // construction: an extra key there is the schema's own choice, not a silent swallow, and
    // the gate must not start refusing calls that work today.
    for name in ["move_cursor", "sort", "select"] {
        let schema = tool_schema(name).expect("a registered tool has a schema");
        assert!(
            schema.get("additionalProperties").is_none(),
            "{name} closed its schema; pick another open one for this test"
        );
    }
    assert!(
        validate_params(
            "sort",
            &json!({ "pane": "left", "by": "name", "order": "asc", "somethingElse": 1 })
        )
        .is_ok(),
        "an open schema keeps accepting what it always did"
    );
}

#[test]
fn every_agent_tool_closes_its_schema() {
    // The gate only refuses an undeclared property on a schema that CLOSED itself, so an
    // agent tool that left its schema open would swallow a guessed argument exactly as
    // `list_dir` did. The agent view is where that costs the most: nobody reads its calls
    // before they run, and the answer goes straight to a person as a fact. An ai-client
    // schema may stay open (a human is driving, and several always have been); an agent one
    // may not.
    for tool in agent_tool_view() {
        let schema = tool_schema(&tool.name).expect("a registered tool has a schema");
        assert_eq!(
            schema.get("additionalProperties"),
            Some(&json!(false)),
            "agent tool '{}' leaves its schema open, so a property it never declared would be swallowed in silence",
            tool.name
        );
    }
}

#[test]
fn every_declared_schema_accepts_a_call_that_only_uses_what_it_declares() {
    // The gate reads the same schemas the registry publishes, so a shape it mishandles (a
    // nested object, an absent `properties` map) would start refusing working calls. This
    // walks every tool in both views with the properties its own schema declares, and
    // requires a pass.
    for tool in get_all_tools().into_iter().chain(agent_tool_view()) {
        let schema = tool_schema(&tool.name).expect("a registered tool has a schema");
        let mut params = serde_json::Map::new();
        for (name, property) in schema["properties"].as_object().into_iter().flatten() {
            params.insert(name.clone(), sample_for(property));
        }
        assert!(
            validate_params(&tool.name, &serde_json::Value::Object(params.clone())).is_ok(),
            "{} refused a call built from its own declared properties: {params:?}",
            tool.name
        );
    }
}

/// A value of the shape a property's schema declares, enough to satisfy the gate (which
/// reads which keys are present, never what they hold).
fn sample_for(property: &serde_json::Value) -> serde_json::Value {
    match property["type"].as_str() {
        Some("array") => json!([]),
        Some("object") => json!({}),
        Some("integer" | "number") => json!(1),
        Some("boolean") => json!(true),
        _ => json!("x"),
    }
}
