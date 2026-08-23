//! How many tokens one assembled prompt may spend, per model, and the size estimator the
//! whole agent shares.
//!
//! Pure: a table plus arithmetic, no clock, no I/O, no app state. The caller resolves the
//! provider, the model, the user's "Chat memory size" choice, and the local server's window
//! (the interactive slot's, see `commands/agent.rs`), hands them to
//! [`resolve_prompt_budget`] as values, and [`context`](super::context) then assembles
//! inside the answer.
//!
//! **Two numbers, one relationship, on purpose asymmetric.** [`resolve_prompt_budget`]
//! answers "how big may the whole prompt get", [`MAX_TOOL_RESULT_TOKENS`] answers "how big
//! may ONE tool result get". The second is derived from [`DEFAULT_PROMPT_TOKEN_BUDGET`] —
//! the conservative default — and NOT from the resolved budget, because a tool handler knows
//! nothing about the model and may be serving an external MCP client with no Ask Cmdr
//! resolution at all. Don't "fix" the asymmetry by deriving the ceiling from the effective
//! budget: a user who picks 200,000 would let a single result claim 100,000, and the same
//! handler would hand that to an MCP client whose window is a tenth of it.
//!
//! **Three sources decide a budget, and the answer says which one did**
//! ([`BudgetSource`]): the user's explicit size, the local server's configured window, or
//! this module's family table, with [`DEFAULT_PROMPT_TOKEN_BUDGET`] as the honest miss path.
//! There is no provider-reported window: no API this app talks to reports one, so a stale
//! table must be VISIBLE (logged with its source) rather than silently authoritative.
//!
//! **Context windows drift** — re-verify the families below against each provider's model
//! docs at release time, the way `agent::pricing` is re-verified. Every window listed is a
//! floor ("at least this big"), the budget derived from it sits far below it, and an unknown
//! model falls back to [`DEFAULT_PROMPT_TOKEN_BUDGET`] rather than guessing high.

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

/// The cap on any family's budget: what a known cloud family whose window is 128,000 or more
/// gets. Far below those windows on purpose, since a prompt this size costs real money per
/// call and dilutes the model's attention. It still holds a 200-row folder listing plus a
/// full `image_facts` batch with room to spare, which is what the interactive slot actually
/// needs.
pub const PROMPT_BUDGET_60K: usize = 60_000;

/// The share of a context window one prompt may claim, in percent. The rest is headroom for
/// the reply, which comes out of the same window.
const PROMPT_BUDGET_WINDOW_PERCENT: usize = 60;

/// The smallest local context window Ask Cmdr can run a turn in
/// (`ai.localContextSize`). Below it the numbers don't work: every call pays
/// [`FIXED_PROMPT_OVERHEAD_TOKENS`] before the user has said a word, and one paged tool
/// result can spend [`MAX_TOOL_RESULT_TOKENS`] more, so a 4,096-token window (an earlier
/// shipped default) left 2,457 tokens for a 3,124-token prefix: not one working turn. The
/// setting offers nothing smaller, and a window that still comes in under this is refused
/// honestly ([`BudgetRefusal`]) rather than assembled against.
pub const MIN_LOCAL_CONTEXT_TOKENS: u32 = 16_384;

/// The most estimated tokens ONE tool result may spend on its items: half of
/// [`DEFAULT_PROMPT_TOKEN_BUDGET`], expressed as a fraction so the two numbers can't drift
/// apart. A result at the ceiling still leaves the conversation around it room to fit.
///
/// Deliberately NOT derived from the resolved budget — see this module's header for why the
/// asymmetry is load-bearing.
///
/// A tool over the ceiling returns FEWER items plus `returned` / `total` /
/// `truncated: true`, never a silent cut — see `mcp::executor::fit_to_result_budget`.
pub const MAX_TOOL_RESULT_TOKENS: usize = DEFAULT_PROMPT_TOKEN_BUDGET / 2;

/// A single tool result must leave the conversation around it room to fit. Checked at
/// COMPILE time, so a future edit to either number can't quietly invert the relationship.
const _: () = assert!(MAX_TOOL_RESULT_TOKENS < DEFAULT_PROMPT_TOKEN_BUDGET);

