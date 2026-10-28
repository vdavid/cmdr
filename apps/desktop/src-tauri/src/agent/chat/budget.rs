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
//! **Automatic means the whole window.** A known window resolves to all of it, less the reply's
//! room ([`AGENT_MAX_OUTPUT_TOKENS`]) and a margin for our `chars/4` estimator's error
//! ([`ESTIMATE_SAFETY_PERCENT`]). There is no cap below the window: what a big prompt costs is
//! for compaction to answer (planned, `DETAILS.md` § Resolving one budget), not for a ceiling
//! that hides most of a 1M window from the user.
//!
//! **Four sources decide a budget, and the answer says which one did** ([`BudgetSource`]):
//! the user's explicit size, the local server's configured window, the window the provider
//! itself reports (`ai::model_capabilities`, handed in as a value), or this module's family
//! table, with [`DEFAULT_PROMPT_TOKEN_BUDGET`] as the honest miss path. The table is the
//! fallback for providers that report nothing (OpenAI, DeepSeek, xAI) and for the first send
//! before a report arrives, so a stale table must be VISIBLE (logged with its source) rather
//! than silently authoritative.
//!
//! **Context windows drift** — re-verify the families below against each provider's model
//! docs at release time, the way `agent::pricing` is re-verified. Every window listed is a
//! floor ("at least this big"), and an unknown model falls back to
//! [`DEFAULT_PROMPT_TOKEN_BUDGET`] rather than guessing high.

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

/// The share of what's left of a window, after the reply's room, that the prompt leaves
/// unclaimed, in percent: the margin for [`CHARS_PER_TOKEN_ESTIMATE`]'s error. `chars/4` is
/// close for English prose, but the agent's prompts are mostly JSON, paths, and file names,
/// which real tokenizers split finer (about 3.5 characters a token), so a prompt the estimate
/// puts right at the window would be over it. Overrunning is a hard provider rejection
/// mid-turn; erring here costs a sliver of a large window.
pub const ESTIMATE_SAFETY_PERCENT: usize = 15;

/// The smallest local context window Ask Cmdr can run a turn in
/// (`ai.localContextSize`). Below it the numbers don't work: every call pays
/// [`FIXED_PROMPT_OVERHEAD_TOKENS`] before the user has said a word, the reply claims
/// [`AGENT_MAX_OUTPUT_TOKENS`] of the same window, and one paged tool result can spend
/// [`MAX_TOOL_RESULT_TOKENS`] more. At 32,768 the resolved budget is 17,652, which clears
/// that with room for the conversation itself; one window down, 16,384 resolves to 3,726,
/// which can't even hold the prefix. The setting offers nothing smaller, and a window that
/// still comes in under this is refused honestly ([`BudgetRefusal`]) rather than assembled
/// against.
///
/// ⚠️ **This floor is derived from the prefix, so a tool joining the view can outgrow it.**
/// `the_floor_leaves_room_for_a_prefix_and_a_paged_result` is what catches that; answer it
/// by raising this number or shrinking the prefix, never by loosening the assertion.
pub const MIN_LOCAL_CONTEXT_TOKENS: u32 = 32_768;

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
/// prompt is 2,378 of it, the 21 tool declarations the rest.
///
/// It grows whenever a tool joins the view, and every call pays it whether or not it uses
/// that tool: the four suggested-ops tools cost about 1,300 tokens of schema between them,
/// which is roughly four files off a 16,000-token rename batch, and `propose_suggestions`,
/// the most expensive single declaration, costs 759 even trimmed. Keep a new schema terse.
pub const FIXED_PROMPT_OVERHEAD_TOKENS: usize = 7_107;

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

/// The BASE per-call output room, shared by reasoning tokens, visible text, and tool calls: what a
/// call gets when the model barely thinks. A higher reasoning effort raises the actual ceiling
/// (`effort::reply_room`), and the prompt budget leaves that much of the window free
/// ([`BudgetInputs::reply_tokens`]).
pub const AGENT_MAX_OUTPUT_TOKENS: u32 = 12_000;

