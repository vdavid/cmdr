use super::*;

fn reported(levels: &[&str], modes: &[&str], default: Option<&str>) -> ReasoningCapabilities {
    ReasoningCapabilities {
        supported: Some(true),
        mandatory: None,
        effort_levels: levels.iter().map(|l| l.to_string()).collect(),
        default_effort: default.map(str::to_string),
        thinking_modes: modes.iter().map(|m| m.to_string()).collect(),
    }
}

const CLAUDE_LEVELS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

#[test]
fn anthropic_offers_its_reported_levels_off_and_recommends_high() {
    let caps = reported(CLAUDE_LEVELS, &["adaptive", "disabled"], None);
    let options = effort_options(ProviderTag::Anthropic, "claude-sonnet-5-5", Some(&caps));
    assert_eq!(options.reasons, Some(true));
    assert_eq!(
        options.levels,
        vec![
            EffortLevel::Low,
            EffortLevel::Medium,
            EffortLevel::High,
            EffortLevel::Max
        ],
        "`xhigh` has no level of ours, so it isn't offered"
    );
    assert!(options.can_turn_off, "`disabled` is a thinking mode it accepts");
    assert_eq!(
        options.recommended,
        Some(EffortLevel::High),
        "Anthropic's documented default"
    );
    assert_eq!(options.source, Some(KnowledgeSource::Provider));
}

#[test]
fn a_model_that_cannot_stop_thinking_offers_no_off() {
    let caps = reported(CLAUDE_LEVELS, &["adaptive"], None);
    assert!(!effort_options(ProviderTag::Anthropic, "claude-opus-5-5", Some(&caps)).can_turn_off);
}

#[test]
fn openrouter_reports_its_own_default_and_whether_reasoning_is_mandatory() {
    let mut caps = reported(&["max", "xhigh", "high", "medium", "low"], &[], Some("medium"));
    caps.mandatory = Some(true);
    let options = effort_options(ProviderTag::OpenAi, "openai/gpt-6.1-terra-pro", Some(&caps));
    assert_eq!(options.recommended, Some(EffortLevel::Medium));
    assert!(!options.can_turn_off);
    caps.mandatory = Some(false);
    assert!(effort_options(ProviderTag::OpenAi, "openai/gpt-6.1-terra-pro", Some(&caps)).can_turn_off);
}

#[test]
fn openai_reports_nothing_so_the_built_in_table_answers() {
    let options = effort_options(ProviderTag::OpenAiResponses, "gpt-5.5", None);
    assert_eq!(options.reasons, Some(true));
    assert_eq!(
        options.levels,
        vec![EffortLevel::Low, EffortLevel::Medium, EffortLevel::High]
    );
    assert_eq!(options.recommended, Some(EffortLevel::Medium));
    assert_eq!(options.source, Some(KnowledgeSource::BuiltIn));

    let plain = effort_options(ProviderTag::OpenAi, "gpt-4.1-mini", None);
    assert_eq!(plain.reasons, Some(false), "the table knows gpt-4.1 doesn't reason");
    assert!(plain.levels.is_empty());
}

#[test]
fn a_model_nothing_knows_about_offers_recommended_only() {
    let options = effort_options(ProviderTag::OpenAi, "deepseek-chat", None);
    assert_eq!(options.reasons, None);
    assert!(options.levels.is_empty() && !options.can_turn_off);
}

#[test]
fn a_reasoning_model_with_no_reported_levels_borrows_the_tables() {
    // Groq says `gpt-oss` reasons but lists no levels; the table knows them.
    let caps = ReasoningCapabilities {
        supported: Some(true),
        ..ReasoningCapabilities::default()
    };
    let options = effort_options(ProviderTag::OpenAi, "openai/gpt-oss-120b", Some(&caps));
    assert_eq!(
        options.levels,
        vec![EffortLevel::Low, EffortLevel::Medium, EffortLevel::High]
    );

    // Gemini says it thinks, and no levels; its 3.x line takes low and high only.
    let gemini = effort_options(ProviderTag::Gemini, "gemini-3.8-flash", Some(&caps));
    assert_eq!(gemini.levels, vec![EffortLevel::Low, EffortLevel::High]);
    assert_eq!(gemini.recommended, None, "Gemini thinks dynamically by default");
}

