//! The PII-free analytics event for one Ask Cmdr turn.
//!
//! Without this, the agent's funnel starts at the proposal layer, and a zero on the
//! north-star acceptance rate (`agent/suggested_ops/analytics.rs`) can't be read: "nobody
//! configured a provider", "chats happen but the model never proposes", and "the capture
//! path is broken" all look identical from the outside. `ask_cmdr_turn` is the denominator
//! that separates them, and `proposals` on it is the direct link to the layer below.
//!
//! ❌ Every property is categorical: an origin token, an outcome token, a typed failure
//! kind, the provider tag and preset id, a public model id or `other`, coarse count buckets,
//! and a bool. Never a prompt, a path, a file name, a model reply, or anything a tool read.

use serde_json::{Value, json};

use super::types::{TurnParams, TurnResult, TurnTally};
use crate::agent::chat::stream::AgentErrorKindView;
use crate::agent::llm::types::ProviderTag;
use crate::agent::store::TurnLimitKind;
use crate::analytics::item_count_bucket;

/// Reports one finished turn.
pub(super) fn turn_finished(params: &TurnParams<'_>, result: &TurnResult, tally: &TurnTally) {
    let preset = crate::ai::state::get_cloud_provider_id();
    crate::analytics::events::capture("ask_cmdr_turn", turn_props(params, &preset, result, tally));
}

/// The event's properties. Pure (no I/O, no gating), so the vocabulary is unit-testable
/// without a running app — the same split `analytics/events.rs` uses for the body. `preset` is
/// the cloud provider preset id (`ai.cloudProvider`), which the heartbeat's config shape already
/// ships as categorical.
fn turn_props(params: &TurnParams<'_>, preset: &str, result: &TurnResult, tally: &TurnTally) -> Value {
    let (outcome, failure) = match result {
        TurnResult::Answered { .. } => ("answered", "none"),
        TurnResult::Cancelled => ("cancelled", "none"),
        TurnResult::Failed(kind) => ("failed", kind.as_token()),
    };
    let local = params.provider == ProviderTag::Local;
    json!({
        "origin": params.origin.as_token(),
        "outcome": outcome,
        "failure": failure,
        "provider": params.provider.as_token(),
        "preset": if local { "local" } else { preset_token(preset) },
        "model": if local { "local" } else { model_token(preset, &params.model) },
        "tool_turns": item_count_bucket(tally.tool_turns),
        "proposals": item_count_bucket(tally.proposals),
        // How busy the provider and model ran (`retry.rs`), and whether a call went quiet for the
        // whole stall window: the two "high demand" signals the Gemini default is judged by.
        "busy_retries": item_count_bucket(tally.busy_retries as usize),
        "stalled": tally.limit.is_some_and(|limit| limit.kind == TurnLimitKind::Stalled),
    })
}

/// The cloud presets whose model ids come from the provider's public catalog. The rest name a
/// model the user named themselves (an Azure deployment, an Ollama or LM Studio tag, a custom
/// endpoint's model), which could say something about them, so those report `other`. A new
/// preset reports `other` until it's added here.
const CATALOG_PRESETS: &[&str] = &[
    "openai",
    "anthropic",
    "google-gemini",
    "groq",
    "together-ai",
    "fireworks-ai",
    "mistral",
    "openrouter",
    "deepseek",
    "qwen",
    "xai",
    "perplexity",
];

/// The preset id as sent, when it's one of ours (a short lowercase id), else `other`: an empty
/// id before `configure_ai`, or anything unexpected, never travels.
fn preset_token(preset: &str) -> &str {
    let ours = !preset.is_empty()
        && preset.len() <= 24
        && preset
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if ours { preset } else { "other" }
}

/// The model id when it's a public catalog id on a catalog preset (lowercase letters, digits,
/// dots, and dashes, as `gemini-3.8-flash` or `claude-sonnet-5-5` are), else `other`. ❌ A slash
/// or colon id stays out: a vendor route or a fine-tune (`ft:…:my-org:…`) can carry an
/// organization's name.
fn model_token<'a>(preset: &str, model: &'a str) -> &'a str {
    let catalog_shaped = !model.is_empty()
        && model.len() <= 48
        && model
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.');
    if CATALOG_PRESETS.contains(&preset) && catalog_shaped {
        model
    } else {
        "other"
    }
}

