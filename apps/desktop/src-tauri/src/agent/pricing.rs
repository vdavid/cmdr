//! A minimal per-model price table for honest cost estimation (spec §2.4, plan §5).
//!
//! Prices are USD per **million tokens**, quoted the way providers publish them, split
//! into prompt (input) and completion (output). Matched by model-id prefix, longest
//! first, because ids drift within a family (`gpt-4o-mini` must win over `gpt-4o`).
//!
//! **PRICES DRIFT — verify against each provider's pricing page at release time.** A model NOT in the table is
//! UNPRICED: the meter records its tokens with cost 0 and `priced = false`, so the UI shows
//! the token counts with cost "unknown", never a silent $0. A local model is free/on-device:
//! cost 0 and `priced = true`. This honest miss-path is the point — an approximate estimate
//! for known models, and an explicit "unknown" (never a fake zero) for everything else.

use crate::agent::llm::types::ProviderTag;

/// The outcome of pricing one completed call.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PricedCost {
    /// Integer micro-USD estimate (0 when local or unpriced).
    pub cost_micros: i64,
    /// False when the model wasn't in the table (an unknown cloud model): the cost is
    /// unknown, shown as such, never a silent $0.
    pub priced: bool,
}

/// One family's price, USD per million tokens (prompt/completion). Matched by id prefix.
struct ModelPrice {
    prefix: &'static str,
    prompt_usd_per_mtok: f64,
    completion_usd_per_mtok: f64,
}

/// The provisional Tier-1 price table (USD per million tokens). Ordered so a
/// longer/more-specific prefix precedes the family it shares a stem with (mini before
/// the base model). Prices are approximate and MUST be re-verified at release.
const ANTHROPIC_PRICES: &[ModelPrice] = &[
    // Haiku (cheaper) before the broader `claude-` families it doesn't share a stem with.
    ModelPrice {
        prefix: "claude-3-5-haiku",
        prompt_usd_per_mtok: 0.80,
        completion_usd_per_mtok: 4.00,
    },
    ModelPrice {
        prefix: "claude-haiku",
        prompt_usd_per_mtok: 0.80,
        completion_usd_per_mtok: 4.00,
    },
    ModelPrice {
        prefix: "claude-opus",
        prompt_usd_per_mtok: 15.00,
        completion_usd_per_mtok: 75.00,
    },
    ModelPrice {
        prefix: "claude-sonnet",
        prompt_usd_per_mtok: 3.00,
        completion_usd_per_mtok: 15.00,
    },
    ModelPrice {
        prefix: "claude-3-7-sonnet",
        prompt_usd_per_mtok: 3.00,
        completion_usd_per_mtok: 15.00,
    },
    ModelPrice {
        prefix: "claude-3-5-sonnet",
        prompt_usd_per_mtok: 3.00,
        completion_usd_per_mtok: 15.00,
    },
];

const OPENAI_PRICES: &[ModelPrice] = &[
    ModelPrice {
        prefix: "gpt-4o-mini",
        prompt_usd_per_mtok: 0.15,
        completion_usd_per_mtok: 0.60,
    },
    ModelPrice {
        prefix: "gpt-4o",
        prompt_usd_per_mtok: 2.50,
        completion_usd_per_mtok: 10.00,
    },
    ModelPrice {
        prefix: "gpt-4.1-mini",
        prompt_usd_per_mtok: 0.40,
        completion_usd_per_mtok: 1.60,
    },
    ModelPrice {
        prefix: "gpt-4.1-nano",
        prompt_usd_per_mtok: 0.10,
        completion_usd_per_mtok: 0.40,
    },
    ModelPrice {
        prefix: "gpt-4.1",
        prompt_usd_per_mtok: 2.00,
        completion_usd_per_mtok: 8.00,
    },
    ModelPrice {
        prefix: "o4-mini",
        prompt_usd_per_mtok: 1.10,
        completion_usd_per_mtok: 4.40,
    },
];