#[test]
fn a_model_the_provider_says_does_not_reason_offers_nothing() {
    let caps = ReasoningCapabilities {
        supported: Some(false),
        ..ReasoningCapabilities::default()
    };
    let options = effort_options(ProviderTag::OpenAi, "codestral-2508", Some(&caps));
    assert_eq!(options.reasons, Some(false));
    assert!(options.levels.is_empty());
}

#[test]
fn a_local_model_offers_nothing() {
    let options = effort_options(ProviderTag::Local, "gpt-oss-20b", None);
    assert_eq!(options.reasons, None);
    assert!(options.levels.is_empty());
}

fn claude() -> EffortOptions {
    effort_options(
        ProviderTag::Anthropic,
        "claude-sonnet-5-5",
        Some(&reported(CLAUDE_LEVELS, &["adaptive", "disabled"], None)),
    )
}

#[test]
fn recommended_sends_nothing_and_sizes_the_reply_for_what_the_provider_will_do() {
    let resolved = resolve_effort(&claude(), EffortChoice::Recommended, Some(128_000));
    assert_eq!(
        resolved.effort,
        ReasoningEffort::ProviderDefault,
        "the one level safe everywhere"
    );
    assert_eq!(
        resolved.max_output_tokens, 32_000,
        "Claude's default is high, so High's room"
    );
    assert_eq!(resolved.adjusted_to, None);
}

#[test]
fn an_offered_level_goes_out_as_chosen_with_room_to_think() {
    let options = claude();
    for (level, effort, room) in [
        (EffortLevel::Low, ReasoningEffort::Low, 12_000),
        (EffortLevel::Medium, ReasoningEffort::Medium, 16_000),
        (EffortLevel::High, ReasoningEffort::High, 32_000),
        (EffortLevel::Max, ReasoningEffort::Max, 64_000),
    ] {
        let resolved = resolve_effort(&options, EffortChoice::Level(level), Some(128_000));
        assert_eq!((resolved.effort, resolved.max_output_tokens), (effort, room));
    }
}

#[test]
fn the_reply_room_never_passes_what_the_model_can_write() {
    let resolved = resolve_effort(&claude(), EffortChoice::Level(EffortLevel::Max), Some(40_000));
    assert_eq!(resolved.max_output_tokens, 40_000);
    let tiny = resolve_effort(&claude(), EffortChoice::Level(EffortLevel::Low), Some(8_192));
    assert_eq!(
        tiny.max_output_tokens, 8_192,
        "a model that can't write 12,000 gets what it can"
    );
}

#[test]
fn a_level_the_model_cannot_take_is_mapped_to_the_nearest_it_can() {
    let gemini = effort_options(
        ProviderTag::Gemini,
        "gemini-3.8-flash",
        Some(&ReasoningCapabilities {
            supported: Some(true),
            ..ReasoningCapabilities::default()
        }),
    );
    // Medium sits between low and high: the tie goes to the cheaper one.
    let medium = resolve_effort(&gemini, EffortChoice::Level(EffortLevel::Medium), None);
    assert_eq!(medium.effort, ReasoningEffort::Low);
    assert_eq!(medium.adjusted_to, Some(EffortChoice::Level(EffortLevel::Low)));
    let max = resolve_effort(&gemini, EffortChoice::Level(EffortLevel::Max), None);
    assert_eq!(max.effort, ReasoningEffort::High);
}

#[test]
fn a_level_chosen_for_a_model_with_no_levels_falls_back_to_recommended() {
    // A stored choice from another model, or a model that stopped reasoning: never send a
    // level it can't take (a 400), send nothing.
    let unknown = effort_options(ProviderTag::OpenAi, "deepseek-chat", None);
    let resolved = resolve_effort(&unknown, EffortChoice::Level(EffortLevel::High), None);
    assert_eq!(resolved.effort, ReasoningEffort::ProviderDefault);
    assert_eq!(resolved.adjusted_to, Some(EffortChoice::Recommended));
    assert_eq!(resolved.max_output_tokens, AGENT_MAX_OUTPUT_TOKENS);
}

