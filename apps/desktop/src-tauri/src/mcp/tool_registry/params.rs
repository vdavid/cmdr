//! The declared-schema gate: what a params object must satisfy before any handler sees it.
//!
//! Handlers pluck fields off a raw `serde_json::Value` (`params.get("path")`), so a key the
//! schema never declared used to vanish without a trace. A model that guessed a `name`
//! filter onto `list_dir` got an ordinary folder listing back and then reported, confidently
//! and four times over, that it had searched and found nothing. A fabricated zero is exactly
//! what `agent/tools/CLAUDE.md`'s honesty contract exists to prevent, so an undeclared
//! property is now a refusal that names itself.
//!
//! **Deliberately not a JSON Schema validator.** It reads ONE level of ONE object schema and
//! answers two questions: is every `required` property present, and (only when the schema
//! closed itself with `additionalProperties: false`) is every property present one the schema
//! declares. Nothing else:
//!
//! - **Types and enums stay unchecked.** A wrong type already produces an honest, specific
//!   refusal from the handler that reads the field; the silent failure mode was only ever
//!   about keys nobody reads and fields nobody supplied.
//! - **Nesting stays unchecked.** The tools that take structured rows (`propose_rename_plan`,
//!   `propose_suggestions`) deserialize them into serde structs carrying
//!   `deny_unknown_fields`, which already refuses an unknown field and names it. What those
//!   structs can't do is name the ROW, which is why [`check_object`] is public: the rename
//!   boundary points it at one row at a time.
//!
//! So a schema that never declared `additionalProperties` (most of the ai-client family) stays
//! open exactly as it always was, and closing one is what opts a tool in.

use serde_json::Value;

use crate::mcp::executor::ToolError;

/// One way a params object contradicted the schema it was checked against. Ordered so a
/// caller collecting violations across many objects (the rename boundary, row by row) reports
/// them in a stable order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ParamViolation {
    /// A property the schema doesn't declare, on a schema that closed itself with
    /// `additionalProperties: false`.
    Unknown(String),
    /// A property the schema's `required` list names, absent from the object.
    Missing(String),
}

/// Everything one object got wrong, plus what it was allowed to carry. Empty
/// [`violations`](Self::violations) means it checked out.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParamProblems {
    pub violations: Vec<ParamViolation>,
    /// Every property the schema declares: what a caller may send. Alphabetical, which is
    /// also the order the schema itself went out on the wire in (serde_json's `Map` is a
    /// `BTreeMap`), so the list reads back in the order the caller was shown.
    pub accepted: Vec<String>,
}

impl ParamProblems {
    pub fn is_empty(&self) -> bool {
        self.violations.is_empty()
    }

    fn named(&self, pick: fn(&ParamViolation) -> Option<&String>) -> Vec<&String> {
        self.violations.iter().filter_map(pick).collect()
    }

    fn unknown(&self) -> Vec<&String> {
        self.named(|v| match v {
            ParamViolation::Unknown(name) => Some(name),
            ParamViolation::Missing(_) => None,
        })
    }

    fn missing(&self) -> Vec<&String> {
        self.named(|v| match v {
            ParamViolation::Missing(name) => Some(name),
            ParamViolation::Unknown(_) => None,
        })
    }

    /// The sentence a caller reads, with `subject` naming what was checked (a tool name, or
    /// a row of a plan). It names every offending property and then lists what the schema
    /// accepts, so one turn is enough to correct the call.
    pub fn message(&self, subject: &str) -> String {
        let mut parts = Vec::new();
        let unknown = self.unknown();
        let missing = self.missing();
        if !unknown.is_empty() {
            parts.push(format!(
                "{subject} has no {} {}",
                join(&unknown),
                pluralize_property(unknown.len())
            ));
        }
        if !missing.is_empty() {
            // allowed-pluralize-noun: `needs` is the verb and what follows is a list of property names, never a count plus a noun
            parts.push(format!("{subject} needs {}", join(&missing)));
        }
        format!("{}. It takes {}.", parts.join(", and "), list_names(&self.accepted))
    }

    /// The same facts as typed data, carried on the error's `data` member so a caller acts on
    /// the shape rather than parsing the sentence.
    pub fn detail(&self) -> Value {
        serde_json::json!({
            "unknownProperties": self.unknown(),
            "missingProperties": self.missing(),
            "accepted": self.accepted,
        })
    }
}

