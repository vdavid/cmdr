//! What the rail knows about the slot BEFORE anything is sent: whether a provider is set up at
//! all, and what every model call of a new chat costs at minimum.
//!
//! - **Readiness** is setup only: off, a cloud provider missing its key or endpoint, a local
//!   model not downloaded or not running. Cloud consent is the rail's gate's question and a
//!   managed refusal is the send's, so neither is answered here.
//! - **The baseline** is the prefix every call carries (system prompt, tool declarations,
//!   `CMDR.md`) plus the memory notes a new chat's first turn carries, against the budget the
//!   next send would resolve (`context::fresh_thread_tokens`). It's the
//!   honest floor of the gauge: no call of this chat can cost less.
//!
//! The pure decision is [`provider_readiness`]; [`slot_status`] gathers the live facts.

use serde::Serialize;
use tauri::AppHandle;

use super::session::{provider_and_model, resolve_prompt_budget};
use crate::agent::llm::types::ProviderTag;

/// Whether a send could reach a model, as far as SETUP goes. The rail warns on every variant
/// but `Ready`, and its Send button says why it's disabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ProviderReadiness {
    Ready,
    /// The AI provider is "off" (its default), or not one we recognize.
    Off,
    /// Cloud is picked and its provider needs an API key, but none is stored.
    CloudNeedsKey,
    /// Cloud is picked with no endpoint (a custom provider left blank).
    CloudNeedsEndpoint,
    /// Local is picked and its model isn't downloaded yet (or is still downloading).
    LocalNotSetUp,
    /// Local is picked, the model is there, and its server is neither running nor starting.
    LocalNotRunning,
}

/// The live facts [`provider_readiness`] decides from, as plain values so the decision is
/// testable without an app.
#[derive(Debug, Clone, Copy)]
pub struct ProviderFacts<'a> {
    pub provider: &'a str,
    pub cloud_requires_key: bool,
    pub cloud_key_set: bool,
    pub cloud_endpoint_set: bool,
    pub local_model_installed: bool,
    /// Running, or starting: a server on its way up (the app's own launch) isn't a setup gap.
    pub local_server_up: bool,
}

/// The pure decision. Mirrors `ai::manager::resolve_backend`'s setup checks, minus consent and
/// policy (see the module docs), plus the local model's install state, which the backend
/// resolution reads only as "is there a port".
pub fn provider_readiness(facts: ProviderFacts<'_>) -> ProviderReadiness {
    match facts.provider {
        "cloud" if facts.cloud_requires_key && !facts.cloud_key_set => ProviderReadiness::CloudNeedsKey,
        "cloud" if !facts.cloud_endpoint_set => ProviderReadiness::CloudNeedsEndpoint,
        "cloud" => ProviderReadiness::Ready,
        "local" if !facts.local_model_installed => ProviderReadiness::LocalNotSetUp,
        "local" if !facts.local_server_up => ProviderReadiness::LocalNotRunning,
        "local" => ProviderReadiness::Ready,
        _ => ProviderReadiness::Off,
    }
}

/// The slot as the rail sees it before a send.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SlotStatus {
    pub readiness: ProviderReadiness,
    /// The provider and model the next send would meter under, and whether that model has a
    /// price, so a chat with nothing metered yet can say "free", "about $0", or "cost unknown"
    /// honestly. `None` while AI is off: there is nothing to price.
    pub next_call: Option<NextCallPricing>,
    /// What every call of a new chat costs at minimum. `None` when no budget resolves (AI is
    /// off, or a local window under the floor, which the send refuses on its own).
    pub baseline: Option<BaselineUsage>,
}

#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NextCallPricing {
    pub provider: ProviderTag,
    pub priced: bool,
}

/// The prefix's estimated size against the budget the next send resolves. Both are `chars/4`
/// estimates, like every other gauge figure.
#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BaselineUsage {
    pub estimated_tokens: u32,
    pub budget_tokens: u32,
}

