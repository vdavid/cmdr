//! Whether a card gets a verdict, decided before anything is read for a model, and how far
//! the model that writes it can be trusted.
//!
//! ❗ **The order is the consent guarantee**: the provider, then the person's setting, then
//! the slot (`resolve_agent_llm`, which carries the managed policy, cloud consent, and the
//! E2E fake). Only a slot that resolved builds a [`Ready`], and the peek and the prompt take
//! one, so a refusal here reads no file for a model and calls no model.
//!
//! Every model that resolves writes the take; [`model_trust`] decides, in this one place,
//! whether the card warns about it.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::AppHandle;

use super::super::event::{ModelTrust, VerdictUnavailable};
use super::prompt::PromptVariant;
use crate::agent::chat::session::{AgentSlot, SlotRefusal, resolve_agent_llm, resolve_verdict_call_settings};
use crate::agent::llm::AgentLlm;
use crate::agent::llm::types::{AgentUsage, CallSettings, ProviderTag};
use crate::agent::pricing::PricedCost;

/// The most the verdict writes, in tokens: two or three sentences, and a derailed
/// answer stops early.
pub(crate) const VERDICT_REPLY_TOKENS: u32 = 120;

/// Records one finished call's cost on the verdict's ledger row and answers what it
/// metered. Blocking (a `main.db` write): call it off the async worker.
pub(crate) type Meter = Arc<dyn Fn(AgentUsage) -> PricedCost + Send + Sync>;

/// A slot that resolved: everything the call needs.
pub(crate) struct Ready {
    pub(crate) llm: Box<dyn AgentLlm>,
    pub(crate) provider: ProviderTag,
    pub(crate) model: String,
    pub(crate) call: CallSettings,
    pub(crate) variant: PromptVariant,
    /// How far the card can trust this model's take ([`model_trust`]).
    pub(crate) trust: ModelTrust,
    pub(crate) meter: Meter,
}

/// What the gate reads before the slot.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GateInputs<'a> {
    /// `ai.provider`: `off`, `local`, or `cloud`.
    pub(crate) provider: &'a str,
    /// The person's `aboutThis.aiVerdict`.
    pub(crate) verdict_on: bool,
}

/// The gate, in its order. `resolve` runs only when everything before it allows.
pub(crate) fn decide<R>(
    inputs: GateInputs<'_>,
    resolve: impl FnOnce() -> Result<R, SlotRefusal>,
) -> Result<R, VerdictUnavailable> {
    if inputs.provider == "off" {
        return Err(VerdictUnavailable::AiOff);
    }
    if !inputs.verdict_on {
        return Err(VerdictUnavailable::TurnedOff);
    }
    let local = inputs.provider == "local";
    resolve().map_err(|refusal| match refusal {
        // A local server that isn't running: a verdict never starts it or a download.
        SlotRefusal::NotConfigured if local => VerdictUnavailable::NotReady,
        SlotRefusal::NotConfigured => VerdictUnavailable::NotConfigured,
        SlotRefusal::NoCloudConsent => VerdictUnavailable::NoCloudConsent,
        SlotRefusal::Managed(refusal) => VerdictUnavailable::Managed { refusal },
    })
}

/// The gate for a card a person opened, with the app's own reads. Blocking (it reads
/// `settings.json` and `main.db`): run it on a blocking thread.
pub(crate) fn resolve_for_card(app: &AppHandle) -> Result<Ready, VerdictUnavailable> {
    let provider = crate::ai::state::get_provider();
    let inputs = GateInputs {
        provider: &provider,
        verdict_on: crate::settings::load_about_this_ai_verdict(app),
    };
    let (resolved, provider, model) = decide(inputs, || resolve_agent_llm(app, AgentSlot::AboutThis))?;
    // The E2E fake answers as `Local` / `fake` and writes fixed text: it shows the plain
    // block, and the gallery shows each warning.
    let trust = if crate::test_mode::ask_cmdr_fake_active() {
        ModelTrust::Trusted
    } else {
        model_trust(provider, &model)
    };
    let Some(meter) = ledger_meter(app, provider, model.clone()) else {
        // ❌ Never an unmetered call: without its ledger row the verdict doesn't run.
        log::warn!(target: "about", "the verdict's cost ledger isn't there; no verdict");
        return Err(VerdictUnavailable::NotReady);
    };
    let call = resolve_verdict_call_settings(app, provider, &model, VERDICT_REPLY_TOKENS);
    Ok(Ready {
        llm: resolved.into_unlogged_llm(),
        provider,
        variant: PromptVariant::for_provider(provider),
        trust,
        model,
        call,
        meter,
    })
}