/// What every call pays before the user has said a word: the system prompt plus the tool
/// declarations. Measured against the shipped assets, and pinned there —
/// `context/cost_tests.rs` fails if the real prefix drifts away from this figure. The system
/// prompt is 1,371 of it, the 14 tool declarations the rest.
///
/// It grows whenever a tool joins the view, and every call pays it whether or not it uses
/// that tool: the suggested-ops trio cost about 1,100 tokens of schema between them, which
/// is roughly four files off a 16,000-token rename batch. Keep a new schema terse.
pub const FIXED_PROMPT_OVERHEAD_TOKENS: usize = 4_972;

/// What one `image_facts` row costs at the corpus' average OCR length.
pub const IMAGE_FACTS_TOKENS_PER_FILE: usize = 269;

/// What one plan row costs: source path, proposed name, and evidence. This is also what the
/// row costs as OUTPUT when the model emits it, which is what [`files_per_batch`] divides the
/// completion slot by.
pub const PLAN_ROW_TOKENS_PER_FILE: usize = 59;

/// What one pane-listing entry costs.
pub const LISTING_TOKENS_PER_FILE: usize = 21;

/// What one file costs a content-based rename, all in. Summed from the parts rather than
/// written as a total, so a re-measured part can't leave the total behind. Every part is
/// pinned against the real shapes in `context/cost_tests.rs`.
pub const RENAME_TOKENS_PER_FILE: usize =
    IMAGE_FACTS_TOKENS_PER_FILE + PLAN_ROW_TOKENS_PER_FILE + LISTING_TOKENS_PER_FILE;

/// Per-call output room, shared by reasoning tokens, visible text, and tool calls.
pub const AGENT_MAX_OUTPUT_TOKENS: u32 = 12_000;

/// How much of [`AGENT_MAX_OUTPUT_TOKENS`] a batch hint leaves for the model's reasoning and
/// its accompanying sentence, rather than for plan rows.
///
/// Half the slot, because we cannot see how much a reasoning model will spend before it starts
/// emitting: `ai::client` already retries with a raised ceiling when reasoning consumes the
/// whole budget, so an exhausted slot is an observed failure here, not a hypothetical. Erring
/// large costs a smaller batch; erring small costs the entire plan, cut off mid-JSON.
const REASONING_RESERVE_TOKENS: usize = AGENT_MAX_OUTPUT_TOKENS as usize / 2;

/// The share of a budget [`files_per_batch`] leaves unclaimed. The measured 100-file turn
/// came in about 4% above what the per-file costs account for (the paths the calls name, the
/// envelope, the user's own sentence, JSON scaffolding), so a hint that spent the whole
/// budget would put a batch just over it.
const BATCH_HINT_HEADROOM_PERCENT: usize = 10;

/// How many files one content-based rename batch fits, as the smaller of two limits:
///
/// - what the PROMPT holds: `(budget − overhead) / per-file cost`, less
///   [`BATCH_HINT_HEADROOM_PERCENT`];
/// - what one REPLY can emit: the completion slot, less [`REASONING_RESERVE_TOKENS`], divided
///   by [`PLAN_ROW_TOKENS_PER_FILE`].
///
/// **Both limits are load-bearing, and the second binds first on a large budget.** The number
/// is advertised to the model as "propose this many files", and the model answers by emitting
/// that many plan rows, so a hint past the completion slot doesn't degrade gracefully: the
/// reply is cut off mid-JSON and the whole plan is lost. A 60,000-token budget holds 145 files
/// comfortably in the prompt and can only get about 101 of them back.
///
/// This arithmetic lives HERE, next to the budget it derives from, and not in the system
/// prompt or the UI: those render the number, they don't own it. `0` is a meaningful answer
/// (a budget that can't even hold the prefix cannot do a batch at all).
pub fn files_per_batch(prompt_tokens: usize) -> usize {
    let assembly_ceiling = prompt_tokens * (100 - BATCH_HINT_HEADROOM_PERCENT) / 100;
    let prompt_fits = assembly_ceiling.saturating_sub(FIXED_PROMPT_OVERHEAD_TOKENS) / RENAME_TOKENS_PER_FILE;
    let reply_fits =
        (AGENT_MAX_OUTPUT_TOKENS as usize).saturating_sub(REASONING_RESERVE_TOKENS) / PLAN_ROW_TOKENS_PER_FILE;
    prompt_fits.min(reply_fits)
}

