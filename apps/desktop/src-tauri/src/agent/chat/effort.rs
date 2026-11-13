//! How hard the interactive slot's model thinks, and how much room its reply gets for it.
//!
//! Pure, like [`budget`](super::budget): the caller hands in the provider, the model, what the
//! provider reports about its reasoning (`ai::model_capabilities`), and the user's choice
//! (`ai.reasoningEfforts`, per provider and model), and gets back the level to send and the
//! per-call output ceiling.
//!
//! **Never send a level the model can't take.** An explicit level on a model that doesn't
//! reason is a 400 there, so a level goes out only when the provider listed it or the
//! built-in table vouches for the family; a choice the model can't take maps to the nearest
//! one it can ([`resolve_effort`] says when, the caller logs it), and with nothing known the
//! answer is [`ReasoningEffort::ProviderDefault`], the one value safe everywhere.
//!
//! **Thinking shares the reply's room.** Reasoning tokens come out of the same `max_tokens`
//! as the visible answer and the tool calls, so a higher effort gets a bigger ceiling
//! ([`reply_room`]), capped at what the model can write. The prompt budget leaves exactly
//! that much of the window free (`budget::BudgetInputs::reply_tokens`).

use crate::agent::chat::budget::{
    AGENT_MAX_OUTPUT_TOKENS, ESTIMATE_SAFETY_PERCENT, FIXED_PROMPT_OVERHEAD_TOKENS, MAX_TOOL_RESULT_TOKENS,
};
use crate::agent::llm::ReasoningEffort;
use crate::agent::llm::types::ProviderTag;
use crate::ai::model_capabilities::ReasoningCapabilities;

/// One level of thinking the user can pick, cheapest first. "Recommended" sits outside it
/// ([`EffortChoice`]) because it isn't a point on this scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EffortLevel {
    Low,
    Medium,
    High,
    Max,
}

impl EffortLevel {
    const ALL: [EffortLevel; 4] = [Self::Low, Self::Medium, Self::High, Self::Max];

    /// The stored and wire token, also each provider's own word for it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Max => "max",
        }
    }

    /// A provider's level name as ours. Names we have no level for (`xhigh`, `minimal`,
    /// `none`) answer `None` and are simply not offered.
    fn from_provider_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|level| level.as_str() == word)
    }

    pub(crate) fn effort(self) -> ReasoningEffort {
        match self {
            Self::Low => ReasoningEffort::Low,
            Self::Medium => ReasoningEffort::Medium,
            Self::High => ReasoningEffort::High,
            Self::Max => ReasoningEffort::Max,
        }
    }
}

/// What the user picked for one model. Thinking stays on: users get no Off (David's call,
/// #770), though Cmdr may still send [`ReasoningEffort::Off`] itself for an internal call, where
/// the model can stop ([`EffortOptions::can_turn_off`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffortChoice {
    /// Send nothing and let the provider decide: the default, and safe on every model.
    Recommended,
    Level(EffortLevel),
}

impl EffortChoice {
    /// A stored token. Anything this build doesn't know reads as Recommended, so a value from a
    /// newer build or a hand-edited file can never become a level the model can't take. That
    /// includes `"off"`, which older builds offered: nobody keeps a pick they can't see.
    pub fn parse(token: &str) -> Self {
        EffortLevel::from_provider_word(token).map_or(Self::Recommended, Self::Level)
    }
}

/// The user's choice for (`provider_id`, `model`) out of the stored map
/// (`ai.reasoningEfforts`, a JSON object keyed `"<provider id>/<model>"`). Missing or
/// unreadable reads as Recommended.
pub fn choice_for(stored_json: &str, provider_id: &str, model: &str) -> EffortChoice {
    serde_json::from_str::<serde_json::Value>(stored_json)
        .ok()
        .and_then(|map| {
            map.get(effort_key(provider_id, model))?
                .as_str()
                .map(EffortChoice::parse)
        })
        .unwrap_or(EffortChoice::Recommended)
}