/// ❗ How far the card can trust `model`, reached through `provider`: the ONE place a model's
/// tier is decided. Every model writes the take; this only picks the card's warning.
///
/// - **Trusted**: Anthropic's Opus, Fable, and Sonnet families on Anthropic's own API
///   (`claude-<family>-<version>`, a dated snapshot too), OpenAI's `gpt-6-astra` on OpenAI's
///   API (a dated snapshot too), and Google's `gemini-4-carbon` on Gemini's API. The eval
///   measured only Sonnet 5.5 and Opus 5.5; the rest of the list is David's call
///   (`docs/notes/about-verdict-eval.md`).
/// - **Often wrong**: the bundled local model, and the Haiku family in any provider's
///   spelling (`claude-haiku-5-5`, an aggregator's `anthropic/claude-3.5-haiku`): the eval
///   measured both below the bar.
/// - **Untested**: everything else, an aggregator's route to a trusted model included (its
///   call settings are another measurement). An id this doesn't recognize lands here, never
///   in `Trusted`.
pub(crate) fn model_trust(provider: ProviderTag, model: &str) -> ModelTrust {
    if provider == ProviderTag::Local {
        return ModelTrust::OftenWrong;
    }
    let model = model.to_ascii_lowercase();
    if model.contains("claude") && model.contains("haiku") {
        return ModelTrust::OftenWrong;
    }
    let trusted = match provider {
        ProviderTag::Anthropic => is_trusted_claude(&model),
        ProviderTag::OpenAi | ProviderTag::OpenAiResponses => is_id_or_snapshot(&model, "gpt-6-astra"),
        ProviderTag::Gemini => model == "gemini-4-carbon",
        ProviderTag::Local => false,
    };
    if trusted {
        ModelTrust::Trusted
    } else {
        ModelTrust::Untested
    }
}

/// The families David trusts, in Anthropic's id shape: `claude-<family>-<version>`, where
/// everything after the family is version digits and dashes (`claude-sonnet-5-5`,
/// `claude-opus-4-1-20250805`). A suffix that isn't a version (`-mini`) or the older
/// `claude-3-5-sonnet` shape doesn't match.
fn is_trusted_claude(model: &str) -> bool {
    const TRUSTED_FAMILIES: [&str; 3] = ["opus", "fable", "sonnet"];
    let Some(rest) = model.strip_prefix("claude-") else {
        return false;
    };
    TRUSTED_FAMILIES.iter().any(|family| {
        rest.strip_prefix(family)
            .and_then(|version| version.strip_prefix('-'))
            .is_some_and(|version| {
                version.starts_with(|c: char| c.is_ascii_digit())
                    && version.bytes().all(|b| b.is_ascii_digit() || b == b'-')
            })
    })
}

/// `model` is `id` itself, or a dated snapshot of it in OpenAI's shape (`-2026-10-01`).
fn is_id_or_snapshot(model: &str, id: &str) -> bool {
    model.strip_prefix(id).is_some_and(|rest| {
        rest.is_empty()
            || rest.strip_prefix('-').is_some_and(|date| {
                let parts: Vec<&str> = date.split('-').collect();
                matches!(parts.as_slice(), [y, m, d]
                    if y.len() == 4 && m.len() == 2 && d.len() == 2
                        && date.bytes().all(|b| b.is_ascii_digit() || b == b'-'))
            })
    })
}