#[test]
fn recommended_on_a_reasoning_model_with_no_known_default_gets_medium_room() {
    let gemini = effort_options(
        ProviderTag::Gemini,
        "gemini-2.5-pro",
        Some(&ReasoningCapabilities {
            supported: Some(true),
            ..ReasoningCapabilities::default()
        }),
    );
    let resolved = resolve_effort(&gemini, EffortChoice::Recommended, Some(65_536));
    assert_eq!(resolved.max_output_tokens, 16_000);
}

#[test]
fn a_stored_choice_parses_and_anything_else_reads_as_recommended() {
    assert_eq!(EffortChoice::parse("high"), EffortChoice::Level(EffortLevel::High));
    // The picker offers no Off (#770): a pick stored before that reads as Recommended, so
    // nobody keeps a choice they can no longer see.
    assert_eq!(EffortChoice::parse("off"), EffortChoice::Recommended);
    assert_eq!(EffortChoice::parse("recommended"), EffortChoice::Recommended);
    assert_eq!(EffortChoice::parse("xhigh"), EffortChoice::Recommended);
}

#[test]
fn the_choice_is_keyed_by_provider_and_model() {
    let json = r#"{"anthropic/claude-opus-5-5":"max","openai/gpt-5.5":"low"}"#;
    assert_eq!(
        choice_for(json, "anthropic", "claude-opus-5-5"),
        EffortChoice::Level(EffortLevel::Max)
    );
    assert_eq!(
        choice_for(json, "openrouter", "claude-opus-5-5"),
        EffortChoice::Recommended
    );
    assert_eq!(choice_for("not json", "anthropic", "x"), EffortChoice::Recommended);
}

/// A small reported window can't give the reply 32,000 tokens: the prompt would get almost none
/// of it, and prompt plus `max_tokens` would overrun the window. The reply takes at most a
/// quarter of the window, never less than the base slot while the window can spare it.
#[test]
fn a_high_effort_on_a_32k_window_leaves_the_prompt_a_working_budget() {
    let high = resolve_effort(&claude(), EffortChoice::Level(EffortLevel::High), None);
    let reply = fit_reply_to_window(high.max_output_tokens, Some(32_768));
    assert_eq!(reply, AGENT_MAX_OUTPUT_TOKENS);
    let prompt = (32_768 - reply as usize) * 85 / 100;
    assert!(
        prompt >= MIN_CLOUD_PROMPT_TOKENS,
        "the prompt keeps room for the prefix and a paged result: {prompt}"
    );
}

#[test]
fn a_large_window_keeps_the_full_reply_room() {
    assert_eq!(fit_reply_to_window(64_000, Some(1_000_000)), 64_000);
    assert_eq!(
        fit_reply_to_window(64_000, None),
        64_000,
        "an unknown window changes nothing"
    );
    // A quarter of 200,000 is 50,000: Max gives way, High doesn't.
    assert_eq!(fit_reply_to_window(64_000, Some(200_000)), 50_000);
    assert_eq!(fit_reply_to_window(32_000, Some(200_000)), 32_000);
}

/// Below the base slot's comfort zone, the reply shrinks so the prompt keeps its floor, but never
/// under a usable minimum: a tiny window then overruns honestly rather than replying in nothing.
#[test]
fn a_tiny_window_shrinks_the_reply_to_keep_the_prompt_floor() {
    let reply = fit_reply_to_window(AGENT_MAX_OUTPUT_TOKENS, Some(24_000));
    assert!(reply < AGENT_MAX_OUTPUT_TOKENS);
    assert!((24_000 - reply as usize) * 85 / 100 >= MIN_CLOUD_PROMPT_TOKENS);
    assert_eq!(
        fit_reply_to_window(AGENT_MAX_OUTPUT_TOKENS, Some(8_192)),
        MIN_REPLY_TOKENS
    );
}