/// How much of [`AGENT_MAX_OUTPUT_TOKENS`] a batch hint leaves for the model's reasoning and
/// its accompanying sentence, rather than for plan rows.
///
/// Half the BASE slot, because we cannot see how much a reasoning model will spend before it
/// starts emitting, and an exhausted slot ends the turn unfinished (`UnfinishedReply`; the agent
/// path has no retry). Erring large costs a smaller batch; erring small costs the entire plan,
/// cut off mid-JSON. The batch stays sized on the base slot at every effort: a bigger slot at a
/// higher effort is room the model spends thinking, not room for more rows.
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

/// The most a wake's digest may spend, however large the window. A digest is a summary the
/// agent reads to decide whether anything is worth saying, and it already spends its share on
/// the highest-interest folders first, so past this a bigger window buys a longer tail of
/// low-interest folders at the price of every wake. It is what a 60,000-token budget gave the
/// digest, a size the wake loop was tuned against.
pub const WAKE_DIGEST_MAX_TOKENS: usize = 10_000;

/// What a wake's digest may spend, out of the turn's resolved prompt budget.
///
/// Derived rather than a constant, for the same reason [`files_per_batch`] is: a user on a
/// small local window must not have the digest alone push their tool results out of the
/// prompt. And capped at [`WAKE_DIGEST_MAX_TOKENS`], since a fifth of a 1M window is a digest
/// nobody needs. `0` is a meaningful answer — a budget that cannot hold the prefix cannot
/// hold a digest either, and the wake then stays quiet rather than opening a thread it can
/// say nothing in.
pub fn wake_digest_budget(prompt_tokens: usize) -> usize {
    (prompt_tokens.saturating_sub(FIXED_PROMPT_OVERHEAD_TOKENS) * WAKE_DIGEST_BUDGET_PERCENT / 100)
        .min(WAKE_DIGEST_MAX_TOKENS)
}

/// What share of a turn's prompt budget the agent's own memory may claim.
///
/// A tenth, and a SHARE rather than a byte count, for a reason a flat cap can't cover: memory
/// notes are never elided (`context::assemble_prompt` tightens tool results only), so every
/// byte of memory is a permanent tax on every thread, paid again in each turn whose memory
/// changed (`context::memory_notes_for_turn`). Run the numbers a
/// flat 8 KB would give at the smallest window the app supports: [`MIN_LOCAL_CONTEXT_TOKENS`]
/// resolves to a 17,652-token budget, the fixed overhead takes 7,107 of it, and 8 KB of memory
/// at [`CHARS_PER_TOKEN_ESTIMATE`] takes 2,048 more, so memory alone would claim about a fifth
/// of what is left for the digest, the envelope, the history, and every tool result, and it would
/// claim the same 2,048 on a window a tenth the size. **And the agent writes this file itself**,
/// so a flat cap lets it permanently degrade its own chat with no way back.
const MEMORY_BUDGET_PERCENT: usize = 10;

/// How many BYTES of memory this turn carries, out of its resolved prompt budget. Bytes rather
/// than tokens because the reader is slicing a file (`agent::memory`), and it says in the
/// prompt when it had to cut.
///
/// No cap of its own on a large window: from a budget of about 170,000 up, the slice exceeds
/// `memory::MEMORY_DIR_MAX_BYTES` (64 KB, about 16,000 tokens), so the disk cap is what binds
/// and the whole folder rides along: a small share of such a window, cached after the turn
/// that carries it.
///
/// ⚠️ Not to be confused with `memory::MEMORY_DIR_MAX_BYTES`: that one protects DISK and is a
/// constant; this one protects the PROMPT and moves with the model.
pub fn memory_slice_bytes(prompt_tokens: usize) -> usize {
    prompt_tokens.saturating_sub(FIXED_PROMPT_OVERHEAD_TOKENS) * MEMORY_BUDGET_PERCENT / 100 * CHARS_PER_TOKEN_ESTIMATE
}

/// One family's context window, matched by model-id prefix. The FIRST matching row wins, so a
/// more specific id sits above the family it shares a stem with. The budget is DERIVED from
/// the window ([`budget_for`]) rather than stored beside it, so a family can't carry a
/// budget its window couldn't hold.
struct ModelFamily {
    prefix: &'static str,
    /// At least this many tokens. Verify at release time; drift upward is harmless.
    window_tokens: usize,
}