/// What share of a turn's prompt budget a wake's DIGEST may claim.
///
/// A fifth: the digest opens the turn, and everything after it (the envelope, the tool results
/// the agent pulls once it is awake, its own reasoning) has to fit in the same window. The
/// compactor spends whatever it gets on the highest-interest folders first and rolls the rest
/// up, so a smaller share costs detail rather than correctness.
const WAKE_DIGEST_BUDGET_PERCENT: usize = 20;

/// What a wake's digest may spend, out of the turn's resolved prompt budget.
///
/// Derived rather than a constant, for the same reason [`files_per_batch`] is: a user on a
/// small local window must not have the digest alone push their tool results out of the
/// prompt. `0` is a meaningful answer — a budget that cannot hold the prefix cannot hold a
/// digest either, and the wake then stays quiet rather than opening a thread it can say
/// nothing in.
pub fn wake_digest_budget(prompt_tokens: usize) -> usize {
    prompt_tokens.saturating_sub(FIXED_PROMPT_OVERHEAD_TOKENS) * WAKE_DIGEST_BUDGET_PERCENT / 100
}

/// One family's context window, matched by model-id prefix (longest first, like
/// `agent::pricing`'s table, so a more specific id wins over the family it shares a stem
/// with). The budget is DERIVED from the window ([`budget_for_window`]) rather than stored
/// beside it, so a family can't carry a budget its window couldn't hold.
struct ModelFamily {
    prefix: &'static str,
    /// At least this many tokens. Verify at release time; drift upward is harmless.
    window_tokens: usize,
}

/// What one prompt may claim inside `window_tokens`: [`PROMPT_BUDGET_WINDOW_PERCENT`] of it,
/// capped at [`PROMPT_BUDGET_60K`]. The rest of the window is the reply's.
fn budget_for_window(window_tokens: usize) -> usize {
    (window_tokens * PROMPT_BUDGET_WINDOW_PERCENT / 100).min(PROMPT_BUDGET_60K)
}

/// Claude families all carry 200,000-token windows or more.
const ANTHROPIC_FAMILIES: &[ModelFamily] = &[ModelFamily {
    prefix: "claude-",
    window_tokens: 200_000,
}];

/// The OpenAI-compatible families, first-party and third-party alike, because every provider
/// preset except Anthropic and Gemini routes through the OpenAI adapter (`ai::client`) and so
/// arrives here as [`ProviderTag::OpenAi`].
///
/// **The class this table gets wrong when it's short**: a family Cmdr ships a provider preset
/// for, whose window is far above [`PROMPT_BUDGET_60K`], silently taking the conservative
/// default because nobody added a row. `deepseek`, `qwen`, `grok`, and `mistral` were all in
/// that state at once. `unknown_families_take_the_conservative_default` walks every shipped
/// preset's default model, so the next preset added without a row here fails a test instead
/// of quietly costing a user four fifths of their window.
///
/// Anything older and smaller (`gpt-3.5-turbo`) is absent on purpose and takes the
/// conservative default.
const OPENAI_COMPATIBLE_FAMILIES: &[ModelFamily] = &[
    ModelFamily {
        prefix: "gpt-5",
        window_tokens: 400_000,
    },
    ModelFamily {
        prefix: "gpt-4.1",
        window_tokens: 1_000_000,
    },
    ModelFamily {
        prefix: "gpt-4o",
        window_tokens: 128_000,
    },
    ModelFamily {
        prefix: "o3",
        window_tokens: 200_000,
    },
    ModelFamily {
        prefix: "o4-mini",
        window_tokens: 200_000,
    },
    ModelFamily {
        prefix: "deepseek",
        window_tokens: 128_000,
    },
    ModelFamily {
        prefix: "qwen",
        window_tokens: 128_000,
    },
    ModelFamily {
        prefix: "grok",
        window_tokens: 128_000,
    },
    ModelFamily {
        prefix: "mistral",
        window_tokens: 128_000,
    },
    ModelFamily {
        prefix: "ministral",
        window_tokens: 128_000,
    },
    // Llama 3.x and 4 across the hosts that serve them: `llama-3.3-70b-versatile` (Groq),
    // `llama-v3p3-70b-instruct` (Fireworks), `Llama-4-Maverick-…` (Together). Llama 2 is
    // absent on purpose (a 4,096-token window), which is why these prefixes carry the
    // generation.
    ModelFamily {
        prefix: "llama-3",
        window_tokens: 128_000,
    },
    ModelFamily {
        prefix: "llama-4",
        window_tokens: 128_000,
    },
    ModelFamily {
        prefix: "llama-v3",
        window_tokens: 128_000,
    },
    ModelFamily {
        prefix: "sonar",
        window_tokens: 128_000,
    },
];