/// A meter onto `main.db`'s reserved `about_this` row, or `None` when the store or the
/// row isn't there.
fn ledger_meter(app: &AppHandle, provider: ProviderTag, model: String) -> Option<Meter> {
    use tauri::Manager;
    let db_path: PathBuf = app.try_state::<crate::agent::AgentDb>()?.db_path().to_path_buf();
    let conn = crate::agent::store::open_write_connection(&db_path).ok()?;
    let ledger = crate::agent::store::about_this_conversation(&conn).ok()?;
    Some(Arc::new(move |usage| {
        meter_onto(&db_path, ledger, provider, &model, usage)
    }))
}

/// Prices `usage` and folds it onto the ledger row. A write that fails is logged and
/// the priced answer still goes back: the card shows what it cost either way.
fn meter_onto(
    db_path: &std::path::Path,
    ledger: i64,
    provider: ProviderTag,
    model: &str,
    usage: AgentUsage,
) -> PricedCost {
    let priced = crate::agent::pricing::price_call(provider, model, usage);
    let record = crate::agent::store::CostRecord {
        day: crate::agent::chat::runtime::day_for(
            chrono::Utc::now().timestamp(),
            crate::agent::chat::session::local_offset(),
        ),
        conversation_id: ledger,
        provider,
        model: model.to_string(),
        prompt_tokens: u64::from(usage.prompt_tokens),
        completion_tokens: u64::from(usage.completion_tokens),
        cache_read_tokens: u64::from(usage.cache_read_tokens),
        cache_write_tokens: u64::from(usage.cache_write_tokens),
        cost_micros: priced.cost_micros,
        priced: priced.priced,
    };
    let written = crate::agent::store::open_write_connection(db_path)
        .and_then(|conn| crate::agent::store::record_cost(&conn, &record));
    if let Err(e) = written {
        log::warn!(target: "about", "the verdict's cost wasn't recorded: {e}");
    }
    priced
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(provider: &str) -> GateInputs<'_> {
        GateInputs {
            provider,
            verdict_on: true,
        }
    }

    /// A verdict's cost lands on the reserved row, and the row holds cost only: no message,
    /// no title, nothing of the verdict or its prompt. An unpriced model records its tokens
    /// as unpriced, never as $0.
    #[test]
    fn the_cost_lands_on_the_ledger_row_with_no_text() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = crate::agent::store::main_db_path(dir.path());
        let store = crate::agent::store::AgentStore::open(&db_path).expect("open");
        let ledger = crate::agent::store::about_this_conversation(store.conn()).expect("the row");
        let usage = AgentUsage {
            prompt_tokens: 900,
            completion_tokens: 40,
            ..AgentUsage::default()
        };
        let priced = meter_onto(&db_path, ledger, ProviderTag::Anthropic, "claude-haiku-5-5", usage);
        assert!(priced.priced && priced.cost_micros > 0);
        let unpriced = meter_onto(
            &db_path,
            ledger,
            ProviderTag::Anthropic,
            "claude-from-the-future",
            usage,
        );
        assert!(!unpriced.priced);

        let cost = crate::agent::store::conversation_cost(store.conn(), ledger).expect("cost");
        assert_eq!(cost.prompt_tokens, 1_800);
        assert!(!cost.fully_priced, "one unpriced call makes the total a lower bound");
        let messages: i64 = store
            .conn()
            .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
            .expect("count");
        assert_eq!(messages, 0, "a verdict writes no message");
        let title: String = store
            .conn()
            .query_row("SELECT title FROM conversations WHERE id = ?1", [ledger], |row| {
                row.get(0)
            })
            .expect("title");
        assert_eq!(title, "");
    }

    /// Each refusal before the slot never asks the slot.
    #[test]
    fn a_refusal_before_the_slot_never_resolves_it() {
        let never = || -> Result<(), SlotRefusal> { panic!("the slot must not be asked") };
        assert_eq!(decide(inputs("off"), never), Err(VerdictUnavailable::AiOff));
        let off = GateInputs {
            verdict_on: false,
            ..inputs("cloud")
        };
        assert_eq!(decide(off, never), Err(VerdictUnavailable::TurnedOff));
    }

    /// Each tier, from the ids the providers publish. An id the table doesn't recognize is
    /// untested, never trusted, and a Haiku is often wrong in any provider's spelling.
    #[test]
    fn each_model_lands_in_its_tier() {
        use ModelTrust::{OftenWrong, Trusted, Untested};
        use ProviderTag::{Anthropic, Gemini, Local, OpenAi, OpenAiResponses};
        let cases = [
            (Anthropic, "claude-sonnet-5-5", Trusted),
            (Anthropic, "claude-sonnet-5-5-20261001", Trusted),
            (Anthropic, "claude-opus-5-5", Trusted),
            (Anthropic, "claude-fable-5-2", Trusted),
            (Anthropic, "Claude-Opus-4-1-20250805", Trusted),
            (OpenAi, "gpt-6-astra", Trusted),
            (OpenAiResponses, "gpt-6-astra", Trusted),
            (OpenAi, "gpt-6-astra-2026-10-01", Trusted),
            (Gemini, "gemini-4-carbon", Trusted),
            (Anthropic, "claude-haiku-5-5", OftenWrong),
            (Anthropic, "Claude-Haiku-4-5", OftenWrong),
            (OpenAi, "anthropic/claude-3.5-haiku", OftenWrong),
            (Local, "qwen3-4b-instruct", OftenWrong),
            (Local, "claude-opus-5-5", OftenWrong),
            // An aggregator's route to a trusted model: other call settings, so untested.
            (OpenAi, "anthropic/claude-sonnet-5.5", Untested),
            (OpenAi, "openai/gpt-6-astra", Untested),
            (OpenAi, "google/gemini-4-carbon", Untested),
            (Anthropic, "claude-sonnet-5-5-mini", Untested),
            (Anthropic, "claude-3-5-sonnet-20241022", Untested),
            (Anthropic, "claude-sonnetish-5", Untested),
            (Anthropic, "claude-mythos-5-2", Untested),
            (OpenAi, "gpt-6-astra-mini", Untested),
            (OpenAi, "gpt-6.1-sol", Untested),
            (OpenAi, "gpt-4.1-mini", Untested),
            (Gemini, "gemini-4-carbon-lite", Untested),
            (Gemini, "gemini-2.5-flash", Untested),
            (Anthropic, "", Untested),
        ];
        for (provider, model, tier) in cases {
            assert_eq!(model_trust(provider, model), tier, "{provider:?} {model}");
        }
    }

    /// The slot's refusals keep their type, and a local server that isn't running reads
    /// as not ready, not unconfigured.
    #[test]
    fn the_slots_refusals_map_to_the_cards_words() {
        use crate::managed_policy::ManagedAiRefusal;
        let refused = |refusal| move || -> Result<(), SlotRefusal> { Err(refusal) };
        assert_eq!(
            decide(inputs("cloud"), refused(SlotRefusal::NoCloudConsent)),
            Err(VerdictUnavailable::NoCloudConsent)
        );
        assert_eq!(
            decide(inputs("cloud"), refused(SlotRefusal::NotConfigured)),
            Err(VerdictUnavailable::NotConfigured)
        );
        assert_eq!(
            decide(inputs("local"), refused(SlotRefusal::NotConfigured)),
            Err(VerdictUnavailable::NotReady)
        );
        assert_eq!(
            decide(
                inputs("cloud"),
                refused(SlotRefusal::Managed(ManagedAiRefusal::CloudAiOff))
            ),
            Err(VerdictUnavailable::Managed {
                refusal: ManagedAiRefusal::CloudAiOff
            })
        );
        assert_eq!(decide(inputs("cloud"), || Ok::<_, SlotRefusal>(7)), Ok(7));
    }
}
