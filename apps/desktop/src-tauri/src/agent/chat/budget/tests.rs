use super::*;

/// A slot on Automatic with no provider report yet: the first send after launch.
fn automatic(provider: ProviderTag, model: &str) -> ResolvedBudget {
    resolve_prompt_budget(BudgetInputs {
        provider,
        model,
        user_choice: None,
        local_context_tokens: MIN_LOCAL_CONTEXT_TOKENS,
        reported_window_tokens: None,
        reply_tokens: AGENT_MAX_OUTPUT_TOKENS as usize,
    })
    .expect("a cloud slot is never refused")
}

/// The whole window, less the reply's room, less the estimator's margin.
#[test]
fn automatic_spends_the_whole_window() {
    assert_eq!(budget_for_window(1_000_000), 839_800);
    assert_eq!(budget_for_window(200_000), 159_800);
    assert_eq!(budget_for_window(MIN_LOCAL_CONTEXT_TOKENS as usize), 17_652);
    // A window smaller than the reply's room leaves nothing, rather than wrapping around.
    assert_eq!(budget_for_window(4_096), 0);
}

/// A higher reasoning effort raises the reply's ceiling, and Automatic leaves exactly that much
/// of the window free, since thinking comes out of the same window as the prompt.
#[test]
fn automatic_leaves_the_slots_reply_room_free() {
    let resolved = resolve_prompt_budget(BudgetInputs {
        provider: ProviderTag::Anthropic,
        model: "claude-opus-5-5",
        user_choice: None,
        local_context_tokens: 0,
        reported_window_tokens: Some(1_000_000),
        reply_tokens: 64_000,
    })
    .expect("a cloud slot is never refused");
    assert_eq!(resolved.prompt_tokens, (1_000_000 - 64_000) * 85 / 100);
}

#[test]
fn automatic_follows_the_family_table() {
    for (provider, model, window) in [
        (ProviderTag::Anthropic, "claude-sonnet-4-5", 200_000),
        (ProviderTag::Anthropic, "claude-sonnet-4-5-20250929", 200_000),
        (ProviderTag::Anthropic, "claude-sonnet-4-20250514", 200_000),
        (ProviderTag::Anthropic, "claude-opus-5-5", 1_000_000),
        (ProviderTag::Anthropic, "claude-sonnet-4-6", 1_000_000),
        (ProviderTag::OpenAi, "gpt-4o-mini", 128_000),
        (ProviderTag::OpenAi, "gpt-4.1", 1_000_000),
        (ProviderTag::OpenAiResponses, "gpt-5", 400_000),
        (ProviderTag::OpenAiResponses, "gpt-5.2-chat-latest", 128_000),
        (ProviderTag::OpenAiResponses, "gpt-5.5", 1_050_000),
        (ProviderTag::OpenAiResponses, "gpt-5.4-mini", 400_000),
        (ProviderTag::OpenAiResponses, "gpt-6.1-terra", 1_050_000),
        (ProviderTag::OpenAi, "o4-mini", 200_000),
        (ProviderTag::Gemini, "gemini-2.5-flash", 1_000_000),
        (ProviderTag::Gemini, "gemini-3.8-flash", 1_000_000),
        (ProviderTag::Gemini, "gemini-2.5-flash-image", 32_768),
    ] {
        let resolved = automatic(provider, model);
        assert_eq!(
            (resolved.prompt_tokens, resolved.source, resolved.known_window_tokens),
            (budget_for_window(window), BudgetSource::FamilyTable, Some(window)),
            "{model} has a {window}-token window in the table, and Automatic spends all of it"
        );
    }
}

