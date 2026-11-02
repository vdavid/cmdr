//! The tool-search pair: how the agent reaches a tool it was only told about in one line.
//!
//! Every provider call pays for every schema it declares, so the agent declares a small core
//! ([`Load::Core`]) and reads the rest off the prompt ([`deferred_tool_lines`]).
//! [`tool_details`](execute_tool_details) hands over a schema as an ordinary result, so a thread pays
//! only for what it uses, and `use_tool` runs one.
//!
//! **`use_tool` never runs here.** The runtime unwraps a well-formed call before anything reads it
//! ([`AgentToolCall::effective`](crate::agent::llm::types::AgentToolCall::effective)), so `admit`, the
//! schema gate, Stop, the repeat breaker, the evidence ledgers, and the rail all see the tool it names.
//! What reaches [`execute_use_tool`] is a call that didn't unwrap, and all it can do is say why.
//!
//! The pair is the in-process agent's: `get_all_tools` omits [`Load::Meta`], and `execute_tool` refuses
//! it for an outside client, whose `tools/list` already carries every schema.
//!
//! [`Load::Core`]: super::Load::Core
//! [`Load::Meta`]: super::Load::Meta
//! [`deferred_tool_lines`]: super::deferred_tool_lines

use serde_json::{Value, json};

use super::policy::{Verdict, admit};
use super::table::{table_tools, tool_access, tool_load};
use super::{Caller, Load, ToolError, ToolId, ToolResult, fit_to_result_budget};
use crate::agent::types::TurnOrigin;

/// The one thing `use_tool` declares beyond [`NAME`] is [`PARAMS`], and anything else means the call
/// asked for more than it would get.
const NAME: &str = "name";
const PARAMS: &str = "params";

/// `name` is the tool, and `params` is free-form: what it holds is the named tool's own schema's
/// business, which the schema gate checks once the call is unwrapped.
///
/// ⚠️ `params` needs a `type`. llama-server refuses a whole request over a property that has none
/// ("JSON schema conversion failed: Unrecognized schema", verified on Cmdr's bundled llama-server
/// 0.9.5, live smoke, 2026-11-02), so Cmdr's own local model would answer nothing at all. The union
/// says what [`use_tool_target`] accepts, stays out of the closed-schema walk's way, and Anthropic,
/// OpenAI, and Gemini take it (the same smokes).
pub fn use_tool_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            NAME: { "type": "string", "description": "The tool to run." },
            PARAMS: {
                "type": ["object", "null"],
                "description": "That tool's arguments, as a JSON object. Leave it out when it takes none.",
            },
        },
        "required": [NAME],
        "additionalProperties": false,
    })
}

pub fn tool_details_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "names": {
                "type": "array",
                "items": { "type": "string" },
                "description": "The tools to read about.",
            },
        },
        "required": ["names"],
        "additionalProperties": false,
    })
}

/// Why a `use_tool` call doesn't name a tool to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UseToolProblem {
    /// `name` is absent, blank, or not a string.
    NoName,
    /// `name` is `use_tool` or `tool_details`: nesting would hide the call from everything that reads
    /// the effective one.
    Nested(String),
    /// `params` is something other than a JSON object.
    ParamsNotAnObject,
    /// A key beside `name` and `params`. Unwrapping would drop it and act on less than was asked, so
    /// the call stays `use_tool` and the schema gate refuses it by name.
    StrayArgument,
}

impl UseToolProblem {
    fn into_error(self) -> ToolError {
        let message = match self {
            UseToolProblem::NoName | UseToolProblem::StrayArgument => {
                "use_tool takes name, the tool to run, and params, that tool's arguments.".to_string()
            }
            UseToolProblem::Nested(name) => {
                format!("use_tool runs a tool from your More tools list, and {name} isn't one. Call {name} directly.")
            }
            UseToolProblem::ParamsNotAnObject => {
                "use_tool's params must be a JSON object holding the tool's own arguments.".to_string()
            }
        };
        ToolError::invalid_params(message)
    }
}

