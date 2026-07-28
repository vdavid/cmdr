//! How many tokens one assembled prompt may spend, per model, and the size estimator the
//! whole agent shares.
//!
//! Pure: a table plus arithmetic, no clock, no I/O, no app state. The caller resolves the
//! provider and model (the interactive slot's, see `commands/agent.rs`) and asks here for
//! the budget; [`context`](super::context) then assembles inside it.
//!
//! **Two numbers, one relationship.** [`prompt_budget`] answers "how big may the whole
//! prompt get", [`MAX_TOOL_RESULT_TOKENS`] answers "how big may ONE tool result get". The
//! second is derived from the CONSERVATIVE default, not from the resolved model, so a tool
//! handler (which knows nothing about the model, and may be serving an external MCP client
//! instead) can cap itself against a number that holds on every budget we ever assemble
//! against.
//!
//! **Context windows drift** — re-verify the families below against each provider's model
//! docs at release time, the way `agent::pricing` is re-verified. The budgets sit far below
//! every listed family's window, so drift is only ever safe in the direction that matters,
//! and an unknown model falls back to [`DEFAULT_PROMPT_TOKEN_BUDGET`] rather than guessing
//! high.

use serde::Serialize;
use serde_json::Value;

use crate::agent::llm::types::ProviderTag;

/// Rough characters-per-token divisor for every size estimate in the agent: what drives
/// elision, the elision stub's hint, and each tool's self-cap. A heuristic, not a real
/// tokenizer, and deliberately the ONE divisor, so the estimates can't disagree with each
/// other.
pub const CHARS_PER_TOKEN_ESTIMATE: usize = 4;

/// The budget for a model we don't recognize, and the floor the tool-result ceiling is
/// derived from. Conservative on purpose: it must fit a model with a small window, since
/// guessing high turns into a hard provider rejection mid-turn.
pub const DEFAULT_PROMPT_TOKEN_BUDGET: usize = 16_000;

/// The budget for a known cloud family with a large window (128k and up). Far below the
/// window: a prompt this size costs real money per call and dilutes the model's attention.
/// It still holds a 200-row folder listing plus a full `image_facts` batch with room to
/// spare, which is what the interactive slot actually needs.
pub const LARGE_CONTEXT_PROMPT_BUDGET: usize = 60_000;

/// The share of a local server's configured context window one prompt may claim, in
/// percent. The rest is headroom for the reply, which comes out of the same window.
const LOCAL_PROMPT_BUDGET_PERCENT: usize = 60;

/// The floor under a local budget, so a tiny configured window still leaves the assembly
/// something to work with (it will report itself over budget, honestly, rather than
/// assembling against ~0).
const MIN_LOCAL_PROMPT_BUDGET: usize = 2_000;

/// The most estimated tokens ONE tool result may spend on its items: half of
/// [`DEFAULT_PROMPT_TOKEN_BUDGET`], expressed as a fraction so the two numbers can't drift
/// apart. A result at the ceiling still leaves the conversation around it room to fit.
///
/// A tool over the ceiling returns FEWER items plus `returned` / `total` /
/// `truncated: true`, never a silent cut — see `mcp::executor::fit_to_result_budget`.
pub const MAX_TOOL_RESULT_TOKENS: usize = DEFAULT_PROMPT_TOKEN_BUDGET / 2;

/// A single tool result must leave the conversation around it room to fit. Checked at
/// COMPILE time, so a future edit to either number can't quietly invert the relationship.
const _: () = assert!(MAX_TOOL_RESULT_TOKENS < DEFAULT_PROMPT_TOKEN_BUDGET);

/// One family's prompt budget, matched by model-id prefix (longest first, like
/// `agent::pricing`'s table, so a more specific id wins over the family it shares a stem
/// with).
struct FamilyBudget {
    prefix: &'static str,
    prompt_budget: usize,
}

/// Claude families all carry 200k-token windows or more.
const ANTHROPIC_BUDGETS: &[FamilyBudget] = &[FamilyBudget {
    prefix: "claude-",
    prompt_budget: LARGE_CONTEXT_PROMPT_BUDGET,
}];

/// The large-window OpenAI families (`gpt-4o` 128k, `gpt-4.1` and `gpt-5` far more, the
/// `o*` reasoning models 200k). Anything older and smaller (`gpt-3.5-turbo`) is absent on
/// purpose and takes the conservative default.
const OPENAI_BUDGETS: &[FamilyBudget] = &[
    FamilyBudget {
        prefix: "gpt-5",
        prompt_budget: LARGE_CONTEXT_PROMPT_BUDGET,
    },
    FamilyBudget {
        prefix: "gpt-4.1",
        prompt_budget: LARGE_CONTEXT_PROMPT_BUDGET,
    },
    FamilyBudget {
        prefix: "gpt-4o",
        prompt_budget: LARGE_CONTEXT_PROMPT_BUDGET,
    },
    FamilyBudget {
        prefix: "o3",
        prompt_budget: LARGE_CONTEXT_PROMPT_BUDGET,
    },
    FamilyBudget {
        prefix: "o4-mini",
        prompt_budget: LARGE_CONTEXT_PROMPT_BUDGET,
    },
];

