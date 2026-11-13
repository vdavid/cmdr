//! Whether a card gets a verdict, decided before anything is read for a model.
//!
//! ❗ **The order is the consent guarantee**: the provider, then the person's setting,
//! then the local-model default, then the slot (`resolve_agent_llm`, which carries the
//! managed policy, cloud consent, and the E2E fake). Only a slot that resolved builds a
//! [`Ready`], and the peek and the prompt take one, so a refusal here reads no file for
//! a model and calls no model.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::AppHandle;

use super::super::event::VerdictUnavailable;
use super::prompt::PromptVariant;
use crate::agent::chat::session::{AgentSlot, SlotRefusal, resolve_agent_llm, resolve_verdict_call_settings};
use crate::agent::llm::AgentLlm;
use crate::agent::llm::types::{AgentUsage, CallSettings, ProviderTag};
use crate::agent::pricing::PricedCost;

/// Whether the local model writes verdicts. Off until the verdict eval measures its
/// answers good: flipping it is this one default.
pub(crate) const VERDICT_ON_LOCAL: bool = false;

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
    pub(crate) meter: Meter,
}

/// What the gate reads before the slot.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GateInputs<'a> {
    /// `ai.provider`: `off`, `local`, or `cloud`.
    pub(crate) provider: &'a str,
    /// The person's `aboutThis.aiVerdict`.
    pub(crate) verdict_on: bool,
    /// [`VERDICT_ON_LOCAL`].
    pub(crate) local_allowed: bool,
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
    if local && !inputs.local_allowed {
        return Err(VerdictUnavailable::LocalOff);
    }
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
        local_allowed: VERDICT_ON_LOCAL,
    };
    let (resolved, provider, model) = decide(inputs, || resolve_agent_llm(app, AgentSlot::AboutThis))?;
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
        model,
        call,
        meter,
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
            local_allowed: false,
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
        assert_eq!(decide(inputs("local"), never), Err(VerdictUnavailable::LocalOff));
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
        let local = GateInputs {
            local_allowed: true,
            ..inputs("local")
        };
        assert_eq!(
            decide(local, refused(SlotRefusal::NotConfigured)),
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