/// Check one object against one level of `schema`. See the module docs for what this
/// deliberately does not look at.
///
/// A schema that isn't an object schema, or that declares no `properties`, accepts
/// everything: there is nothing authored to check against, and inventing a rule there would
/// refuse calls that work.
pub fn check_object(schema: &Value, value: &Value) -> ParamProblems {
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return ParamProblems::default();
    };
    let accepted: Vec<String> = properties.keys().cloned().collect();
    // A tool that takes no arguments is called with `null` as often as with `{}`, and both
    // mean the same thing.
    let empty = serde_json::Map::new();
    let given = match value {
        Value::Object(map) => map,
        Value::Null => &empty,
        // A non-object params value is the transport's problem, not the schema's: dispatch
        // hands it to the handler, which reads nothing out of it and says so.
        _ => return ParamProblems::default(),
    };

    let mut violations = Vec::new();
    if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
        violations.extend(
            given
                .keys()
                .filter(|key| !properties.contains_key(*key))
                .map(|key| ParamViolation::Unknown(key.clone())),
        );
    }
    violations.extend(
        schema
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|name| !given.contains_key(*name))
            .map(|name| ParamViolation::Missing(name.to_string())),
    );
    ParamProblems { violations, accepted }
}

/// The gate as the dispatch paths use it: a params object checked against its tool's own
/// declared schema, refused with a message the caller can act on in one turn.
pub fn gate(tool: &str, schema: &Value, params: &Value) -> Result<(), ToolError> {
    let problems = check_object(schema, params);
    if problems.is_empty() {
        return Ok(());
    }
    Err(ToolError::invalid_params(problems.message(tool)).with_data(problems.detail()))
}

fn join(names: &[&String]) -> String {
    list_names(&names.iter().map(|n| (*n).clone()).collect::<Vec<_>>())
}

/// `a`, `a and b`, `a, b, and c`: the house Oxford comma, since these sentences are read by
/// people as often as by models. Shared with the callers that build their own refusal out of
/// [`check_object`] (the rename boundary), so one list never reads differently from another.
pub fn list_names(names: &[String]) -> String {
    match names {
        [] => "nothing".to_string(),
        [one] => one.clone(),
        [first, second] => format!("{first} and {second}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

fn pluralize_property(count: usize) -> &'static str {
    if count == 1 { "parameter" } else { "parameters" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn closed() -> Value {
        json!({
            "type": "object",
            "properties": { "path": { "type": "string" }, "limit": { "type": "integer" } },
            "required": ["path"],
            "additionalProperties": false
        })
    }

    #[test]
    fn a_closed_schema_names_every_undeclared_property_and_what_it_accepts() {
        let problems = check_object(&closed(), &json!({ "path": "/x", "name": "p", "nameMatch": "prefix" }));
        assert_eq!(
            problems.violations,
            vec![
                ParamViolation::Unknown("name".into()),
                ParamViolation::Unknown("nameMatch".into())
            ]
        );
        assert_eq!(
            problems.message("list_dir"),
            "list_dir has no name and nameMatch parameters. It takes limit and path."
        );
    }

    #[test]
    fn an_open_schema_accepts_anything_it_did_not_declare() {
        let mut open = closed();
        open.as_object_mut()
            .expect("object schema")
            .remove("additionalProperties");
        let problems = check_object(&open, &json!({ "path": "/x", "whatever": 1 }));
        assert!(problems.is_empty(), "an open schema stays open: {problems:?}");
    }

    #[test]
    fn a_missing_required_property_is_named() {
        let problems = check_object(&closed(), &json!({ "limit": 5 }));
        assert_eq!(problems.violations, vec![ParamViolation::Missing("path".into())]);
        assert_eq!(
            problems.message("list_dir"),
            "list_dir needs path. It takes limit and path."
        );
    }

    #[test]
    fn both_kinds_of_problem_are_reported_together() {
        // One round trip has to be enough to fix the whole call, so a call that is both
        // missing something and carrying something extra hears about both.
        let problems = check_object(&closed(), &json!({ "name": "penguin" }));
        assert_eq!(
            problems.message("list_dir"),
            "list_dir has no name parameter, and list_dir needs path. It takes limit and path."
        );
    }

    #[test]
    fn a_schema_with_no_properties_map_checks_nothing() {
        // Nothing is authored to check against, and inventing a rule here would refuse calls
        // that work today.
        assert!(check_object(&json!({ "type": "object" }), &json!({ "anything": 1 })).is_empty());
    }

    #[test]
    fn a_tool_that_takes_no_arguments_accepts_null_as_readily_as_an_empty_object() {
        let schema = json!({ "type": "object", "properties": {}, "additionalProperties": false });
        assert!(check_object(&schema, &Value::Null).is_empty());
        assert!(check_object(&schema, &json!({})).is_empty());
    }

    #[test]
    fn three_names_carry_the_oxford_comma() {
        let schema = json!({
            "type": "object",
            "properties": { "a": {}, "b": {}, "c": {} },
            "required": ["a", "b", "c"],
            "additionalProperties": false
        });
        assert_eq!(
            check_object(&schema, &json!({})).message("thing"),
            "thing needs a, b, and c. It takes a, b, and c."
        );
    }
}