/// Gather the live facts and answer for the rail. Reads the same settings and the same prefix
/// parts a send would, at the moment it's asked; the rail re-asks when any of them may have moved.
pub fn slot_status(app: &AppHandle) -> SlotStatus {
    let readiness = if crate::test_mode::ask_cmdr_fake_active() {
        ProviderReadiness::Ready
    } else {
        live_readiness()
    };
    if readiness == ProviderReadiness::Off {
        return SlotStatus {
            readiness,
            next_call: None,
            baseline: None,
        };
    }

    let (provider, model) = if crate::test_mode::ask_cmdr_fake_active() {
        (ProviderTag::Local, "fake".to_string())
    } else {
        provider_and_model(crate::settings::load_ask_cmdr_interactive_model(app).as_deref())
    };
    let priced = crate::agent::pricing::price_call(provider, &model, 0, 0).priced;
    let baseline = resolve_prompt_budget(app, provider, &model)
        .ok()
        .map(|budget| BaselineUsage {
            estimated_tokens: u32::try_from(super::runtime::baseline_prompt_tokens(app, budget)).unwrap_or(u32::MAX),
            budget_tokens: u32::try_from(budget).unwrap_or(u32::MAX),
        });
    SlotStatus {
        readiness,
        next_call: Some(NextCallPricing { provider, priced }),
        baseline,
    }
}

fn live_readiness() -> ProviderReadiness {
    let provider = crate::ai::state::get_provider();
    let (api_key, base_url, _model) = crate::ai::state::get_cloud_config();
    // Only a local provider needs the install check, which reads the disk.
    let local = (provider == "local").then(crate::ai::manager::get_ai_runtime_status);
    provider_readiness(ProviderFacts {
        provider: &provider,
        cloud_requires_key: crate::ai::state::get_cloud_requires_api_key(),
        cloud_key_set: !api_key.is_empty(),
        cloud_endpoint_set: !base_url.is_empty(),
        local_model_installed: local.as_ref().is_some_and(|s| s.model_installed),
        local_server_up: local.as_ref().is_some_and(|s| s.server_running || s.server_starting),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(provider: &str) -> ProviderFacts<'_> {
        ProviderFacts {
            provider,
            cloud_requires_key: true,
            cloud_key_set: true,
            cloud_endpoint_set: true,
            local_model_installed: true,
            local_server_up: true,
        }
    }

    #[test]
    fn off_and_unknown_providers_read_as_off() {
        assert_eq!(provider_readiness(facts("off")), ProviderReadiness::Off);
        assert_eq!(provider_readiness(facts("carrier-pigeon")), ProviderReadiness::Off);
    }

    #[test]
    fn a_cloud_provider_needs_its_key_only_when_it_asks_for_one() {
        let no_key = ProviderFacts {
            cloud_key_set: false,
            ..facts("cloud")
        };
        assert_eq!(provider_readiness(no_key), ProviderReadiness::CloudNeedsKey);
        // Ollama, LM Studio, and custom endpoints run keyless.
        let keyless = ProviderFacts {
            cloud_requires_key: false,
            ..no_key
        };
        assert_eq!(provider_readiness(keyless), ProviderReadiness::Ready);
        let no_endpoint = ProviderFacts {
            cloud_endpoint_set: false,
            ..keyless
        };
        assert_eq!(provider_readiness(no_endpoint), ProviderReadiness::CloudNeedsEndpoint);
    }

    #[test]
    fn local_needs_its_model_then_its_server_and_a_starting_server_counts_as_up() {
        let missing = ProviderFacts {
            local_model_installed: false,
            local_server_up: false,
            ..facts("local")
        };
        assert_eq!(provider_readiness(missing), ProviderReadiness::LocalNotSetUp);
        let stopped = ProviderFacts {
            local_model_installed: true,
            ..missing
        };
        assert_eq!(provider_readiness(stopped), ProviderReadiness::LocalNotRunning);
        assert_eq!(provider_readiness(facts("local")), ProviderReadiness::Ready);
    }

    #[test]
    fn the_cloud_facts_never_decide_a_local_slot() {
        let local_without_key = ProviderFacts {
            cloud_key_set: false,
            cloud_endpoint_set: false,
            ..facts("local")
        };
        assert_eq!(provider_readiness(local_without_key), ProviderReadiness::Ready);
    }
}