/// What a `use_tool` call's arguments ask to run: the tool, and its own arguments. Absent `params` and
/// `null` both mean none, which is how a tool that takes nothing gets called.
pub fn use_tool_target(arguments: &Value) -> Result<(ToolId, Value), UseToolProblem> {
    let Some(arguments) = arguments.as_object() else {
        return Err(UseToolProblem::NoName);
    };
    if arguments.keys().any(|key| key != NAME && key != PARAMS) {
        return Err(UseToolProblem::StrayArgument);
    }
    let name = arguments
        .get(NAME)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or(UseToolProblem::NoName)?;
    if matches!(tool_load(name), Some(Load::Meta(_))) {
        return Err(UseToolProblem::Nested(name.to_string()));
    }
    let params = match arguments.get(PARAMS) {
        None | Some(Value::Null) => json!({}),
        Some(params @ Value::Object(_)) => params.clone(),
        Some(_) => return Err(UseToolProblem::ParamsNotAnObject),
    };
    Ok((ToolId::from_wire_name(name), params))
}

/// A `use_tool` call the runtime couldn't unwrap, saying why. A well-formed one never gets here, and
/// refuses if it does: running it from here would run it past the gate that reads the effective call.
pub async fn execute_use_tool(params: &Value) -> ToolResult {
    match use_tool_target(params) {
        Err(problem) => Err(problem.into_error()),
        Ok((tool, _)) => Err(ToolError::invalid_params(format!(
            "{} has to go through a turn.",
            tool.as_wire_name()
        ))),
    }
}

/// The full description and schema of each named tool, in the order asked for, under the one-result
/// budget. A name outside the table and a tool the agent can never run each say so in their own list,
/// so a typo reads differently from a refusal, and nothing reads as a tool that isn't there.
pub async fn execute_tool_details(params: &Value) -> ToolResult {
    let Some(asked) = params.get("names").and_then(Value::as_array) else {
        return Err(ToolError::invalid_params(
            "tool_details takes names, a list of tool names.",
        ));
    };
    let mut names: Vec<&str> = Vec::with_capacity(asked.len());
    for name in asked {
        let Some(name) = name.as_str() else {
            return Err(ToolError::invalid_params(
                "tool_details takes names, a list of tool names.",
            ));
        };
        if !names.contains(&name) {
            names.push(name);
        }
    }

    let mut table = table_tools();
    let mut found = Vec::with_capacity(names.len());
    let mut unknown = Vec::new();
    let mut unavailable = Vec::new();
    for name in names {
        let Some(access) = tool_access(name) else {
            unknown.push(name);
            continue;
        };
        if let Verdict::Refuse(refusal) = admit(user_started_agent(), access) {
            unavailable.push(json!({ "name": name, "refusal": refusal.token(), "reason": refusal.message(name) }));
            continue;
        }
        let Some(at) = table.iter().position(|tool| tool.name == name) else {
            unknown.push(name);
            continue;
        };
        let tool = table.swap_remove(at);
        found.push(json!({
            "name": tool.name,
            "description": tool.description,
            "inputSchema": tool.input_schema,
        }));
    }

    let fitted = fit_to_result_budget(found);
    let mut answer = json!({
        "tools": fitted.items,
        "returned": fitted.items.len(),
        "total": fitted.total,
        "truncated": fitted.truncated,
    });
    if !unknown.is_empty() {
        answer["unknown"] = json!(unknown);
    }
    if !unavailable.is_empty() {
        answer["unavailable"] = json!(unavailable);
    }
    Ok(answer)
}

/// What the prefix and the pair answer for: the agent on a turn someone asked for, whatever turn this
/// call serves. A background turn hears `refusedInBackground` from the gate when it tries the tool, and
/// the prefix stays byte-identical across a thread's turns.
pub(super) fn user_started_agent() -> Caller {
    Caller::Agent {
        origin: TurnOrigin::Text,
    }
}