/// What the provider says beats the table, in both directions: the table is a floor
/// someone verified once, the report is today's answer.
#[test]
fn a_provider_reported_window_wins_over_the_table() {
    for (model, reported) in [("claude-sonnet-4-5", 1_000_000), ("claude-opus-5-5", 200_000)] {
        let resolved = resolve_prompt_budget(BudgetInputs {
            provider: ProviderTag::Anthropic,
            model,
            user_choice: None,
            local_context_tokens: 0,
            reported_window_tokens: Some(reported),
            reply_tokens: AGENT_MAX_OUTPUT_TOKENS as usize,
        })
        .expect("a cloud slot is never refused");
        assert_eq!(
            (resolved.prompt_tokens, resolved.source, resolved.known_window_tokens),
            (
                budget_for_window(reported),
                BudgetSource::ProviderReported,
                Some(reported)
            )
        );
    }
    // A model the table has never heard of gets its whole window once the provider says.
    let reported = known_window(ProviderTag::OpenAi, "kimi-k3", 0, Some(1_048_576));
    assert_eq!(
        reported,
        Some(KnownWindow {
            tokens: 1_048_576,
            source: WindowSource::Provider
        })
    );
}

/// A local model's window is the server's own setting; no cloud report can move it.
#[test]
fn a_local_window_ignores_any_report() {
    assert_eq!(
        known_window(ProviderTag::Local, "claude-opus-5-5", 65_536, Some(1_000_000)),
        Some(KnownWindow {
            tokens: 65_536,
            source: WindowSource::LocalServer
        })
    );
}

/// A wake's digest is a summary; a 1M window doesn't make it worth 168,000 tokens a wake.
#[test]
fn a_wake_digest_is_capped_on_a_large_window() {
    assert_eq!(wake_digest_budget(budget_for_window(1_000_000)), WAKE_DIGEST_MAX_TOKENS);
    assert_eq!(
        wake_digest_budget(budget_for_window(MIN_LOCAL_CONTEXT_TOKENS as usize)),
        1_787,
        "a small window still gets its fifth, below the cap"
    );
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
        "claude-sonnet-5-5",                                 // Anthropic
        "gemini-3.8-flash",                                  // Google Gemini
        "openai/gpt-oss-120b",                               // Groq
        "meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8", // Together AI
        "accounts/fireworks/models/glm-5p3",                 // Fireworks AI
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
        assert!(
            resolved.known_window_tokens.is_some_and(|window| window > 100_000),
            "the table gives {model} a window of at least 100,000"
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
        reported_window_tokens: None,
        reply_tokens: AGENT_MAX_OUTPUT_TOKENS as usize,
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
        reported_window_tokens: None,
        reply_tokens: AGENT_MAX_OUTPUT_TOKENS as usize,
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
        reported_window_tokens: None,
        reply_tokens: AGENT_MAX_OUTPUT_TOKENS as usize,
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
        reported_window_tokens: None,
        reply_tokens: AGENT_MAX_OUTPUT_TOKENS as usize,
    })
    .expect("a window at or above the floor resolves");
    assert_eq!(
        (resolved.prompt_tokens, resolved.source),
        (17_652, BudgetSource::LocalServerWindow),
        "the whole window less the reply's room and the estimator's margin"
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
        reported_window_tokens: None,
        reply_tokens: AGENT_MAX_OUTPUT_TOKENS as usize,
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
            reported_window_tokens: None,
            reply_tokens: AGENT_MAX_OUTPUT_TOKENS as usize,
        })
        .is_err(),
        "the window is the server's, not the setting's, so the refusal stands"
    );
}

/// The assertion that decides where [`MIN_LOCAL_CONTEXT_TOKENS`] has to sit. A tool
/// joining the agent view grows the prefix, and this is what notices when the growth has
/// left the smallest window unable to hold a turn. ❌ Don't answer a failure here by
/// shrinking the reply's room or the margin; raise the floor, or make the prefix smaller.
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