/// The key one model's choice is stored under. Provider-scoped, since one id can name
/// different models (and different limits) on different hosts.
pub fn effort_key(provider_id: &str, model: &str) -> String {
    format!("{provider_id}/{model}")
}

/// Where what we know about a model's reasoning came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnowledgeSource {
    /// The provider's own model list.
    Provider,
    /// This module's family table: verified once, may have aged.
    BuiltIn,
}

/// What a model can be asked to do, for the picker and for [`resolve_effort`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffortOptions {
    /// Whether the model reasons: `Some(false)` hides the picker, `None` means nothing knows.
    pub reasons: Option<bool>,
    /// The levels it takes, cheapest first. Empty when none are known.
    pub levels: Vec<EffortLevel>,
    /// Whether it can be asked not to think at all. For Cmdr's own calls only: the picker
    /// offers no Off, so this never reaches the UI.
    pub can_turn_off: bool,
    /// What Recommended amounts to, where anything knows: the provider's reported default, or
    /// its documented one. `None` means the provider decides per request (Gemini's dynamic
    /// thinking) or nothing knows.
    pub recommended: Option<EffortLevel>,
    pub source: Option<KnowledgeSource>,
}

impl EffortOptions {
    fn unknown() -> Self {
        Self {
            reasons: None,
            levels: Vec::new(),
            can_turn_off: false,
            recommended: None,
            source: None,
        }
    }
}

/// One family's reasoning, for a provider that reports none (OpenAI) or reports that a model
/// reasons without listing levels (Groq, Gemini). First match wins.
struct ReasoningFamily {
    prefix: &'static str,
    reasons: bool,
    levels: &'static [EffortLevel],
    recommended: Option<EffortLevel>,
}

const LOW_MEDIUM_HIGH: &[EffortLevel] = &[EffortLevel::Low, EffortLevel::Medium, EffortLevel::High];

/// Verified 2026-10-27 against each provider's reasoning docs. `reasoning_effort` on OpenAI's
/// reasoning models takes low / medium / high (medium is its default); Gemini 3 takes
/// `thinkingLevel` low or high and thinks dynamically by default, 2.5 a `thinkingBudget` genai
/// maps all three onto; Claude takes `output_config.effort` with high as its default; the
/// open-weight `gpt-oss` takes the same three as OpenAI's.
const REASONING_FAMILIES: &[ReasoningFamily] = &[
    // Chat-tuned GPT-5 variants take no reasoning effort, so they get no picker.
    ReasoningFamily {
        prefix: "gpt-5-chat",
        reasons: false,
        levels: &[],
        recommended: None,
    },
    ReasoningFamily {
        prefix: "gpt-5.2-chat",
        reasons: false,
        levels: &[],
        recommended: None,
    },
    ReasoningFamily {
        prefix: "gpt-oss",
        reasons: true,
        levels: LOW_MEDIUM_HIGH,
        recommended: Some(EffortLevel::Medium),
    },
    ReasoningFamily {
        prefix: "gpt-6",
        reasons: true,
        levels: LOW_MEDIUM_HIGH,
        recommended: Some(EffortLevel::Medium),
    },
    ReasoningFamily {
        prefix: "gpt-5",
        reasons: true,
        levels: LOW_MEDIUM_HIGH,
        recommended: Some(EffortLevel::Medium),
    },
    // The o-series arms outlive OpenAI's own API (`o3`, the last, retires 2026-12-11): Azure
    // OpenAI, OpenRouter (`openai/o3`), and proxies keep serving these ids.
    ReasoningFamily {
        prefix: "o1",
        reasons: true,
        levels: LOW_MEDIUM_HIGH,
        recommended: Some(EffortLevel::Medium),
    },
    ReasoningFamily {
        prefix: "o3",
        reasons: true,
        levels: LOW_MEDIUM_HIGH,
        recommended: Some(EffortLevel::Medium),
    },
    ReasoningFamily {
        prefix: "o4",
        reasons: true,
        levels: LOW_MEDIUM_HIGH,
        recommended: Some(EffortLevel::Medium),
    },
    ReasoningFamily {
        prefix: "gpt-4",
        reasons: false,
        levels: &[],
        recommended: None,
    },
    ReasoningFamily {
        prefix: "gemini-3",
        reasons: true,
        levels: &[EffortLevel::Low, EffortLevel::High],
        recommended: None,
    },
    ReasoningFamily {
        prefix: "gemini-2.5",
        reasons: true,
        levels: LOW_MEDIUM_HIGH,
        recommended: None,
    },
    ReasoningFamily {
        prefix: "claude-",
        reasons: true,
        levels: &[
            EffortLevel::Low,
            EffortLevel::Medium,
            EffortLevel::High,
            EffortLevel::Max,
        ],
        recommended: Some(EffortLevel::High),
    },
];

