//! `validate_params`: a call carrying what its tool's schema never declared is refused.

use serde_json::json;

use crate::tools::{ToolId, tool_schema, validate_params};

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

    assert_eq!(error.code, crate::tools::error_codes::INVALID_PARAMS);
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
fn a_per_row_property_sent_at_the_top_level_is_told_which_row_takes_it() {
    // Straight from a live transcript: the model put ONE `volumeId` beside `renames` rather
    // than one on every row, which is the natural read of a plan that binds a single volume.
    // Naming only the top-level offender and then listing `renames` as everything the tool
    // takes reads as "your rows were fine", so the model re-sent the byte-identical call,
    // `chat/runtime/repeats.rs` stopped the second one, and it then told the user the plan was
    // submitted. Nothing had been staged. A refusal that names where the property goes is what
    // ends that loop, so the gate looks one level down to say it.
    let error = validate_params(
        "propose_rename_plan",
        &json!({
            "volumeId": "root",
            "renames": [{
                "sourcePath": "/Users/me/notes/2030-01-01 22-47-24.md",
                "destinationName": "a.md",
                "evidence": { "source": "userInstruction", "detail": "Sequential rename: a.md" },
            }],
        }),
    )
    .expect_err("a per-row property sent at the top level is refused");

    assert!(
        error.message.contains("each renames row takes volumeId"),
        "the refusal must say which row takes the property: {}",
        error.message
    );
    let data = error.data.expect("the refusal carries typed detail");
    assert_eq!(
        data["misplacedProperties"],
        json!([{ "property": "volumeId", "rows": "renames" }]),
        "the same fact rides the typed detail, so a caller acts on the shape"
    );
    assert_eq!(
        data["unknownProperties"],
        json!([]),
        "a property the schema declares one level down is misplaced, never undeclared"
    );
}

#[test]
fn a_property_no_row_declares_either_stays_undeclared() {
    // The one-level look-down explains a hoisted field; it must not turn a genuinely invented
    // property into a confident "put it on a row".
    let error = validate_params("propose_rename_plan", &json!({ "renames": [], "dryRun": true }))
        .expect_err("an invented property is refused");
    let data = error.data.expect("typed detail");
    assert_eq!(data["unknownProperties"], json!(["dryRun"]));
    assert_eq!(data["misplacedProperties"], json!([]));
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

/// Every object a schema describes that leaves itself open, by its path through the schema.
/// Walks the schema's own shape, so a property that happens to be called `type` or `properties`
/// isn't mistaken for the keyword.
fn open_objects(schema: &serde_json::Value, at: &str, open: &mut Vec<String>) {
    let Some(map) = schema.as_object() else { return };
    let is_object = map.get("type").and_then(|t| t.as_str()) == Some("object") || map.contains_key("properties");
    if is_object && map.get("additionalProperties") != Some(&json!(false)) {
        open.push(at.to_string());
    }
    for (keyword, value) in map {
        match keyword.as_str() {
            "properties" => {
                for (name, property) in value.as_object().into_iter().flatten() {
                    open_objects(property, &format!("{at}.{name}"), open);
                }
            }
            "enum" | "const" | "default" | "examples" => {}
            _ => match value {
                serde_json::Value::Array(branches) => {
                    for (i, branch) in branches.iter().enumerate() {
                        open_objects(branch, &format!("{at}.{keyword}[{i}]"), open);
                    }
                }
                other => open_objects(other, &format!("{at}.{keyword}"), open),
            },
        }
    }
}

#[test]
fn every_tool_in_the_table_closes_its_schema_all_the_way_down() {
    // The gate only refuses an undeclared property on a schema that CLOSED itself, so one left
    // open swallows a guessed argument exactly as `list_dir` did, and a stale `rollback: true`
    // would act on less than was asked. Walks the table, not a caller's view, so no tool escapes
    // by who may call it.
    let mut open = Vec::new();
    for tool in ToolId::KNOWN {
        let name = tool.as_wire_name();
        let schema = tool_schema(name).expect("a table entry has a schema");
        open_objects(&schema, name, &mut open);
    }
    assert!(
        open.is_empty(),
        "these schemas leave an object open, so a property they never declared would be swallowed in silence: {open:#?}"
    );
}

#[test]
fn the_walk_finds_an_open_object_however_it_is_nested() {
    // The test above is only as good as its walk. `type` here is a property, not a keyword.
    let schema = json!({
        "type": "object",
        "properties": {
            "type": { "type": "string" },
            "rows": { "type": "array", "items": { "type": "object", "properties": {} } },
            "either": { "anyOf": [{ "type": "object", "properties": {}, "additionalProperties": false }, { "type": "object" }] }
        },
        "additionalProperties": false
    });
    let mut open = Vec::new();
    open_objects(&schema, "t", &mut open);
    open.sort();
    assert_eq!(open, vec!["t.either.anyOf[1]", "t.rows.items"]);
}

#[test]
fn every_tool_in_the_table_refuses_a_property_it_never_declared() {
    // What closing is for, end to end through the gate. It runs before the ask, so a stray argument
    // on `quit` or `eject` refuses before it reaches a person.
    for tool in ToolId::KNOWN {
        let name = tool.as_wire_name();
        let error = validate_params(name, &json!({ "notARealParameter": true }))
            .expect_err("a property nobody declared is refused");
        assert_eq!(
            error.data.expect("typed detail")["unknownProperties"],
            json!(["notARealParameter"]),
            "{name} must name the offender"
        );
    }
}

#[test]
fn switch_pane_refuses_a_pane_rather_than_ignoring_it() {
    // A pane here reads as "focus this pane", and the call toggles to whichever pane isn't focused,
    // which can be the opposite one.
    let error = validate_params("switch_pane", &json!({ "pane": "left" })).expect_err("switch_pane takes nothing");
    assert_eq!(error.data.expect("typed detail")["unknownProperties"], json!(["pane"]));
}

#[test]
fn every_declared_schema_accepts_a_call_that_only_uses_what_it_declares() {
    // The gate reads the same schemas the registry publishes, so a shape it mishandles (a
    // nested object, an absent `properties` map) would start refusing working calls. This
    // walks every tool with the properties its own schema declares, and requires a pass.
    for tool in ToolId::KNOWN {
        let name = tool.as_wire_name();
        let schema = tool_schema(name).expect("a registered tool has a schema");
        let mut params = serde_json::Map::new();
        for (name, property) in schema["properties"].as_object().into_iter().flatten() {
            params.insert(name.clone(), sample_for(property));
        }
        assert!(
            validate_params(name, &serde_json::Value::Object(params.clone())).is_ok(),
            "{name} refused a call built from its own declared properties: {params:?}"
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