/// What one prompt may claim inside `window_tokens`: all of it, less the reply's room
/// (`reply_tokens`, which comes out of the same window) and [`ESTIMATE_SAFETY_PERCENT`] of the
/// rest for the estimator's error.
fn budget_for(window_tokens: usize, reply_tokens: usize) -> usize {
    window_tokens.saturating_sub(reply_tokens) * (100 - ESTIMATE_SAFETY_PERCENT) / 100
}

/// [`budget_for`] with the base reply slot: what a window gives a model that barely thinks.
#[cfg(test)]
fn budget_for_window(window_tokens: usize) -> usize {
    budget_for(window_tokens, AGENT_MAX_OUTPUT_TOKENS as usize)
}

/// Claude 4.6 and everything after carry a 1M window; 4.5 and older carry 200,000. Anthropic
/// reports both through `GET /v1/models` (`max_input_tokens`), so this table only answers
/// before that report arrives (verified 2026-10-27, `max_input_tokens` for all 14 listed
/// models). The dated and undated ids of the old generations sit above the catch-all.
const ANTHROPIC_FAMILIES: &[ModelFamily] = &[
    ModelFamily {
        prefix: "claude-3",
        window_tokens: 200_000,
    },
    ModelFamily {
        prefix: "claude-opus-4-0",
        window_tokens: 200_000,
    },
    ModelFamily {
        prefix: "claude-opus-4-1",
        window_tokens: 200_000,
    },
    ModelFamily {
        prefix: "claude-opus-4-2025",
        window_tokens: 200_000,
    },
    ModelFamily {
        prefix: "claude-opus-4-5",
        window_tokens: 200_000,
    },
    ModelFamily {
        prefix: "claude-sonnet-4-0",
        window_tokens: 200_000,
    },
    ModelFamily {
        prefix: "claude-sonnet-4-2025",
        window_tokens: 200_000,
    },
    ModelFamily {
        prefix: "claude-sonnet-4-5",
        window_tokens: 200_000,
    },
    ModelFamily {
        prefix: "claude-haiku-4-5",
        window_tokens: 200_000,
    },
    ModelFamily {
        prefix: "claude-",
        window_tokens: 1_000_000,
    },
];

/// The OpenAI-compatible families, first-party and third-party alike, because every provider
/// preset except Anthropic and Gemini routes through the OpenAI adapter (`ai::client`) and so
/// arrives here as [`ProviderTag::OpenAi`]. OpenAI, DeepSeek, Qwen, and xAI report no window
/// in their model lists, so for them this table is the only knowledge there is; the OpenAI
/// rows are from OpenRouter's catalog, which does report them (verified 2026-10-27,
/// `context_length` in `GET /api/v1/models`).
///
/// **The class this table gets wrong when it's short**: a family Cmdr ships a provider preset
/// for silently taking the conservative default because nobody added a row. `deepseek`,
/// `qwen`, `grok`, and `mistral` were all in that state at once.
/// `every_shipped_cloud_preset_is_in_the_family_table` walks every shipped preset's default
/// model, so the next preset added without a row here fails a test instead of quietly costing
/// a user most of their window.
///
/// Anything older and smaller (`gpt-3.5-turbo`) is absent on purpose and takes the
/// conservative default.
const OPENAI_COMPATIBLE_FAMILIES: &[ModelFamily] = &[
    ModelFamily {
        prefix: "gpt-6",
        window_tokens: 1_050_000,
    },
    // The 5.4 line splits: the flagship and pro carry 1,050,000, mini and nano 400,000, and the
    // image model 272,000.
    ModelFamily {
        prefix: "gpt-5.4-mini",
        window_tokens: 400_000,
    },
    ModelFamily {
        prefix: "gpt-5.4-nano",
        window_tokens: 400_000,
    },
    ModelFamily {
        prefix: "gpt-5.4-image",
        window_tokens: 272_000,
    },
    ModelFamily {
        prefix: "gpt-5.4",
        window_tokens: 1_050_000,
    },
    ModelFamily {
        prefix: "gpt-5.5",
        window_tokens: 1_050_000,
    },
    ModelFamily {
        prefix: "gpt-5.6",
        window_tokens: 1_050_000,
    },
    // The chat-tuned 5.2 is far smaller than the 5.x family it shares a stem with.
    ModelFamily {
        prefix: "gpt-5.2-chat",
        window_tokens: 128_000,
    },
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
    // OpenAI's open-weight line, served by the hosts rather than by OpenAI: `gpt-oss-20b` and
    // `gpt-oss-120b` (Groq, Fireworks). Both report a 131,072-token window in their hosts'
    // model lists (verified 2026-09-04). It sits below the `gpt-` rows above only in reading
    // order; the prefixes don't overlap.
    ModelFamily {
        prefix: "gpt-oss",
        window_tokens: 131_072,
    },
    // Zhipu's GLM line, which is what Fireworks serves in place of the Llamas it dropped.
    // Fireworks reports 1,048,576 for `glm-5p3` in its model list (verified 2026-10-27), which
    // is what a Fireworks user gets; this floor is for the moment before that report arrives
    // and for a smaller GLM on another host.
    ModelFamily {
        prefix: "glm-",
        window_tokens: 128_000,
    },
    // Llama 3.x and 4 across the hosts that serve them: `Llama-4-Maverick-…` (Together).
    // Groq and Fireworks both retired their Llama models in 2026, but a user can still name
    // one on another host, and Llama 2 is absent on purpose (a 4,096-token window), which is
    // why these prefixes carry the generation.
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
    // Just under 128K, so a rounded 128,000 would overclaim (verified 2026-10-27, OpenRouter's
    // `perplexity/sonar`; Perplexity's own model list doesn't serve its chat models).
    ModelFamily {
        prefix: "sonar",
        window_tokens: 127_072,
    },
];

