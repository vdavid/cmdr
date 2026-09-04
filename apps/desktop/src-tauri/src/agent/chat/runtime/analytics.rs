//! The PII-free PostHog event for one Ask Cmdr turn.
//!
//! Without this, the agent's funnel starts at the proposal layer, and a zero on the
//! north-star acceptance rate (`agent/suggested_ops/analytics.rs`) can't be read: "nobody
//! configured a provider", "chats happen but the model never proposes", and "the capture
//! path is broken" all look identical from the outside. `ask_cmdr_turn` is the denominator
//! that separates them, and `proposals` on it is the direct link to the layer below.
//!
//! ❌ Every property is categorical: an origin token, an outcome token, a typed failure
//! kind, the provider tag, and coarse count buckets. Never a prompt, a path, a file name,
//! a model reply, or anything a tool read.

use serde_json::{Value, json};

use super::types::{TurnParams, TurnResult, TurnTally, UserTurn};
use crate::agent::chat::stream::AgentErrorKindView;
use crate::analytics::item_count_bucket;

/// Reports one finished turn.
pub(super) fn turn_finished(params: &TurnParams<'_>, result: &TurnResult, tally: &TurnTally) {
    crate::analytics::posthog::capture("ask_cmdr_turn", turn_props(params, result, tally));
}

/// The event's properties. Pure (no I/O, no gating), so the vocabulary is unit-testable
/// without a running app — the same split `analytics/posthog.rs` uses for the body.
fn turn_props(params: &TurnParams<'_>, result: &TurnResult, tally: &TurnTally) -> Value {
    let (outcome, failure) = match result {
        TurnResult::Answered { .. } => ("answered", "none"),
        TurnResult::Cancelled => ("cancelled", "none"),
        TurnResult::Failed(kind) => ("failed", kind.as_token()),
    };
    json!({
        "origin": origin_token(params.user.as_ref()),
        "outcome": outcome,
        "failure": failure,
        "provider": params.provider.as_token(),
        "tool_turns": item_count_bucket(tally.tool_turns),
        "proposals": item_count_bucket(tally.proposals),
    })
}

/// What opened the turn. `resume` is the post-crash replay of a persisted thread, which is
/// neither a fresh user message nor a wake and would otherwise inflate whichever it was
/// folded into.
fn origin_token(user: Option<&UserTurn<'_>>) -> &'static str {
    match user {
        Some(UserTurn::Text(_)) => "text",
        Some(UserTurn::Wake(_)) => "wake",
        Some(UserTurn::Outcomes(_)) => "outcomes",
        None => "resume",
    }
}

/// Reports a send refused before a turn ever started.
///
/// The four gates in `commands/agent/chat.rs` (no store, no consent, no resolvable provider,
/// a local window under the floor) return before `run_turn`, so without this the TOP of the
/// funnel is invisible and "nobody configured a provider" looks identical to "nobody opened
/// the rail". Same event and same prop names as a turn, so one insight covers both.
pub(crate) fn send_refused(kind: AgentErrorKindView) {
    crate::analytics::posthog::capture("ask_cmdr_turn", refusal_props(kind));
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
        "tool_turns": item_count_bucket(0),
        "proposals": item_count_bucket(0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::chat::context::ContextEnvelope;
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
            user,
            cmdr_md: None,
            memory: None,
            envelope,
            offset: FixedOffset::east_opt(0).expect("UTC is a valid offset"),
            now_secs: 0,
            provider,
            model: "test-model".to_string(),
            prompt_budget: 1000,
        }
    }

    #[test]
    fn an_answered_turn_reports_its_origin_provider_and_counts() {
        let envelope = envelope();
        let p = params(Some(UserTurn::Text("hi")), ProviderTag::Anthropic, &envelope);
        let props = turn_props(
            &p,
            &TurnResult::Answered {
                assistant_message_id: 7,
            },
            &TurnTally {
                tool_turns: 3,
                proposals: 1,
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
        let props = turn_props(&p, &TurnResult::Cancelled, &TurnTally::default());

        assert_eq!(props["outcome"], json!("cancelled"));
        assert_eq!(props["failure"], json!("none"));
    }

    #[test]
    fn a_resumed_thread_is_its_own_origin() {
        // A post-crash replay has no opener. Folding it into `text` would count one user
        // message twice and make the wake/interactive split wrong.
        let envelope = envelope();
        let p = params(None, ProviderTag::Gemini, &envelope);
        let props = turn_props(&p, &TurnResult::Cancelled, &TurnTally::default());

        assert_eq!(props["origin"], json!("resume"));
    }

    /// Every property is categorical or a bucket: no value may look like a path, a name, or
    /// a prompt. The debug-build net in `posthog::sanitize_props` only warns, so the
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
            &TurnResult::Answered {
                assistant_message_id: 1,
            },
            &TurnTally {
                tool_turns: 1,
                proposals: 0,
            },
        );

        let map = props.as_object().expect("props are an object");
        for (key, value) in map {
            let s = value.as_str().unwrap_or_else(|| panic!("{key} is a string token"));
            assert!(
                !s.contains('/') && !s.contains('\\') && !s.contains('@') && s.len() <= 24,
                "prop '{key}' carries a non-categorical value: {s}"
            );
        }
    }

    #[test]
    fn a_refused_send_reports_the_gate_that_stopped_it() {
        // A send refused before `run_turn` (AI off, consent not accepted) never reaches the
        // loop, so without this the top of the funnel is invisible: "nobody has a provider"
        // and "nobody opened the rail" would both read as no events at all.
        let props = refusal_props(AgentErrorKindView::NoConsent);

        assert_eq!(props["origin"], json!("text"));
        assert_eq!(props["outcome"], json!("refused"));
        assert_eq!(props["failure"], json!("no_consent"));
        // Most refusals fire before the slot resolves, so there is no honest provider to name.
        assert_eq!(props["provider"], json!("unresolved"));
        assert_eq!(props["tool_turns"], json!("0"));
        assert_eq!(props["proposals"], json!("0"));
    }

    #[test]
    fn a_refusal_and_a_turn_report_the_same_prop_names() {
        // One event, one schema: a refusal must not invent a key a turn doesn't have, or a
        // PostHog insight over `ask_cmdr_turn` breaks the moment it includes refusals.
        let envelope = envelope();
        let p = params(Some(UserTurn::Text("hi")), ProviderTag::Anthropic, &envelope);
        let turn = turn_props(&p, &TurnResult::Cancelled, &TurnTally::default());
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
        assert_eq!(AgentErrorKindView::NoConsent.as_token(), "no_consent");
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