const GEMINI_PRICES: &[ModelPrice] = &[
    ModelPrice {
        prefix: "gemini-2.5-flash-lite",
        prompt_usd_per_mtok: 0.10,
        completion_usd_per_mtok: 0.40,
    },
    ModelPrice {
        prefix: "gemini-2.5-flash",
        prompt_usd_per_mtok: 0.30,
        completion_usd_per_mtok: 2.50,
    },
    ModelPrice {
        prefix: "gemini-2.5-pro",
        prompt_usd_per_mtok: 1.25,
        completion_usd_per_mtok: 10.00,
    },
    ModelPrice {
        prefix: "gemini-2.0-flash",
        prompt_usd_per_mtok: 0.10,
        completion_usd_per_mtok: 0.40,
    },
];

/// Price one completed call's usage. Local ⇒ free + priced; a known cloud model ⇒
/// estimated + priced; an unknown cloud model ⇒ 0 + unpriced (shown "unknown", never $0).
pub fn price_call(provider: ProviderTag, model: &str, prompt_tokens: u64, completion_tokens: u64) -> PricedCost {
    if provider == ProviderTag::Local {
        return PricedCost {
            cost_micros: 0,
            priced: true,
        };
    }
    let Some(price) = lookup_price(provider, model) else {
        return PricedCost {
            cost_micros: 0,
            priced: false,
        };
    };
    // cost_micros = tokens * usd_per_mtok (derivation: micro-USD = USD * 1e6, and
    // USD = tokens/1e6 * usd_per_mtok, so the 1e6 factors cancel). Round to the nearest
    // micro-USD.
    let micros =
        prompt_tokens as f64 * price.prompt_usd_per_mtok + completion_tokens as f64 * price.completion_usd_per_mtok;
    PricedCost {
        cost_micros: micros.round() as i64,
        priced: true,
    }
}

/// Find the price for a provider+model by longest-matching id prefix, or `None` for an
/// unknown model. The table's ordering guarantees a more-specific prefix is tried first.
fn lookup_price(provider: ProviderTag, model: &str) -> Option<&'static ModelPrice> {
    let table = match provider {
        ProviderTag::Anthropic => ANTHROPIC_PRICES,
        // The Responses API is the same OpenAI catalog, priced identically.
        ProviderTag::OpenAi | ProviderTag::OpenAiResponses => OPENAI_PRICES,
        ProviderTag::Gemini => GEMINI_PRICES,
        ProviderTag::Local => return None,
    };
    table.iter().find(|p| model.starts_with(p.prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_is_free_and_priced() {
        let cost = price_call(ProviderTag::Local, "any-local-model", 1000, 500);
        assert_eq!(
            cost,
            PricedCost {
                cost_micros: 0,
                priced: true
            }
        );
    }

    #[test]
    fn unknown_cloud_model_is_unpriced_never_zero_cost_silently() {
        // The honest miss-path: an unknown cloud model records tokens (elsewhere) but its
        // cost is UNKNOWN, flagged `priced = false`, never a silent $0.
        let cost = price_call(ProviderTag::OpenAi, "some-future-model-9000", 1000, 500);
        assert!(!cost.priced, "an unknown model must be flagged unpriced");
        assert_eq!(cost.cost_micros, 0);
    }

    #[test]
    fn known_openai_model_estimates_cost() {
        // gpt-4o-mini: $0.15/Mtok prompt, $0.60/Mtok completion.
        // 1000 * 0.15 + 500 * 0.60 = 150 + 300 = 450 micro-USD ($0.00045).
        let cost = price_call(ProviderTag::OpenAi, "gpt-4o-mini", 1000, 500);
        assert!(cost.priced);
        assert_eq!(cost.cost_micros, 450);
    }

    #[test]
    fn longer_prefix_wins_over_family_stem() {
        // `gpt-4o-mini` must not match the pricier `gpt-4o` entry.
        let mini = price_call(ProviderTag::OpenAi, "gpt-4o-mini-2024-07-18", 1_000_000, 0);
        assert_eq!(mini.cost_micros, 150_000, "gpt-4o-mini prompt is $0.15/Mtok");
        let base = price_call(ProviderTag::OpenAi, "gpt-4o-2024-11-20", 1_000_000, 0);
        assert_eq!(base.cost_micros, 2_500_000, "gpt-4o prompt is $2.50/Mtok");
    }

    #[test]
    fn known_anthropic_and_gemini_models_are_priced() {
        assert!(price_call(ProviderTag::Anthropic, "claude-sonnet-4-5", 100, 100).priced);
        assert!(price_call(ProviderTag::Gemini, "gemini-2.5-flash", 100, 100).priced);
    }
}