/// Gemini's chat models carry a 1M-token window across 2.x and 3.x flash and pro; the image
/// variants are far smaller, so they come first (verified 2026-10-26, `inputTokenLimit` in
/// `GET /v1/models`, which `ai::model_capabilities` reads too).
const GEMINI_FAMILIES: &[ModelFamily] = &[
    ModelFamily {
        prefix: "gemini-3-pro-image",
        window_tokens: 131_072,
    },
    ModelFamily {
        prefix: "gemini-3.1-flash-image",
        window_tokens: 65_536,
    },
    ModelFamily {
        prefix: "gemini-3.1-flash-lite-image",
        window_tokens: 65_536,
    },
    ModelFamily {
        prefix: "gemini-2.5-flash-image",
        window_tokens: 32_768,
    },
    ModelFamily {
        prefix: "gemini-2",
        window_tokens: 1_000_000,
    },
    ModelFamily {
        prefix: "gemini-3",
        window_tokens: 1_000_000,
    },
];

/// Where a model's window came from. The settings surface names it, so a guess from the table
/// never passes for the provider's word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowSource {
    /// The local server's configured context window (`ai.localContextSize`).
    LocalServer,
    /// The provider's own model list (`ai::model_capabilities`).
    Provider,
    /// This module's family table: a floor someone verified once, which may have aged.
    FamilyTable,
}

/// A model's context window as far as anything here knows it, and who said so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownWindow {
    pub tokens: usize,
    pub source: WindowSource,
}

/// Which source decided a budget. Carried out of [`resolve_prompt_budget`] and logged at
/// send, so "839,800 because Anthropic says this model has 1,000,000" is distinguishable from
/// "160,000 because you asked for it", from "the table's guess", and from "16,000 because
/// nothing here knows this model".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetSource {
    /// The user's explicit "Chat memory size" choice, used as given.
    UserSetting,
    /// The window the provider's own model list reports.
    ProviderReported,
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
            Self::ProviderReported => "the window the provider reports",
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
    /// The window the provider's model list reports for this model, when a report has arrived
    /// (`ai::model_capabilities`). Ignored for [`ProviderTag::Local`], whose window is the
    /// server's own setting.
    pub reported_window_tokens: Option<usize>,
    /// The reply's ceiling this slot sends (`effort::resolve_effort`): Automatic leaves that much of
    /// the window free, since thinking, the reply, and its tool calls all come out of it.
    pub reply_tokens: usize,
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
    /// The user's explicit size, plus the reply's room, is larger than
    /// [`Self::known_window_tokens`]. It is used
    /// ANYWAY: our knowledge will be wrong sometimes and the user may be right about their own
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