/// ⚠️ Memory is a PERMANENT tax: the system string is never elided, so whatever it claims
/// is gone from every turn of every thread for good. And the agent writes the file itself,
/// so a flat byte cap would let it degrade its own chat with no way back. At the smallest
/// window the app supports, a memory file sitting on its 64 KB disk cap has to be cut hard.
#[test]
fn memory_claims_a_share_of_the_budget_rather_than_a_fixed_block() {
    let floored = budget_for_window(MIN_LOCAL_CONTEXT_TOKENS as usize);
    let on_the_floor = memory_slice_bytes(floored);

    assert!(
        on_the_floor < crate::agent::memory::MEMORY_DIR_MAX_BYTES as usize,
        "a memory file at its disk cap must be CUT at the smallest window, not carried whole: \
         {} of a {floored}-token budget",
        crate::pluralize::pluralize(on_the_floor as u64, "byte")
    );
    let workable = floored - FIXED_PROMPT_OVERHEAD_TOKENS;
    assert!(
        on_the_floor / CHARS_PER_TOKEN_ESTIMATE * 5 < workable,
        "memory must leave the conversation the bulk of what the prefix didn't take: \
         {} against {workable} workable tokens",
        crate::pluralize::pluralize(on_the_floor as u64, "byte")
    );
    assert!(
        memory_slice_bytes(60_000) > on_the_floor,
        "a bigger window has to be able to carry more of it, or the share is a constant in disguise"
    );
    // On a 1M window the share passes the disk cap, so the disk cap is what binds and no
    // prompt-side cap is needed (see `memory_slice_bytes`).
    assert!(
        memory_slice_bytes(budget_for_window(1_000_000)) > crate::agent::memory::MEMORY_DIR_MAX_BYTES as usize,
        "a whole-window budget carries the whole memory folder"
    );
}

#[test]
fn a_batch_hint_derives_from_the_budget() {
    // (budget − 10% headroom − 8,717 of prefix) / 349 per file, while the prompt is what
    // binds.
    assert_eq!(files_per_batch(16_000), 16);
    assert_eq!(files_per_batch(32_000), 57);
    // Past roughly 49,000 the reply's own ceiling binds instead, and the hint stops
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

/// The "Chat memory changed" line names what the USER moved: the setting or the model. A new
/// effort (it only reserves more reply) or a provider report arriving for a model the table
/// already knows moves the actual budget, and must not move this figure.
#[test]
fn the_chat_memory_figure_ignores_the_reply_reservation_and_a_late_report() {
    let inputs = |reported_window_tokens, reply_tokens| BudgetInputs {
        provider: ProviderTag::Gemini,
        model: "gemini-3.8-flash",
        user_choice: None,
        local_context_tokens: 0,
        reported_window_tokens,
        reply_tokens,
    };
    let before = chat_memory_figure(inputs(None, AGENT_MAX_OUTPUT_TOKENS as usize));
    assert_eq!(before, chat_memory_figure(inputs(None, 64_000)), "Max effort moved it");
    assert_eq!(
        before,
        chat_memory_figure(inputs(Some(1_048_576), AGENT_MAX_OUTPUT_TOKENS as usize)),
        "the catalog arriving moved it"
    );
    // A model only the report knows: the report is all there is, so it counts.
    let unknown = |reported| BudgetInputs {
        provider: ProviderTag::OpenAi,
        model: "kimi-k3",
        user_choice: None,
        local_context_tokens: 0,
        reported_window_tokens: reported,
        reply_tokens: AGENT_MAX_OUTPUT_TOKENS as usize,
    };
    assert_ne!(
        chat_memory_figure(unknown(None)),
        chat_memory_figure(unknown(Some(1_048_576)))
    );
    // An explicit size is the figure, whatever else moved.
    let explicit = BudgetInputs {
        user_choice: Some(60_000),
        ..inputs(Some(1_048_576), 64_000)
    };
    assert_eq!(chat_memory_figure(explicit), Ok(60_000));
}

/// Prompt plus reply must fit the window: a 200,000 size on a 200,000-token model with High's
/// 32,000 reply is over, even though the size alone isn't.
#[test]
fn a_size_that_leaves_no_room_for_the_reply_is_over_the_window() {
    let resolved = resolve_prompt_budget(BudgetInputs {
        provider: ProviderTag::Anthropic,
        model: "claude-opus-4-5",
        user_choice: Some(200_000),
        local_context_tokens: 0,
        reported_window_tokens: None,
        reply_tokens: 32_000,
    })
    .expect("a cloud slot is never refused");
    assert!(resolved.over_known_window);
}