fn family_for(model: &str) -> Option<&'static ReasoningFamily> {
    let id = model.rsplit('/').next().unwrap_or(model).to_ascii_lowercase();
    REASONING_FAMILIES.iter().find(|family| id.starts_with(family.prefix))
}

/// What Recommended amounts to when the provider reports no default but documents one.
fn documented_default(provider: ProviderTag) -> Option<EffortLevel> {
    match provider {
        ProviderTag::Anthropic => Some(EffortLevel::High),
        // What `genai_impl` sends for ProviderDefault there.
        ProviderTag::OpenAiResponses => Some(EffortLevel::Medium),
        _ => None,
    }
}

/// What `model` on `provider` can be asked to do, from the provider's report where there is
/// one and the family table where there isn't (or where the report says "reasons" with no
/// levels). A local model offers nothing: its server decides, and an explicit level is a gamble.
pub fn effort_options(provider: ProviderTag, model: &str, reported: Option<&ReasoningCapabilities>) -> EffortOptions {
    if provider == ProviderTag::Local {
        return EffortOptions::unknown();
    }
    let family = family_for(model);
    let can_turn_off = reported
        .is_some_and(|caps| caps.thinking_modes.iter().any(|mode| mode == "disabled") || caps.mandatory == Some(false));

    if let Some(caps) = reported {
        if caps.supported == Some(false) {
            return EffortOptions {
                reasons: Some(false),
                source: Some(KnowledgeSource::Provider),
                ..EffortOptions::unknown()
            };
        }
        let mut levels: Vec<EffortLevel> = caps
            .effort_levels
            .iter()
            .filter_map(|word| EffortLevel::from_provider_word(word))
            .collect();
        levels.sort();
        levels.dedup();
        let recommended = caps
            .default_effort
            .as_deref()
            .and_then(EffortLevel::from_provider_word)
            .or_else(|| documented_default(provider))
            .or_else(|| family.and_then(|f| f.recommended));
        if !levels.is_empty() {
            return EffortOptions {
                reasons: Some(true),
                levels,
                can_turn_off,
                recommended,
                source: Some(KnowledgeSource::Provider),
            };
        }
        if caps.supported == Some(true) {
            let borrowed = family.filter(|f| f.reasons);
            return EffortOptions {
                reasons: Some(true),
                levels: borrowed.map(|f| f.levels.to_vec()).unwrap_or_default(),
                can_turn_off,
                recommended,
                source: Some(if borrowed.is_some() {
                    KnowledgeSource::BuiltIn
                } else {
                    KnowledgeSource::Provider
                }),
            };
        }
    }

    match family {
        Some(f) => EffortOptions {
            reasons: Some(f.reasons),
            levels: f.levels.to_vec(),
            can_turn_off,
            recommended: f
                .recommended
                .or_else(|| f.reasons.then(|| documented_default(provider)).flatten()),
            source: Some(KnowledgeSource::BuiltIn),
        },
        None => EffortOptions::unknown(),
    }
}

/// What one call sends: the level, the reply's ceiling, and, when the choice couldn't be
/// honored as is, what it became (the caller logs it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedEffort {
    pub effort: ReasoningEffort,
    pub max_output_tokens: u32,
    pub adjusted_to: Option<EffortChoice>,
}