/// Reports a send refused before a turn ever started.
///
/// The four gates in `commands/agent/chat.rs` (no store, no consent, no resolvable provider,
/// a local window under the floor) return before `run_turn`, so without this the TOP of the
/// funnel is invisible and "nobody configured a provider" looks identical to "nobody opened
/// the rail". Same event and same prop names as a turn, so one insight covers both.
pub(crate) fn send_refused(kind: AgentErrorKindView) {
    crate::analytics::events::capture("ask_cmdr_turn", refusal_props(kind));
}

/// The refusal's properties. Pure, for the same reason [`turn_props`] is.
fn refusal_props(kind: AgentErrorKindView) -> Value {
    json!({
        // A refusal can only come from the composer; a wake never reaches this path.
        "origin": "text",
        "outcome": "refused",
        "failure": kind.as_token(),
        // Three of the four gates fire before the slot resolves, and reporting the settings
        // value instead would claim a provider was chosen for a send that never chose one.
        "provider": "unresolved",
        "preset": "unresolved",
        "model": "unresolved",
        "tool_turns": item_count_bucket(0),
        "proposals": item_count_bucket(0),
        "busy_retries": item_count_bucket(0),
        "stalled": false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::chat::context::ContextEnvelope;
    use crate::agent::chat::runtime::UserTurn;
    use crate::agent::chat::runtime::events::AgentErrorKind;
    use crate::agent::llm::types::ProviderTag;
    use chrono::FixedOffset;

    /// A minimal envelope: nothing in the props reads it, it just has to exist.
    fn envelope() -> ContextEnvelope {
        ContextEnvelope {
            captured_at: 1_780_000_000,
            focused_pane_path: None,
            cursor_item: None,
            selection_count: 0,
            volumes: vec![],
            attachments: vec![],
            denied_names: vec![],
            rename_batch_files: 101,
        }
    }

    /// A turn's params, minus the borrowed opener the caller supplies.
    fn params<'a>(user: Option<UserTurn<'a>>, provider: ProviderTag, envelope: &'a ContextEnvelope) -> TurnParams<'a> {
        TurnParams {
            conversation_id: 1,
            origin: super::super::test_support::origin_of(user.as_ref()),
            user,
            cmdr_md: None,
            memory: &[],
            envelope,
            offset: FixedOffset::east_opt(0).expect("UTC is a valid offset"),
            now_secs: 0,
            provider,
            model: "test-model".to_string(),
            prompt_budget: 1000,
            chat_memory_tokens: 1000,
            call: crate::agent::llm::CallSettings::default(),
            spend_pause: None,
        }
    }

    #[test]
    fn an_answered_turn_reports_its_origin_provider_and_counts() {
        let envelope = envelope();
        let p = params(Some(UserTurn::Text("hi")), ProviderTag::Anthropic, &envelope);
        let props = turn_props(
            &p,
            "anthropic",
            &TurnResult::Answered {
                assistant_message_id: 7,
            },
            &TurnTally {
                tool_turns: 3,
                proposals: 1,
                ..TurnTally::default()
            },
        );

        assert_eq!(props["origin"], json!("text"));
        assert_eq!(props["outcome"], json!("answered"));
        assert_eq!(props["failure"], json!("none"));
        assert_eq!(props["provider"], json!("anthropic"));
        assert_eq!(props["tool_turns"], json!("2-10"));
        assert_eq!(props["proposals"], json!("1"));
    }

    #[test]
    fn a_failure_carries_its_typed_kind_never_a_message() {
        let envelope = envelope();
        let p = params(Some(UserTurn::Text("hi")), ProviderTag::Local, &envelope);
        let props = turn_props(
            &p,
            "anthropic",
            &TurnResult::Failed(AgentErrorKind::RateLimited),
            &TurnTally::default(),
        );

        assert_eq!(props["outcome"], json!("failed"));
        assert_eq!(props["failure"], json!("rate_limited"));
        assert_eq!(props["provider"], json!("local"));
        // A turn that never reached the provider still reports honest zeros.
        assert_eq!(props["tool_turns"], json!("0"));
        assert_eq!(props["proposals"], json!("0"));
    }

    #[test]
    fn a_cancelled_turn_is_not_a_failure() {
        let envelope = envelope();
        let p = params(Some(UserTurn::Text("hi")), ProviderTag::OpenAi, &envelope);
        let props = turn_props(&p, "anthropic", &TurnResult::Cancelled, &TurnTally::default());

        assert_eq!(props["outcome"], json!("cancelled"));
        assert_eq!(props["failure"], json!("none"));
    }

    #[test]
    fn a_resumed_thread_is_its_own_origin() {
        // A post-crash replay has no opener. Folding it into `text` would count one user
        // message twice and make the wake/interactive split wrong.
        let envelope = envelope();
        let p = params(None, ProviderTag::Gemini, &envelope);
        let props = turn_props(&p, "anthropic", &TurnResult::Cancelled, &TurnTally::default());

        assert_eq!(props["origin"], json!("resume"));
    }

    /// Every property is categorical or a bucket: no value may look like a path, a name, or
    /// a prompt. The debug-build net in `events::sanitize_props` only warns, so the
    /// vocabulary has to be right here.
    #[test]
    fn every_prop_value_is_a_short_token_or_bucket() {
        let envelope = envelope();
        let p = params(
            Some(UserTurn::Text("please rename /Users/dave/photos")),
            ProviderTag::Anthropic,
            &envelope,
        );
        let props = turn_props(
            &p,
            "anthropic",
            &TurnResult::Answered {
                assistant_message_id: 1,
            },
            &TurnTally {
                tool_turns: 1,
                proposals: 0,
                ..TurnTally::default()
            },
        );

        let map = props.as_object().expect("props are an object");
        for (key, value) in map {
            if value.is_boolean() {
                continue;
            }
            let s = value
                .as_str()
                .unwrap_or_else(|| panic!("{key} is a string token or a bool"));
            // 48: a public model id (`model_token`) is the longest token.
            assert!(
                !s.contains('/') && !s.contains('\\') && !s.contains('@') && s.len() <= 48,
                "prop '{key}' carries a non-categorical value: {s}"
            );
        }
    }

    /// Per provider and model is what tells a busy Gemini default from a busy afternoon: a turn
    /// carries its preset, its public model id, how many tries the provider turned away, and
    /// whether a call went quiet for the whole stall window.
    #[test]
    fn a_turn_reports_its_preset_model_and_how_busy_the_provider_was() {
        let envelope = envelope();
        let mut p = params(Some(UserTurn::Text("hi")), ProviderTag::Gemini, &envelope);
        p.model = "gemini-3.8-flash".to_string();
        let props = turn_props(
            &p,
            "google-gemini",
            &TurnResult::Failed(AgentErrorKind::ProviderBusy),
            &TurnTally {
                busy_retries: 3,
                ..TurnTally::default()
            },
        );

        assert_eq!(props["failure"], json!("provider_busy"));
        assert_eq!(props["preset"], json!("google-gemini"));
        assert_eq!(props["model"], json!("gemini-3.8-flash"));
        assert_eq!(props["busy_retries"], json!("2-10"));
        assert_eq!(props["stalled"], json!(false));

        let stalled = turn_props(
            &p,
            "google-gemini",
            &TurnResult::Failed(AgentErrorKind::Timeout),
            &TurnTally {
                limit: Some(crate::agent::store::TurnLimit {
                    kind: TurnLimitKind::Stalled,
                    max: 120,
                    answered: false,
                }),
                ..TurnTally::default()
            },
        );
        assert_eq!(stalled["stalled"], json!(true));
        assert_eq!(stalled["busy_retries"], json!("0"));
    }

    /// ❌ A model the user named themselves never travels: an Azure deployment, a local tag, a
    /// custom endpoint's model, a fine-tune, or a vendor route all report `other`.
    #[test]
    fn only_a_public_catalog_model_id_is_reported() {
        assert_eq!(model_token("anthropic", "claude-sonnet-5-5"), "claude-sonnet-5-5");
        assert_eq!(model_token("openai", "gpt-4.1-mini"), "gpt-4.1-mini");
        assert_eq!(model_token("azure-openai", "acme-legal-gpt4"), "other");
        assert_eq!(model_token("custom", "gemma"), "other");
        assert_eq!(model_token("ollama", "llama3.2"), "other");
        assert_eq!(model_token("openai", "ft:gpt-4o:acme-corp:legal:abc123"), "other");
        assert_eq!(model_token("openrouter", "anthropic/claude-sonnet-5-5"), "other");
        assert_eq!(model_token("openai", "GPT-4o"), "other");
        assert_eq!(model_token("openai", ""), "other");

        assert_eq!(preset_token("google-gemini"), "google-gemini");
        assert_eq!(preset_token(""), "other");
        assert_eq!(preset_token("Weird Preset/1"), "other");

        let envelope = envelope();
        let p = params(Some(UserTurn::Text("hi")), ProviderTag::Local, &envelope);
        let props = turn_props(&p, "anthropic", &TurnResult::Cancelled, &TurnTally::default());
        assert_eq!(
            props["preset"],
            json!("local"),
            "a local model ignores the cloud preset"
        );
        assert_eq!(props["model"], json!("local"));
    }

    #[test]
    fn a_refused_send_reports_the_gate_that_stopped_it() {
        // A send refused before `run_turn` (AI off, consent not accepted) never reaches the
        // loop, so without this the top of the funnel is invisible: "nobody has a provider"
        // and "nobody opened the rail" would both read as no events at all.
        let props = refusal_props(AgentErrorKindView::AskCmdrOff);

        assert_eq!(props["origin"], json!("text"));
        assert_eq!(props["outcome"], json!("refused"));
        assert_eq!(props["failure"], json!("ask_cmdr_off"));
        // Most refusals fire before the slot resolves, so there is no honest provider to name.
        assert_eq!(props["provider"], json!("unresolved"));
        assert_eq!(props["tool_turns"], json!("0"));
        assert_eq!(props["proposals"], json!("0"));
    }

    #[test]
    fn a_refusal_and_a_turn_report_the_same_prop_names() {
        // One event, one schema: a refusal must not invent a key a turn doesn't have, or a
        // query over `ask_cmdr_turn` breaks the moment it includes refusals.
        let envelope = envelope();
        let p = params(Some(UserTurn::Text("hi")), ProviderTag::Anthropic, &envelope);
        let turn = turn_props(&p, "anthropic", &TurnResult::Cancelled, &TurnTally::default());
        let refusal = refusal_props(AgentErrorKindView::NotConfigured);

        let turn_keys: std::collections::BTreeSet<&String> = turn.as_object().expect("object").keys().collect();
        let refusal_keys: std::collections::BTreeSet<&String> = refusal.as_object().expect("object").keys().collect();
        assert_eq!(turn_keys, refusal_keys);
    }

    /// The runtime enum and the view the frontend sees are authored separately, and both
    /// tokenize into the SAME `failure` prop. If they drifted, one gate would report under
    /// two names and the funnel's top and middle could no longer be added up.
    #[test]
    fn the_two_error_vocabularies_cannot_drift() {
        for kind in [
            AgentErrorKind::NoKey,
            AgentErrorKind::NotConfigured,
            AgentErrorKind::Unavailable,
            AgentErrorKind::Timeout,
            AgentErrorKind::AuthFailed,
            AgentErrorKind::RateLimited,
            AgentErrorKind::ModelNotInPlan,
            AgentErrorKind::RequestTooLarge,
            AgentErrorKind::ProviderBusy,
            AgentErrorKind::BudgetExhausted,
            AgentErrorKind::RepeatedToolCall,
            AgentErrorKind::UnfinishedReply,
            AgentErrorKind::Provider,
        ] {
            let view: AgentErrorKindView = kind.into();
            assert_eq!(
                kind.as_token(),
                view.as_token(),
                "{kind:?} tokenizes differently on the runtime and view enums"
            );
        }
    }

    /// The view carries two gates the runtime enum has no variant for. They're the whole
    /// reason the refusal event exists, so they need tokens of their own.
    #[test]
    fn the_pre_turn_only_gates_have_their_own_tokens() {
        assert_eq!(AgentErrorKindView::AskCmdrOff.as_token(), "ask_cmdr_off");
        assert_eq!(AgentErrorKindView::NoCloudConsent.as_token(), "no_cloud_consent");
        assert_eq!(
            AgentErrorKindView::LocalWindowTooSmall.as_token(),
            "local_window_too_small"
        );
    }

    /// Every `AgentErrorKind` needs a distinct token, or two different failure modes merge
    /// into one number and the event stops answering why turns end without an answer.
    #[test]
    fn every_error_kind_has_a_unique_token() {
        let kinds = [
            AgentErrorKind::NoKey,
            AgentErrorKind::NotConfigured,
            AgentErrorKind::Unavailable,
            AgentErrorKind::Timeout,
            AgentErrorKind::AuthFailed,
            AgentErrorKind::RateLimited,
            AgentErrorKind::ModelNotInPlan,
            AgentErrorKind::RequestTooLarge,
            AgentErrorKind::ProviderBusy,
            AgentErrorKind::BudgetExhausted,
            AgentErrorKind::UnfinishedReply,
            AgentErrorKind::Provider,
        ];
        let tokens: std::collections::BTreeSet<&str> = kinds.iter().map(|k| k.as_token()).collect();
        assert_eq!(tokens.len(), kinds.len(), "two error kinds share a token");
        // `none` is the no-failure sentinel on the event; no kind may claim it.
        assert!(
            !tokens.contains("none"),
            "a failure kind must not be tokenized as 'none'"
        );
    }
}