/// Gemini 2.x carries a 1M-token window across flash and pro.
const GEMINI_FAMILIES: &[ModelFamily] = &[ModelFamily {
    prefix: "gemini-2",
    window_tokens: 1_000_000,
}];

/// Which source decided a budget. Carried out of [`resolve_prompt_budget`] and logged at
/// send, so "60,000 because the table says `claude-` has 200,000" is distinguishable from
/// "60,000 because you asked for it" and from "16,000 because nothing here knows this model".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetSource {
    /// The user's explicit "Chat memory size" choice, used as given.
    UserSetting,
    /// A model family this module recognizes.
    FamilyTable,
    /// The local server's configured context window (`ai.localContextSize`).
    LocalServerWindow,
    /// Nothing recognized the model: [`DEFAULT_PROMPT_TOKEN_BUDGET`].
    Default,
}

impl BudgetSource {
    /// A stable token for the log line (never user-facing copy).
    pub fn label(self) -> &'static str {
        match self {
            Self::UserSetting => "the user's chat-memory setting",
            Self::FamilyTable => "the model-family table",
            Self::LocalServerWindow => "the local server's context window",
            Self::Default => "the conservative default (model not in the family table)",
        }
    }
}

/// Everything a resolution needs, as VALUES: the pure core never reads app state or settings
/// itself (invariant 2), so the command layer gathers these and passes them in.
#[derive(Debug, Clone, Copy)]
pub struct BudgetInputs<'a> {
    pub provider: ProviderTag,
    pub model: &'a str,
    /// The user's explicit "Chat memory size", or `None` for "Automatic (recommended)".
    pub user_choice: Option<usize>,
    /// The local server's configured window (`ai.localContextSize`). Only read for
    /// [`ProviderTag::Local`].
    pub local_context_tokens: u32,
}

/// One resolved budget, plus everything needed to tell the user the truth about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedBudget {
    /// What [`context`](super::context) may assemble inside.
    pub prompt_tokens: usize,
    pub source: BudgetSource,
    /// The window we believe this model has, when any source knows it. `None` means nothing
    /// here knows, so no claim about the window can honestly be made.
    pub known_window_tokens: Option<usize>,
    /// The user's explicit size is larger than [`Self::known_window_tokens`]. It is used
    /// ANYWAY: this table will be wrong sometimes and the user may be right about their own
    /// model. The settings surface warns that the model may refuse a message this long.
    pub over_known_window: bool,
}

/// The one case a turn is refused instead of assembled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetRefusal {
    /// A local server running with a window smaller than [`MIN_LOCAL_CONTEXT_TOKENS`]: the
    /// prefix alone wouldn't fit, so there is no honest budget to assemble against. The
    /// setting can't produce this any more, but a server Cmdr didn't launch at the current
    /// setting still can, and pretending otherwise would send a prompt the server rejects.
    LocalWindowBelowFloor { window_tokens: u32, floor_tokens: u32 },
}

/// The window we believe a slot's model has: the local server's configured window, else the
/// family table, else nothing. There is no provider-reported window to consult.
pub fn known_window_tokens(provider: ProviderTag, model: &str, local_context_tokens: u32) -> Option<usize> {
    if provider == ProviderTag::Local {
        return Some(local_context_tokens as usize);
    }
    let table = match provider {
        ProviderTag::Anthropic => ANTHROPIC_FAMILIES,
        // The Responses API is the same OpenAI catalog with the same windows.
        ProviderTag::OpenAi | ProviderTag::OpenAiResponses => OPENAI_COMPATIBLE_FAMILIES,
        ProviderTag::Gemini => GEMINI_FAMILIES,
        ProviderTag::Local => unreachable!("handled above"),
    };
    let id = normalized_model_id(model);
    table.iter().find(|f| id.starts_with(f.prefix)).map(|f| f.window_tokens)
}