/// Gemini 2.x carries a 1M-token window across flash and pro.
const GEMINI_BUDGETS: &[FamilyBudget] = &[FamilyBudget {
    prefix: "gemini-2",
    prompt_budget: LARGE_CONTEXT_PROMPT_BUDGET,
}];

/// The assembled-prompt token budget for one provider + model. A known cloud family gets
/// its family budget; anything else (a new slug, a keyless OpenAI-compatible endpoint) gets
/// the conservative default. Local models resolve through
/// [`prompt_budget_for_local_context`] instead, since their window is a user setting the
/// caller reads.
pub fn prompt_budget(provider: ProviderTag, model: &str) -> usize {
    let table = match provider {
        ProviderTag::Anthropic => ANTHROPIC_BUDGETS,
        // The Responses API is the same OpenAI catalog with the same windows.
        ProviderTag::OpenAi | ProviderTag::OpenAiResponses => OPENAI_BUDGETS,
        ProviderTag::Gemini => GEMINI_BUDGETS,
        ProviderTag::Local => return prompt_budget_for_local_context(0),
    };
    table
        .iter()
        .find(|f| model.starts_with(f.prefix))
        .map(|f| f.prompt_budget)
        .unwrap_or(DEFAULT_PROMPT_TOKEN_BUDGET)
}

/// The budget for a local server running with `context_tokens` of context (the
/// `ai.localContextSize` setting): [`LOCAL_PROMPT_BUDGET_PERCENT`] of the window, floored at
/// [`MIN_LOCAL_PROMPT_BUDGET`], so the reply keeps room in the same window.
pub fn prompt_budget_for_local_context(context_tokens: u32) -> usize {
    let share = context_tokens as usize * LOCAL_PROMPT_BUDGET_PERCENT / 100;
    share.max(MIN_LOCAL_PROMPT_BUDGET)
}

/// Estimate a string's token size (chars / [`CHARS_PER_TOKEN_ESTIMATE`]).
pub fn estimate_tokens_str(text: &str) -> usize {
    text.len().div_ceil(CHARS_PER_TOKEN_ESTIMATE)
}

/// Estimate a JSON value's token size as the model would read it (serialized form).
pub fn estimate_tokens_of_value(value: &Value) -> usize {
    estimate_tokens_str(&value.to_string())
}

/// Estimate what one serializable row costs the model, so a tool can page itself against
/// [`MAX_TOOL_RESULT_TOKENS`]. A row that fails to serialize (it won't: these are plain
/// DTOs) counts as free rather than aborting the shaping.
pub fn estimate_serialized_tokens<T: Serialize>(value: &T) -> usize {
    serde_json::to_string(value)
        .map(|json| estimate_tokens_str(&json))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_large_window_families_get_the_large_budget() {
        for (provider, model) in [
            (ProviderTag::Anthropic, "claude-sonnet-4-5"),
            (ProviderTag::OpenAi, "gpt-4o-mini"),
            (ProviderTag::OpenAi, "gpt-4.1"),
            (ProviderTag::OpenAiResponses, "gpt-5"),
            (ProviderTag::OpenAi, "o4-mini"),
            (ProviderTag::Gemini, "gemini-2.5-flash"),
        ] {
            assert_eq!(
                prompt_budget(provider, model),
                LARGE_CONTEXT_PROMPT_BUDGET,
                "{model} is a large-window family"
            );
        }
    }

    #[test]
    fn an_unknown_model_falls_back_to_the_conservative_default() {
        // Guessing high on an unknown slug is a hard provider rejection mid-turn, so the
        // miss path must be the SMALL number, never the large one.
        assert_eq!(
            prompt_budget(ProviderTag::OpenAi, "some-future-model-9000"),
            DEFAULT_PROMPT_TOKEN_BUDGET
        );
        assert_eq!(
            prompt_budget(ProviderTag::OpenAi, "gpt-3.5-turbo"),
            DEFAULT_PROMPT_TOKEN_BUDGET,
            "a small-window family is absent from the table on purpose"
        );
    }

    #[test]
    fn a_local_budget_tracks_the_configured_context_window() {
        assert_eq!(prompt_budget_for_local_context(32_768), 19_660);
        // A tiny window floors instead of collapsing to nothing.
        assert_eq!(prompt_budget_for_local_context(1_024), MIN_LOCAL_PROMPT_BUDGET);
        assert_eq!(prompt_budget_for_local_context(0), MIN_LOCAL_PROMPT_BUDGET);
        // The local provider never reads the cloud tables.
        assert_eq!(
            prompt_budget(ProviderTag::Local, "claude-sonnet-4-5"),
            MIN_LOCAL_PROMPT_BUDGET
        );
    }

    #[test]
    fn estimates_agree_across_the_shapes_they_measure() {
        assert_eq!(estimate_tokens_str("abcd"), 1);
        assert_eq!(estimate_tokens_str("abcde"), 2, "partial tokens round up");
        let value = serde_json::json!({ "a": "bc" });
        assert_eq!(estimate_tokens_of_value(&value), estimate_serialized_tokens(&value));
    }
}