/// The window we believe a slot's model has: the local server's configured window for a local
/// model; for a cloud one, what the provider reports, else the family table; else nothing.
pub fn known_window(
    provider: ProviderTag,
    model: &str,
    local_context_tokens: u32,
    reported_window_tokens: Option<usize>,
) -> Option<KnownWindow> {
    let table = match provider {
        ProviderTag::Local => {
            return Some(KnownWindow {
                tokens: local_context_tokens as usize,
                source: WindowSource::LocalServer,
            });
        }
        ProviderTag::Anthropic => ANTHROPIC_FAMILIES,
        // The Responses API is the same OpenAI catalog with the same windows.
        ProviderTag::OpenAi | ProviderTag::OpenAiResponses => OPENAI_COMPATIBLE_FAMILIES,
        ProviderTag::Gemini => GEMINI_FAMILIES,
    };
    if let Some(tokens) = reported_window_tokens {
        return Some(KnownWindow {
            tokens,
            source: WindowSource::Provider,
        });
    }
    let id = normalized_model_id(model);
    table.iter().find(|f| id.starts_with(f.prefix)).map(|f| KnownWindow {
        tokens: f.window_tokens,
        source: WindowSource::FamilyTable,
    })
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
/// (warned, never blocked). "Automatic" spends the whole known window ([`budget_for`]):
/// the local server's for a local model, the provider's report or the family table for a cloud
/// one, and falls back to [`DEFAULT_PROMPT_TOKEN_BUDGET`] for a model nothing here recognizes.
pub fn resolve_prompt_budget(inputs: BudgetInputs<'_>) -> Result<ResolvedBudget, BudgetRefusal> {
    if inputs.provider == ProviderTag::Local && inputs.local_context_tokens < MIN_LOCAL_CONTEXT_TOKENS {
        return Err(BudgetRefusal::LocalWindowBelowFloor {
            window_tokens: inputs.local_context_tokens,
            floor_tokens: MIN_LOCAL_CONTEXT_TOKENS,
        });
    }
    let window = known_window(
        inputs.provider,
        inputs.model,
        inputs.local_context_tokens,
        inputs.reported_window_tokens,
    );
    let known_window_tokens = window.map(|w| w.tokens);

    if let Some(chosen) = inputs.user_choice {
        return Ok(ResolvedBudget {
            prompt_tokens: chosen,
            source: BudgetSource::UserSetting,
            known_window_tokens,
            // The reply comes out of the same window: a provider refuses prompt plus `max_tokens` past it.
            over_known_window: known_window_tokens.is_some_and(|window| chosen + inputs.reply_tokens > window),
        });
    }

    let (prompt_tokens, source) = match window {
        Some(KnownWindow { tokens, source }) => (
            budget_for(tokens, inputs.reply_tokens),
            match source {
                WindowSource::LocalServer => BudgetSource::LocalServerWindow,
                WindowSource::Provider => BudgetSource::ProviderReported,
                WindowSource::FamilyTable => BudgetSource::FamilyTable,
            },
        ),
        None => (DEFAULT_PROMPT_TOKEN_BUDGET, BudgetSource::Default),
    };
    Ok(ResolvedBudget {
        prompt_tokens,
        source,
        known_window_tokens,
        over_known_window: false,
    })
}

/// The chat memory a thread's timeline names ("Chat memory changed to …"), which moves only when
/// the USER moved something: their "Chat memory size" or the model. It's the prompt budget with
/// the base reply slot (an effort only reserves more reply), sized from the family table when
/// the table knows the model (a provider report arriving mid-thread would otherwise read as a
/// change nobody made). The actual budget a turn assembles against is [`resolve_prompt_budget`]'s.
pub fn chat_memory_figure(inputs: BudgetInputs<'_>) -> Result<usize, BudgetRefusal> {
    let table_knows = known_window(inputs.provider, inputs.model, inputs.local_context_tokens, None).is_some();
    resolve_prompt_budget(BudgetInputs {
        reply_tokens: AGENT_MAX_OUTPUT_TOKENS as usize,
        reported_window_tokens: if table_knows {
            None
        } else {
            inputs.reported_window_tokens
        },
        ..inputs
    })
    .map(|resolved| resolved.prompt_tokens)
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
mod tests;