/// The per-call output ceiling for a level of thinking: [`AGENT_MAX_OUTPUT_TOKENS`] when the
/// model barely thinks, more as it thinks harder, since thinking spends the same tokens before
/// the reply and its tool calls get any. A ceiling, not a charge: the provider bills what the
/// model actually writes.
pub fn reply_room(level: Option<EffortLevel>) -> u32 {
    match level {
        None | Some(EffortLevel::Low) => AGENT_MAX_OUTPUT_TOKENS,
        Some(EffortLevel::Medium) => 16_000,
        Some(EffortLevel::High) => 32_000,
        Some(EffortLevel::Max) => 64_000,
    }
}

/// The level to send for `choice` on a model with `options`, and the reply's ceiling, capped at
/// the model's own `max_output_tokens` where reported.
pub fn resolve_effort(
    options: &EffortOptions,
    choice: EffortChoice,
    reported_max_output: Option<u32>,
) -> ResolvedEffort {
    // What Recommended is likely to spend: the known default, else a middling guess for a
    // model known to reason.
    let recommended_room = options
        .recommended
        .or_else(|| (options.reasons == Some(true)).then_some(EffortLevel::Medium));
    let recommended = (ReasoningEffort::ProviderDefault, recommended_room);

    let ((effort, room), adjusted_to) = match choice {
        EffortChoice::Recommended => (recommended, None),
        EffortChoice::Level(level) if options.levels.contains(&level) => ((level.effort(), Some(level)), None),
        EffortChoice::Level(wanted) => match nearest(&options.levels, wanted) {
            Some(level) => ((level.effort(), Some(level)), Some(EffortChoice::Level(level))),
            None => (recommended, Some(EffortChoice::Recommended)),
        },
    };
    let ceiling = reply_room(room);
    ResolvedEffort {
        effort,
        max_output_tokens: reported_max_output.map_or(ceiling, |max| ceiling.min(max)),
        adjusted_to,
    }
}

/// The least prompt a cloud call should keep: the fixed prefix plus one paged tool result. The
/// local path has its own, stricter floor (`budget::MIN_LOCAL_CONTEXT_TOKENS`).
pub const MIN_CLOUD_PROMPT_TOKENS: usize = FIXED_PROMPT_OVERHEAD_TOKENS + MAX_TOOL_RESULT_TOKENS;

/// The smallest reply ceiling worth sending: a sentence and a tool call. Under it, a too-small
/// window overruns honestly rather than asking for a reply it can't hold.
pub const MIN_REPLY_TOKENS: u32 = 4_096;

/// The reply's ceiling fitted to the model's known window, so a high effort on a small window
/// can't starve the prompt (the budget leaves the reply's room free) or make prompt plus
/// `max_tokens` overrun the window. At most a quarter of the window, but never under the base
/// slot while the window can spare it; then, if the prompt would still fall under
/// [`MIN_CLOUD_PROMPT_TOKENS`], less, down to [`MIN_REPLY_TOKENS`]. An unknown window changes
/// nothing.
pub fn fit_reply_to_window(reply: u32, window_tokens: Option<usize>) -> u32 {
    let Some(window) = window_tokens else {
        return reply;
    };
    let share = u32::try_from(window / 4)
        .unwrap_or(u32::MAX)
        .max(AGENT_MAX_OUTPUT_TOKENS);
    let reply = reply.min(share);
    // What the reply may take so the prompt keeps its floor after the estimator's margin.
    let prompt_needs = (MIN_CLOUD_PROMPT_TOKENS * 100).div_ceil(100 - ESTIMATE_SAFETY_PERCENT);
    let room = u32::try_from(window.saturating_sub(prompt_needs)).unwrap_or(u32::MAX);
    reply.min(room.max(MIN_REPLY_TOKENS))
}

/// The offered level closest to `wanted` (ties go to the cheaper one).
fn nearest(levels: &[EffortLevel], wanted: EffortLevel) -> Option<EffortLevel> {
    levels
        .iter()
        .copied()
        .min_by_key(|level| ((*level as i32 - wanted as i32).abs(), *level as i32))
}

#[cfg(test)]
mod tests;