/// A model id reduced to what the family table matches on: lowercased, and stripped of any
/// gateway path prefix (`openai/gpt-4.1-mini` on OpenRouter,
/// `accounts/fireworks/models/llama-v3p3-70b-instruct` on Fireworks,
/// `meta-llama/Llama-4-Maverick-…` on Together all name a family the table knows).
fn normalized_model_id(model: &str) -> String {
    model.rsplit('/').next().unwrap_or(model).to_ascii_lowercase()
}

/// The assembled-prompt token budget for one slot, and where the number came from.
///
/// The user's explicit choice wins outright, even above the window we think the model has
/// (warned, never blocked). "Automatic" follows the local server's window for a local model
/// and the family table for a cloud one, and falls back to [`DEFAULT_PROMPT_TOKEN_BUDGET`]
/// for a model nothing here recognizes.
pub fn resolve_prompt_budget(inputs: BudgetInputs<'_>) -> Result<ResolvedBudget, BudgetRefusal> {
    if inputs.provider == ProviderTag::Local && inputs.local_context_tokens < MIN_LOCAL_CONTEXT_TOKENS {
        return Err(BudgetRefusal::LocalWindowBelowFloor {
            window_tokens: inputs.local_context_tokens,
            floor_tokens: MIN_LOCAL_CONTEXT_TOKENS,
        });
    }
    let known_window_tokens = known_window_tokens(inputs.provider, inputs.model, inputs.local_context_tokens);

    if let Some(chosen) = inputs.user_choice {
        return Ok(ResolvedBudget {
            prompt_tokens: chosen,
            source: BudgetSource::UserSetting,
            known_window_tokens,
            over_known_window: known_window_tokens.is_some_and(|window| chosen > window),
        });
    }

    let (prompt_tokens, source) = match (inputs.provider, known_window_tokens) {
        (ProviderTag::Local, Some(window)) => (budget_for_window(window), BudgetSource::LocalServerWindow),
        (_, Some(window)) => (budget_for_window(window), BudgetSource::FamilyTable),
        (_, None) => (DEFAULT_PROMPT_TOKEN_BUDGET, BudgetSource::Default),
    };
    Ok(ResolvedBudget {
        prompt_tokens,
        source,
        known_window_tokens,
        over_known_window: false,
    })
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

    /// A slot on Automatic: the everyday case.
    fn automatic(provider: ProviderTag, model: &str) -> ResolvedBudget {
        resolve_prompt_budget(BudgetInputs {
            provider,
            model,
            user_choice: None,
            local_context_tokens: MIN_LOCAL_CONTEXT_TOKENS,
        })
        .expect("a cloud slot is never refused")
    }

    #[test]
    fn automatic_follows_the_family_table() {
        for (provider, model) in [
            (ProviderTag::Anthropic, "claude-sonnet-4-5"),
            (ProviderTag::OpenAi, "gpt-4o-mini"),
            (ProviderTag::OpenAi, "gpt-4.1"),
            (ProviderTag::OpenAiResponses, "gpt-5"),
            (ProviderTag::OpenAi, "o4-mini"),
            (ProviderTag::Gemini, "gemini-2.5-flash"),
        ] {
            let resolved = automatic(provider, model);
            assert_eq!(
                (resolved.prompt_tokens, resolved.source),
                (PROMPT_BUDGET_60K, BudgetSource::FamilyTable),
                "the 60k budget covers {model}, from the table"
            );
        }
    }

    /// Every model Cmdr ships as a provider preset's default, from
    /// `lib/settings/cloud-providers.ts`. A cloud family belongs in the table; a
    /// local-endpoint preset (Ollama, LM Studio) or an unconfigured Custom endpoint does NOT,
    /// because its window is whatever the user's own server was started with and guessing
    /// high there is a rejected prompt.
    #[test]
    fn every_shipped_cloud_preset_is_in_the_family_table() {
        for model in [
            "gpt-4.1-mini",                                      // OpenAI, Azure OpenAI
            "claude-sonnet-4-5",                                 // Anthropic
            "gemini-2.5-flash",                                  // Google Gemini
            "llama-3.3-70b-versatile",                           // Groq
            "meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8", // Together AI
            "accounts/fireworks/models/llama-v3p3-70b-instruct", // Fireworks AI
            "mistral-small-latest",                              // Mistral AI
            "openai/gpt-4.1-mini",                               // OpenRouter
            "deepseek-chat",                                     // DeepSeek
            "qwen-plus",                                         // Qwen
            "grok-3-mini-fast",                                  // xAI
            "sonar",                                             // Perplexity
        ] {
            let provider = match model {
                m if m.starts_with("claude-") => ProviderTag::Anthropic,
                m if m.starts_with("gemini-") => ProviderTag::Gemini,
                _ => ProviderTag::OpenAi,
            };
            let resolved = automatic(provider, model);
            assert_eq!(
                resolved.source,
                BudgetSource::FamilyTable,
                "a provider preset defaults to {model}, so the table must know its window; \
                 without a row it silently takes a {DEFAULT_PROMPT_TOKEN_BUDGET}-token slice of a far larger window"
            );
            assert_eq!(
                resolved.prompt_tokens, PROMPT_BUDGET_60K,
                "the table gives {model} a window of at least 128,000"
            );
        }
    }

    #[test]
    fn an_unknown_model_falls_back_to_the_conservative_default() {
        // Guessing high on an unknown slug is a hard provider rejection mid-turn, so the
        // miss path must be the SMALL number, never the large one.
        for model in [
            "some-future-model-9000",
            "gpt-3.5-turbo", // a small-window family, absent from the table on purpose
            "llama3.2",      // Ollama's default: a window only the user's own server knows
            "loaded-model",  // LM Studio's placeholder id
            "",              // Custom, unconfigured
        ] {
            let resolved = automatic(ProviderTag::OpenAi, model);
            assert_eq!(
                (resolved.prompt_tokens, resolved.source, resolved.known_window_tokens),
                (DEFAULT_PROMPT_TOKEN_BUDGET, BudgetSource::Default, None),
                "{model} is not in the table, so nothing may claim to know its window"
            );
        }
    }

    #[test]
    fn an_explicit_size_is_honoured_and_labelled() {
        let resolved = resolve_prompt_budget(BudgetInputs {
            provider: ProviderTag::Anthropic,
            model: "claude-sonnet-4-5",
            user_choice: Some(32_000),
            local_context_tokens: 0,
        })
        .expect("a cloud slot is never refused");
        assert_eq!(
            (resolved.prompt_tokens, resolved.source),
            (32_000, BudgetSource::UserSetting),
            "the user's choice wins over the table, in both directions"
        );
        assert!(!resolved.over_known_window, "32,000 fits a 200,000-token window");
    }

    #[test]
    fn a_size_above_the_known_window_warns_and_is_still_used() {
        let resolved = resolve_prompt_budget(BudgetInputs {
            provider: ProviderTag::OpenAi,
            model: "gpt-4o", // 128,000
            user_choice: Some(200_000),
            local_context_tokens: 0,
        })
        .expect("a cloud slot is never refused");
        // Never blocked: our table will be wrong sometimes and the user may be right about
        // their own model. The UI names the consequence instead.
        assert_eq!(resolved.prompt_tokens, 200_000);
        assert_eq!(resolved.source, BudgetSource::UserSetting);
        assert_eq!(resolved.known_window_tokens, Some(128_000));
        assert!(resolved.over_known_window);
    }

    #[test]
    fn an_unknown_model_cannot_be_over_its_window() {
        // Nothing knows the window, so there is nothing to warn about — silence beats a
        // guess dressed as a warning.
        let resolved = resolve_prompt_budget(BudgetInputs {
            provider: ProviderTag::OpenAi,
            model: "some-future-model-9000",
            user_choice: Some(200_000),
            local_context_tokens: 0,
        })
        .expect("a cloud slot is never refused");
        assert!(!resolved.over_known_window);
        assert_eq!(resolved.known_window_tokens, None);
    }

    #[test]
    fn a_local_budget_tracks_the_configured_context_window() {
        let resolved = resolve_prompt_budget(BudgetInputs {
            provider: ProviderTag::Local,
            model: "ministral-3b",
            user_choice: None,
            local_context_tokens: 32_768,
        })
        .expect("a window at or above the floor resolves");
        assert_eq!(
            (resolved.prompt_tokens, resolved.source),
            (19_660, BudgetSource::LocalServerWindow),
            "60% of the window, so the reply keeps room in the same window"
        );
        assert_eq!(resolved.known_window_tokens, Some(32_768));
        // The local provider never reads the cloud tables, whatever the model is called.
        assert_eq!(
            automatic(ProviderTag::Local, "claude-sonnet-4-5").prompt_tokens,
            budget_for_window(MIN_LOCAL_CONTEXT_TOKENS as usize)
        );
    }

    #[test]
    fn a_local_window_under_the_floor_is_refused_honestly() {
        // The setting can't produce this any more, but a server started with a smaller
        // window still can. Assembling anyway would send a prompt the server must reject.
        let refusal = resolve_prompt_budget(BudgetInputs {
            provider: ProviderTag::Local,
            model: "ministral-3b",
            user_choice: None,
            local_context_tokens: 4_096,
        })
        .expect_err("a window under the floor cannot hold one turn");
        assert_eq!(
            refusal,
            BudgetRefusal::LocalWindowBelowFloor {
                window_tokens: 4_096,
                floor_tokens: MIN_LOCAL_CONTEXT_TOKENS,
            }
        );
        // An explicit size can't talk a too-small server into working either.
        assert!(
            resolve_prompt_budget(BudgetInputs {
                provider: ProviderTag::Local,
                model: "ministral-3b",
                user_choice: Some(60_000),
                local_context_tokens: 4_096,
            })
            .is_err(),
            "the window is the server's, not the setting's, so the refusal stands"
        );
    }

    #[test]
    fn the_floor_leaves_room_for_a_prefix_and_a_paged_result() {
        let floored = budget_for_window(MIN_LOCAL_CONTEXT_TOKENS as usize);
        assert!(
            floored > FIXED_PROMPT_OVERHEAD_TOKENS + MAX_TOOL_RESULT_TOKENS / 2,
            "the floor exists so a local user gets a working turn: a budget of {floored} against \
             a prefix of {FIXED_PROMPT_OVERHEAD_TOKENS}"
        );
        assert!(
            files_per_batch(floored) > 0,
            "a local user on the floor can rename a batch"
        );
    }

    #[test]
    fn a_batch_hint_derives_from_the_budget() {
        // (budget − 10% headroom − 4,972 of prefix) / 349 per file, while the prompt is what
        // binds.
        assert_eq!(files_per_batch(16_000), 27);
        assert_eq!(files_per_batch(32_000), 68);
        // Past roughly 45,000 the reply's own ceiling binds instead, and the hint stops
        // growing with the budget: 6,000 emittable tokens / 59 per row.
        assert_eq!(files_per_batch(60_000), 101);
        assert_eq!(
            files_per_batch(200_000),
            101,
            "a huge window still gets one reply's worth"
        );
        // A budget that can't even hold the prefix says so, rather than proposing one file.
        assert_eq!(files_per_batch(2_000), 0);
    }

    /// The hint is advertised to the model as "propose this many files", and the model answers
    /// by EMITTING that many plan rows. So a hint the completion slot can't hold doesn't
    /// degrade, it truncates: the model is cut off mid-JSON and the whole plan is lost, which
    /// is worse than a smaller batch. Reasoning tokens come out of the same slot, so the cap
    /// has to leave room for them.
    #[test]
    fn a_batch_hint_never_exceeds_what_one_reply_can_emit() {
        for budget in [16_000, 32_000, 60_000, 128_000, 200_000] {
            let rows = files_per_batch(budget);
            let plan_output = rows * PLAN_ROW_TOKENS_PER_FILE;
            assert!(
                plan_output <= AGENT_MAX_OUTPUT_TOKENS as usize - REASONING_RESERVE_TOKENS,
                "a {budget}-token budget advertises a batch of {rows}, needing output of {plan_output}, \
                 past what one reply can emit alongside its reasoning"
            );
        }
    }

    #[test]
    fn estimates_agree_across_the_shapes_they_measure() {
        assert_eq!(estimate_tokens_str("abcd"), 1);
        assert_eq!(estimate_tokens_str("abcde"), 2, "partial tokens round up");
        let value = serde_json::json!({ "a": "bc" });
        assert_eq!(estimate_tokens_of_value(&value), estimate_serialized_tokens(&value));
    }
}
