//! Gated live smokes for the genai-backed [`GenaiAgentLlm`], one per Tier-1 cloud
//! provider plus the local slot. Each drives ONE real streaming `respond` call
//! through the whole seam (build request → exec → map deltas → final message), so a
//! regression in adapter routing, tool-call mapping, or stream handling fails here
//! instead of silently in production. Each reasoning dialect also runs a real
//! multi-round tool loop with reasoning on (`smoke_*_tool_loop`), the case that proves
//! the provider accepts the reasoning the agent replays (`DETAILS.md` § Reasoning
//! round-trip records which ran green, and when).
//!
//! `smoke_*_deferred_tool` and `smoke_*_list_only_tool` run a whole turn on Ask Cmdr's real prefix, so a
//! provider that rejects the tool-search pair's schemas or a stored `use_tool` fails there, and what
//! each model did lands in the log (`RUST_LOG=info`, `--no-capture`).
//!
//! `#[ignore]`-gated on the matching env var, never in CI's critical path. Verify
//! current model ids from each provider's models endpoint at run time — never from
//! training data. Run manually, for example:
//!
//! ```sh
//! ANTHROPIC_API_KEY=$(secret ANTHROPIC_API_KEY_CMDR_DEV) \
//!   cargo nextest run --lib --run-ignored only agent::llm::live_smoke_test
//! OPENAI_API_KEY=$(secret OPENAI_API_KEY)   cargo nextest run --lib --run-ignored only agent::llm::live_smoke_test::smoke_openai
//! GEMINI_API_KEY=$(secret GEMINI_API_KEY)   cargo nextest run --lib --run-ignored only agent::llm::live_smoke_test::smoke_gemini
//! # Other loops read MISTRAL_API_KEY, GROQ_API_KEY, FIREWORKS_AI_API_KEY, OPENROUTER_API_KEY.
//! # Local: point at a running Cmdr llama-server.
//! LLAMA_PORT=18437 cargo nextest run --lib --run-ignored only agent::llm::live_smoke_test::smoke_local
//! ```

use futures_util::stream::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::ai::client::AiBackend;

use super::AgentLlm;
use super::genai_impl::GenaiAgentLlm;
use super::types::{
    AgentDelta, AgentMessage, AgentPart, AgentRole, AgentStopReason, AgentToolResult, AgentUsage, ProviderTag,
    ReasoningEffort, ToolDeclaration, ToolId,
};

fn env_or_skip(var: &str) -> String {
    match std::env::var(var) {
        Ok(value) if !value.trim().is_empty() => value,
        _ => panic!("{var} not set"),
    }
}

fn user_turn(text: &str) -> Vec<AgentMessage> {
    vec![AgentMessage {
        role: AgentRole::User,
        parts: vec![AgentPart::Text(text.to_string())],
        at: 0,
    }]
}

/// Drives one `respond` call to completion and returns the joined visible text of
/// the final assembled message. Asserts the stream ended with an `End` delta.
async fn run_one(backend: AiBackend, prompt: &str) -> String {
    let llm = GenaiAgentLlm::new(backend);
    let messages = user_turn(prompt);
    let stream = llm
        .respond(
            "You are a terse assistant. Answer in one short sentence.",
            &[],
            &messages,
            crate::agent::llm::CallSettings::default(),
            CancellationToken::new(),
        )
        .await
        .expect("respond should open a stream");

    let deltas: Vec<AgentDelta> = stream.map(|item| item.expect("no stream error")).collect().await;

    let Some(AgentDelta::End { message, .. }) = deltas.last() else {
        panic!("stream did not end with an End delta");
    };
    message
        .parts
        .iter()
        .filter_map(|part| match part {
            AgentPart::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// What one multi-step tool loop saw, for the per-model assertions.
struct LoopOutcome {
    tool_rounds: usize,
    /// `ReasoningTick` deltas across the loop: proof a thinking block came back, since
    /// Claude 5.x omits the thinking text and genai keeps no Anthropic signature.
    reasoning_ticks: usize,
    /// Reasoning the loop persisted and replayed: standalone `Reasoning` parts plus tool calls
    /// carrying their own signature.
    reasoning_parts: usize,
    answer: String,
    /// Each call's usage, in order: the cache smokes read the reads and writes off it.
    calls: Vec<AgentUsage>,
}

/// Canned `list_dir` answers. The root names three `reports-N` folders and only the
/// one the prompt's arithmetic picks holds the PDFs, so the loop needs at least two
/// tool rounds, and the arithmetic coaxes a thinking model into a thinking block that
/// the next request then replays (or drops) ahead of the tool result.
fn fake_list_dir(arguments: &serde_json::Value) -> serde_json::Value {
    let path = arguments.get("path").and_then(|p| p.as_str()).unwrap_or_default();
    let path = path.trim_end_matches('/');
    if path.ends_with("reports-1592") {
        serde_json::json!({ "entries": [
            { "name": "q1.pdf", "kind": "file", "size": 120_000 },
            { "name": "q2.pdf", "kind": "file", "size": 98_000 },
            { "name": "q3.pdf", "kind": "file", "size": 143_000 },
        ]})
    } else if path.ends_with("reports-1591") || path.ends_with("reports-1593") {
        serde_json::json!({ "entries": [] })
    } else {
        serde_json::json!({ "entries": [
            { "name": "reports-1591", "kind": "folder" },
            { "name": "reports-1592", "kind": "folder" },
            { "name": "reports-1593", "kind": "folder" },
            { "name": "notes.txt", "kind": "file", "size": 2_000 },
        ]})
    }
}

/// Drives a real tool loop through `GenaiAgentLlm` the way `run_turn` does: each
/// `End` message is replayed verbatim as the assistant turn (reasoning parts and
/// all), followed by a tool turn with the results, until the model answers in prose.
async fn run_tool_loop(backend: AiBackend, effort: ReasoningEffort) -> LoopOutcome {
    run_tool_loop_with(
        backend,
        effort,
        "You are a file assistant. Use the tools; never guess folder contents.",
    )
    .await
}

async fn run_tool_loop_with(backend: AiBackend, effort: ReasoningEffort, system: &str) -> LoopOutcome {
    let llm = GenaiAgentLlm::new(backend);
    let tools = [ToolDeclaration {
        name: ToolId::ListDir,
        description: "List one directory's immediate children.".into(),
        schema: serde_json::json!({
            "type": "object",
            "properties": { "path": { "type": "string", "description": "Absolute folder path" } },
            "required": ["path"],
        }),
    }];
    let mut messages = user_turn(
        "Call list_dir on /Users/test/work. Then work out N = 37 * 43 + 1 and call \
         list_dir on the reports-N folder it shows. How many PDF files are in it? \
         Answer with the number.",
    );
    let mut outcome = LoopOutcome {
        tool_rounds: 0,
        reasoning_ticks: 0,
        reasoning_parts: 0,
        answer: String::new(),
        calls: Vec::new(),
    };

    for _ in 0..5 {
        let stream = llm
            .respond(
                system,
                &tools,
                &messages,
                crate::agent::llm::CallSettings {
                    effort,
                    ..crate::agent::llm::CallSettings::default()
                },
                CancellationToken::new(),
            )
            .await
            .unwrap_or_else(|error| panic!("respond failed on step {}: {error:?}", outcome.tool_rounds + 1));
        let deltas: Vec<AgentDelta> = stream
            .map(|item| item.unwrap_or_else(|error| panic!("stream error: {error:?}")))
            .collect()
            .await;
        let Some(AgentDelta::End { stop, message, usage }) = deltas.last() else {
            panic!("stream did not end with an End delta");
        };
        outcome.calls.push(*usage);

        outcome.reasoning_ticks += deltas
            .iter()
            .filter(|delta| matches!(delta, AgentDelta::ReasoningTick))
            .count();
        outcome.reasoning_parts += message
            .parts
            .iter()
            .filter(|part| match part {
                AgentPart::Reasoning(_) => true,
                AgentPart::ToolCall(call) => call.reasoning.is_some(),
                _ => false,
            })
            .count();
        let results: Vec<AgentPart> = message
            .parts
            .iter()
            .filter_map(|part| match part {
                AgentPart::ToolCall(call) => Some(AgentPart::ToolResult(AgentToolResult {
                    call_id: call.call_id.clone(),
                    content: fake_list_dir(&call.arguments),
                    elided: false,
                })),
                _ => None,
            })
            .collect();
        messages.push(message.clone());

        if results.is_empty() {
            assert_eq!(*stop, AgentStopReason::Completed, "final turn should complete");
            outcome.answer = message
                .parts
                .iter()
                .filter_map(|part| match part {
                    AgentPart::Text(text) => Some(text.as_str()),
                    _ => None,
                })
                .collect();
            return outcome;
        }
        outcome.tool_rounds += 1;
        messages.push(AgentMessage {
            role: AgentRole::Tool,
            parts: results,
            at: 0,
        });
    }
    panic!("the model never answered after {} tool rounds", outcome.tool_rounds);
}

fn anthropic_backend(model: &str) -> AiBackend {
    AiBackend::remote_for_tests(
        env_or_skip("ANTHROPIC_API_KEY"),
        // The app's own base URL (`cloud-providers.ts`); without `/v1/` genai 404s.
        "https://api.anthropic.com/v1/".into(),
        model.into(),
    )
}

#[tokio::test]
#[ignore = "real API call: set ANTHROPIC_API_KEY to run"]
async fn smoke_anthropic() {
    let answer = run_one(anthropic_backend("claude-haiku-5-5"), "Say hello.").await;
    assert!(!answer.trim().is_empty(), "expected a visible answer, got {answer:?}");
}

/// Runs the multi-step loop with reasoning on and asserts it reached the right answer. A
/// provider that rejects the replayed reasoning fails on step 2 inside `run_tool_loop`.
/// `expect_reasoning` says the provider is known to hand back replayable reasoning on this
/// model, so a loop that persisted none means the capture broke.
async fn assert_tool_loop(label: &str, backend: AiBackend, effort: ReasoningEffort, expect_reasoning: bool) {
    let outcome = run_tool_loop(backend, effort).await;
    log::info!(
        target: "agent::llm",
        "{label}: {} tool rounds, {} reasoning ticks, {} reasoning parts replayed",
        outcome.tool_rounds,
        outcome.reasoning_ticks,
        outcome.reasoning_parts
    );
    assert!(
        outcome.tool_rounds >= 2,
        "{label}: expected 2+ tool rounds, got {}",
        outcome.tool_rounds
    );
    assert!(
        outcome.answer.contains('3'),
        "{label}: expected the answer 3, got {:?}",
        outcome.answer
    );
    if expect_reasoning {
        assert!(
            outcome.reasoning_parts > 0,
            "{label}: the loop thought ({} ticks) but persisted no reasoning to replay",
            outcome.reasoning_ticks
        );
    }
}

fn remote(env_var: &str, base_url: &str, model: &str) -> AiBackend {
    AiBackend::remote_for_tests(env_or_skip(env_var), base_url.into(), model.into())
}

#[tokio::test]
#[ignore = "real API call: set ANTHROPIC_API_KEY to run"]
async fn smoke_anthropic_tool_loop_haiku() {
    assert_tool_loop(
        "haiku",
        anthropic_backend("claude-haiku-5-5"),
        ReasoningEffort::High,
        true,
    )
    .await;
}

#[tokio::test]
#[ignore = "real API call: set ANTHROPIC_API_KEY to run"]
async fn smoke_anthropic_tool_loop_sonnet() {
    assert_tool_loop(
        "sonnet",
        anthropic_backend("claude-sonnet-5-5"),
        ReasoningEffort::High,
        true,
    )
    .await;
}

#[tokio::test]
#[ignore = "real API call: set ANTHROPIC_API_KEY to run"]
async fn smoke_anthropic_tool_loop_opus() {
    assert_tool_loop(
        "opus",
        anthropic_backend("claude-opus-5-5"),
        ReasoningEffort::High,
        true,
    )
    .await;
}

/// Opus with nothing asked: Claude 5.x thinks by default with the text omitted, so the replay
/// carries signature-only thinking blocks.
#[tokio::test]
#[ignore = "real API call: set ANTHROPIC_API_KEY to run"]
async fn smoke_anthropic_tool_loop_opus_default_effort() {
    assert_tool_loop(
        "opus default",
        anthropic_backend("claude-opus-5-5"),
        ReasoningEffort::ProviderDefault,
        false,
    )
    .await;
}

#[tokio::test]
#[ignore = "real API call: set OPENAI_API_KEY to run"]
async fn smoke_openai_responses_tool_loop() {
    let backend = remote("OPENAI_API_KEY", "https://api.openai.com/v1/", "gpt-5.5");
    assert_tool_loop("gpt-5.5", backend, ReasoningEffort::High, true).await;
}

#[tokio::test]
#[ignore = "real API call: set OPENAI_API_KEY to run"]
async fn smoke_openai_chat_reasoning_tool_loop() {
    // `o3` stays on chat-completions, where OpenAI keeps reasoning server-side: nothing
    // comes back to replay, and nothing has to.
    let backend = remote("OPENAI_API_KEY", "https://api.openai.com/v1/", "o3");
    assert_tool_loop("o3", backend, ReasoningEffort::Medium, false).await;
}

#[tokio::test]
#[ignore = "real API call: set GEMINI_API_KEY to run"]
async fn smoke_gemini_tool_loop() {
    let backend = remote(
        "GEMINI_API_KEY",
        "https://generativelanguage.googleapis.com/v1/",
        "gemini-3.6-flash",
    );
    assert_tool_loop("gemini-3.6-flash", backend, ReasoningEffort::High, true).await;
}

#[tokio::test]
#[ignore = "real API call: set MISTRAL_API_KEY to run"]
async fn smoke_mistral_tool_loop() {
    // Routes to genai's Mistral dialect by host: the reasoning goes back as a `thinking` chunk,
    // since Mistral 422s on `reasoning_content`.
    let backend = remote(
        "MISTRAL_API_KEY",
        "https://api.mistral.ai/v1",
        "magistral-medium-latest",
    );
    assert_tool_loop("magistral-medium", backend, ReasoningEffort::ProviderDefault, true).await;
}

#[tokio::test]
#[ignore = "real API call: set GROQ_API_KEY to run"]
async fn smoke_groq_tool_loop() {
    // Groq 400s an assistant turn carrying `reasoning_content`, so its reasoning stays out of
    // the replay (`ai::client::replays_chat_reasoning_for_host`): this proves the loop runs.
    let backend = remote("GROQ_API_KEY", "https://api.groq.com/openai/v1", "openai/gpt-oss-120b");
    assert_tool_loop("groq gpt-oss-120b", backend, ReasoningEffort::Medium, false).await;
}

#[tokio::test]
#[ignore = "real API call: set FIREWORKS_AI_API_KEY to run"]
async fn smoke_fireworks_deepseek_tool_loop() {
    // DeepSeek V4 on Fireworks speaks DeepSeek's own reasoning format (`reasoning_content`,
    // required back mid-loop), so it stands in for api.deepseek.com without a DeepSeek key.
    let backend = remote(
        "FIREWORKS_AI_API_KEY",
        "https://api.fireworks.ai/inference/v1",
        "accounts/fireworks/models/deepseek-v4p1-flash",
    );
    assert_tool_loop(
        "fireworks deepseek-v4p1-flash",
        backend,
        ReasoningEffort::ProviderDefault,
        true,
    )
    .await;
}

#[tokio::test]
#[ignore = "real API call: set OPENROUTER_API_KEY to run"]
async fn smoke_openrouter_deepseek_tool_loop() {
    let backend = remote(
        "OPENROUTER_API_KEY",
        "https://openrouter.ai/api/v1",
        "deepseek/deepseek-v4.1-flash",
    );
    assert_tool_loop("openrouter deepseek", backend, ReasoningEffort::Medium, false).await;
}

#[tokio::test]
#[ignore = "real API call: set OPENROUTER_API_KEY to run"]
async fn smoke_openrouter_claude_tool_loop() {
    let backend = remote(
        "OPENROUTER_API_KEY",
        "https://openrouter.ai/api/v1",
        "anthropic/claude-sonnet-5.5",
    );
    assert_tool_loop("openrouter claude", backend, ReasoningEffort::Medium, false).await;
}

#[tokio::test]
#[ignore = "real API call: set OPENROUTER_API_KEY to run"]
async fn smoke_openrouter_grok_tool_loop() {
    let backend = remote("OPENROUTER_API_KEY", "https://openrouter.ai/api/v1", "x-ai/grok-4.7");
    assert_tool_loop("openrouter grok", backend, ReasoningEffort::Medium, false).await;
}

/// A system prompt past every provider's cache minimum (512 tokens on Claude 5.x, 1,024 on
/// OpenAI), so the prefix is cacheable at all. Ask Cmdr's own prompt is far past it.
fn cache_padded_system() -> String {
    let rule = "Name folders exactly as the tool lists them, never from memory or a guess. ";
    format!(
        "You are a file assistant. Use the tools; never guess folder contents. {}",
        rule.repeat(120)
    )
}

/// #770's "done when": a multi-step turn reports cache reads from the second call on, and the
/// meter prices them below the uncached rate. Runs the tool loop with a cacheable prefix, logs
/// each call's usage and priced cost (`RUST_LOG=info`, `--no-capture` to see them), and asserts
/// every call after the first read from the cache.
async fn assert_cache_reads(label: &str, backend: AiBackend, provider: ProviderTag, model: &str) {
    let _ = env_logger::builder().is_test(true).try_init();
    let outcome = run_tool_loop_with(backend, ReasoningEffort::ProviderDefault, &cache_padded_system()).await;
    assert!(outcome.calls.len() >= 2, "{label}: expected 2+ calls");
    for (index, usage) in outcome.calls.iter().enumerate() {
        let priced = crate::agent::pricing::price_call(provider, model, *usage);
        let uncached = crate::agent::pricing::price_call(
            provider,
            model,
            AgentUsage {
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                ..*usage
            },
        );
        log::info!(
            target: "agent::llm",
            "{label} call {}: prompt {} (cache read {}, write {}), completion {}, cost {} micro-USD (uncached {}), priced={}",
            index + 1,
            usage.prompt_tokens,
            usage.cache_read_tokens,
            usage.cache_write_tokens,
            usage.completion_tokens,
            priced.cost_micros,
            uncached.cost_micros,
            priced.priced
        );
        assert!(priced.priced, "{label}: {model} must be in the price table");
        if index == 0 {
            continue;
        }
        assert!(
            usage.cache_read_tokens > 0,
            "{label}: call {} read nothing from the cache: {usage:?}",
            index + 1
        );
        assert!(
            priced.cost_micros < uncached.cost_micros,
            "{label}: call {} priced {}, not under the uncached {}",
            index + 1,
            priced.cost_micros,
            uncached.cost_micros
        );
    }
}

#[tokio::test]
#[ignore = "real API call: set ANTHROPIC_API_KEY to run"]
async fn smoke_anthropic_cache_reads() {
    let model = "claude-sonnet-5-5";
    assert_cache_reads("claude cache", anthropic_backend(model), ProviderTag::Anthropic, model).await;
}

#[tokio::test]
#[ignore = "real API call: set OPENAI_API_KEY to run"]
async fn smoke_openai_explicit_cache_reads() {
    // GPT-5.6 and later cache only where a request marks a breakpoint (genai's explicit mode).
    let model = "gpt-6.1-terra";
    let backend = remote("OPENAI_API_KEY", "https://api.openai.com/v1/", model);
    assert_cache_reads("gpt cache", backend, ProviderTag::OpenAiResponses, model).await;
}

#[tokio::test]
#[ignore = "real API call: set OPENROUTER_API_KEY to run"]
async fn smoke_openrouter_claude_cache_reads() {
    let model = "anthropic/claude-sonnet-5.5";
    let backend = remote("OPENROUTER_API_KEY", "https://openrouter.ai/api/v1", model);
    assert_cache_reads("openrouter claude cache", backend, ProviderTag::OpenAi, model).await;
}

#[tokio::test]
#[ignore = "real API call: set OPENAI_API_KEY to run"]
async fn smoke_openai() {
    let key = env_or_skip("OPENAI_API_KEY");
    let backend = AiBackend::remote_for_tests(key, "https://api.openai.com/v1/".into(), "gpt-4o-mini".into());
    let answer = run_one(backend, "Say hello.").await;
    assert!(!answer.trim().is_empty(), "expected a visible answer, got {answer:?}");
}

#[tokio::test]
#[ignore = "real API call: set GEMINI_API_KEY to run"]
async fn smoke_gemini() {
    let key = env_or_skip("GEMINI_API_KEY");
    // The app's Gemini preset: the native `v1/` API (a version-less base 404s) and its default
    // model (`gemini-2.5-flash` is closed to new users, 2026-10-26).
    let backend = AiBackend::remote_for_tests(
        key,
        "https://generativelanguage.googleapis.com/v1/".into(),
        "gemini-3.8-flash".into(),
    );
    let answer = run_one(backend, "Say hello.").await;
    assert!(!answer.trim().is_empty(), "expected a visible answer, got {answer:?}");
}

#[tokio::test]
#[ignore = "local server: set LLAMA_PORT to a running Cmdr llama-server to run"]
async fn smoke_local() {
    let port: u16 = env_or_skip("LLAMA_PORT")
        .parse()
        .expect("LLAMA_PORT must be a port number");
    let backend = AiBackend::local_for_tests(port);
    let answer = run_one(backend, "Say hello.").await;
    assert!(!answer.trim().is_empty(), "expected a visible answer, got {answer:?}");
}

// ── Lazy tool loading: a deferred tool, on the real prefix ───────────────────────

/// What a model did when asked for something only a deferred tool can do.
struct DeferredRun {
    /// Each call as the provider sent it, and the tool that ran.
    calls: Vec<(ToolId, ToolId, serde_json::Value)>,
    /// Calls that came back with a problem, so a fumble shows even when the model recovers.
    problems: Vec<serde_json::Value>,
    answer: String,
}

/// The answer a tool-search call gets for real, through the pair's own handlers and the schema gate, and
/// a canned yes for the tool it reaches. Nothing touches an app, so nothing here moves a pane.
async fn answer_deferred_call(effective: &super::types::AgentToolCall) -> serde_json::Value {
    let refused = |error: crate::tools::ToolError| serde_json::json!({ "problem": error.message, "data": error.data });
    let name = effective.tool.as_wire_name();
    if crate::tools::tool_access(name).is_none() {
        return serde_json::json!({ "available": false, "requested": name });
    }
    if let Err(problem) = crate::tools::validate_params(name, &effective.arguments) {
        let mut answer = refused(problem);
        if let Some(schema) = crate::tools::meta::schema_for_a_refusal(name) {
            answer["tool"] = schema;
        }
        return answer;
    }
    match &effective.tool {
        ToolId::ToolDetails => crate::tools::meta::execute_tool_details(&effective.arguments)
            .await
            .unwrap_or_else(refused),
        ToolId::UseTool => crate::tools::meta::execute_use_tool(&effective.arguments)
            .await
            .unwrap_or_else(refused),
        // A model that looks before it acts gets something to look at, or it asks again until the
        // round cap.
        ToolId::ListPaneFiles => serde_json::json!({
            "pane": "left",
            "path": "/Users/test/work",
            "scope": "folder",
            "returned": 2,
            "total": 2,
            "truncated": false,
            "entries": [
                { "name": "big.iso", "size": 4_000_000_000_u64, "modified": 1_785_247_668 },
                { "name": "notes.txt", "size": 2_000, "modified": 1_785_247_668 },
            ],
        }),
        ToolId::AppState => serde_json::json!({
            "focusedPane": "left",
            "left": { "path": "/Users/test/work", "sort": { "by": "name", "order": "asc" } },
            "right": { "path": "/Users/test", "sort": { "by": "name", "order": "asc" } },
        }),
        // The handler's own refusal: the schema gate reads keys, never a value.
        ToolId::Tab if effective.arguments["action"] != "new" => {
            let mut answer = serde_json::json!({ "problem": "Unknown tab action. It takes new, close, close_others, activate, set_pinned, reopen, and move." });
            answer["tool"] = crate::tools::meta::schema_for_a_refusal(name).unwrap_or_default();
            answer
        }
        ToolId::Dialog if effective.arguments["type"] == "tab" => serde_json::json!({
            "problem": "tab isn't a dialog. It opens settings, file-viewer, or about. Call the tab tool instead, through use_tool if it isn't declared to you.",
        }),
        ToolId::ReadResource => serde_json::json!({
            "uri": effective.arguments["uri"],
            "text": "09:20:01 WARN volume: Couldn't reach the share, retrying\n09:21:44 INFO fileExplorer: Loaded 12 files",
        }),
        _ => serde_json::json!({ "ok": true }),
    }
}

/// One turn on Ask Cmdr's real prefix (the shipped system prompt, the core plus the pair), replaying
/// each message verbatim the way `run_turn` does, so a provider that rejects a stored `use_tool` or the
/// pair's schemas fails here.
async fn run_deferred_tool_turn(backend: AiBackend, request: &str) -> DeferredRun {
    let llm = GenaiAgentLlm::new(backend);
    let system = crate::agent::chat::system_prompt::system_prompt();
    let tools = crate::tools::agent_tool_declarations();
    // The shape a rail turn opens with: the app's facts, then what the person typed.
    let mut messages = vec![AgentMessage {
        role: AgentRole::User,
        parts: vec![
            AgentPart::Envelope(
                "[Mon 2026-11-02 09:24 · focused: /Users/test/work · cursor: .. · 0 selected · rename batch: up to 101 files]"
                    .to_string(),
            ),
            AgentPart::Text(request.to_string()),
        ],
        at: 0,
    }];
    let mut run = DeferredRun {
        calls: Vec::new(),
        problems: Vec::new(),
        answer: String::new(),
    };
    for round in 0..6 {
        let stream = llm
            .respond(
                &system,
                &tools,
                &messages,
                crate::agent::llm::CallSettings::default(),
                CancellationToken::new(),
            )
            .await
            .unwrap_or_else(|error| panic!("respond failed on round {round}: {error:?}"));
        let deltas: Vec<AgentDelta> = stream
            .map(|item| item.unwrap_or_else(|error| panic!("stream error on round {round}: {error:?}")))
            .collect()
            .await;
        let Some(AgentDelta::End { message, .. }) = deltas.last() else {
            panic!("stream did not end with an End delta");
        };
        let mut results = Vec::new();
        for part in &message.parts {
            let AgentPart::ToolCall(call) = part else { continue };
            let effective = call.effective();
            let content = answer_deferred_call(&effective).await;
            if content.get("problem").is_some() || content.get("available").is_some() {
                run.problems.push(content.clone());
            }
            run.calls
                .push((call.tool.clone(), effective.tool.clone(), effective.arguments.clone()));
            results.push(AgentPart::ToolResult(AgentToolResult {
                call_id: call.call_id.clone(),
                content,
                elided: false,
            }));
        }
        messages.push(message.clone());
        if results.is_empty() {
            run.answer = message
                .parts
                .iter()
                .filter_map(|part| match part {
                    AgentPart::Text(text) => Some(text.as_str()),
                    _ => None,
                })
                .collect();
            return run;
        }
        messages.push(AgentMessage {
            role: AgentRole::Tool,
            parts: results,
            at: 0,
        });
    }
    run
}

/// What each model did goes in the log, fumbles included (`RUST_LOG=info`, `--no-capture`): a weak model
/// may fumble the indirection, and a green assert can't say how. It has to reach the tool with arguments
/// its schema takes, by whichever road. `sort` is named in `use_tool`'s description, and `toggle_hidden`
/// only in the system prompt's list.
async fn assert_reaches_a_deferred_tool(label: &str, backend: AiBackend) {
    assert_reaches(
        label,
        backend,
        "Sort the left pane by size, largest first. I've given you everything you need.",
        ToolId::Sort,
    )
    .await;
}

async fn assert_reaches_a_tool_only_the_list_names(label: &str, backend: AiBackend) {
    assert_reaches(label, backend, "Turn on showing hidden files.", ToolId::ToggleHidden).await;
}

async fn assert_reaches(label: &str, backend: AiBackend, request: &str, wanted: ToolId) {
    let _ = env_logger::builder().is_test(true).try_init();
    let run = run_deferred_tool_turn(backend, request).await;
    for (sent, ran, arguments) in &run.calls {
        log::info!(
            target: "agent::llm",
            "{label}: sent {}, ran {}, arguments {arguments}",
            sent.as_wire_name(),
            ran.as_wire_name()
        );
    }
    log::info!(target: "agent::llm", "{label}: {} problem(s) on the way: {:?}", run.problems.len(), run.problems);
    log::info!(target: "agent::llm", "{label} answered: {}", run.answer);

    // The gate refuses a guess at the arguments, so one that fits may take a second try.
    let name = wanted.as_wire_name();
    let reached = run
        .calls
        .iter()
        .find(|(_, ran, arguments)| *ran == wanted && crate::tools::validate_params(name, arguments).is_ok());
    let Some((sent, _, _)) = reached else {
        panic!(
            "{label}: never reached {name} with arguments its schema takes: {:?}",
            run.calls
        );
    };
    log::info!(
        target: "agent::llm",
        "{label}: reached {name} {}",
        if *sent == ToolId::UseTool { "through use_tool" } else { "directly" }
    );
}

#[tokio::test]
#[ignore = "real API call: set ANTHROPIC_API_KEY to run"]
async fn smoke_anthropic_deferred_tool() {
    assert_reaches_a_deferred_tool("haiku", anthropic_backend("claude-haiku-5-5")).await;
}

#[tokio::test]
#[ignore = "real API call: set OPENAI_API_KEY to run"]
async fn smoke_openai_deferred_tool() {
    let backend = remote("OPENAI_API_KEY", "https://api.openai.com/v1/", "gpt-4.1-mini");
    assert_reaches_a_deferred_tool("gpt-4.1-mini", backend).await;
}

#[tokio::test]
#[ignore = "real API call: set OPENAI_API_KEY to run"]
async fn smoke_openai_list_only_tool() {
    let backend = remote("OPENAI_API_KEY", "https://api.openai.com/v1/", "gpt-4.1-mini");
    assert_reaches_a_tool_only_the_list_names("gpt-4.1-mini", backend).await;
}

#[tokio::test]
#[ignore = "real API call: set OPENAI_API_KEY to run"]
async fn smoke_openai_responses_deferred_tool() {
    let backend = remote("OPENAI_API_KEY", "https://api.openai.com/v1/", "gpt-5.5");
    assert_reaches_a_deferred_tool("gpt-5.5", backend).await;
}

#[tokio::test]
#[ignore = "real API call: set GEMINI_API_KEY to run"]
async fn smoke_gemini_deferred_tool() {
    let backend = remote(
        "GEMINI_API_KEY",
        "https://generativelanguage.googleapis.com/v1/",
        "gemini-3.6-flash",
    );
    assert_reaches_a_deferred_tool("gemini-3.6-flash", backend).await;
}

/// The Gemini preset's default model (`cloud-providers.ts`), so the out-of-the-box choice is the one proven.
#[tokio::test]
#[ignore = "real API call: set GEMINI_API_KEY to run"]
async fn smoke_gemini_default_deferred_tool() {
    let backend = remote(
        "GEMINI_API_KEY",
        "https://generativelanguage.googleapis.com/v1/",
        "gemini-3.8-flash",
    );
    assert_reaches_a_deferred_tool("gemini-3.8-flash", backend).await;
}

#[tokio::test]
#[ignore = "real API call: set GROQ_API_KEY to run"]
async fn smoke_groq_deferred_tool() {
    let backend = remote("GROQ_API_KEY", "https://api.groq.com/openai/v1", "openai/gpt-oss-120b");
    assert_reaches_a_deferred_tool("groq gpt-oss-120b", backend).await;
}

/// The preset's default model unless `MISTRAL_MODEL` names another: a free Mistral workspace gets
/// no requests at all on `mistral-small-*` (`x-ratelimit-limit-req-minute: 0`) but does on the
/// Ministral models, so `MISTRAL_MODEL=ministral-14b-latest` proves the host on a free key.
#[tokio::test]
#[ignore = "real API call: set MISTRAL_API_KEY to run"]
async fn smoke_mistral_deferred_tool() {
    let model = std::env::var("MISTRAL_MODEL").unwrap_or_else(|_| "mistral-small-latest".to_string());
    let backend = remote("MISTRAL_API_KEY", "https://api.mistral.ai/v1", &model);
    assert_reaches_a_deferred_tool(&model, backend).await;
}

#[tokio::test]
#[ignore = "local server: set LLAMA_PORT to a running OpenAI-compatible local server to run"]
async fn smoke_local_deferred_tool() {
    let port: u16 = env_or_skip("LLAMA_PORT")
        .parse()
        .expect("LLAMA_PORT must be a port number");
    assert_reaches_a_deferred_tool("local", AiBackend::local_for_tests(port)).await;
}

#[tokio::test]
#[ignore = "local server: set LLAMA_PORT to a running OpenAI-compatible local server to run"]
async fn smoke_local_list_only_tool() {
    let port: u16 = env_or_skip("LLAMA_PORT")
        .parse()
        .expect("LLAMA_PORT must be a port number");
    assert_reaches_a_tool_only_the_list_names("local", AiBackend::local_for_tests(port)).await;
}

/// A plain request a person makes, and what doing it takes.
struct PlainRequest {
    request: &'static str,
    wanted: ToolId,
    done: fn(&serde_json::Value) -> bool,
}

fn opens_a_new_tab(arguments: &serde_json::Value) -> bool {
    arguments["action"] == "new" && arguments["pane"] == "left"
}

fn reads_the_logs(arguments: &serde_json::Value) -> bool {
    arguments["uri"]
        .as_str()
        .is_some_and(|uri| uri.starts_with("cmdr://logs"))
}

/// The requests a person types without knowing Cmdr's tool names, which `gpt-4.1-mini` and Ministral 3B
/// fumbled in the running app. Words don't count here: each run reached the tool with what does the
/// job, or it didn't. `PLAIN_RUNS` sets the runs per request (default three), and the tally lands in the
/// log (`tools/DETAILS.md` § Lazy loading has the last one).
async fn tally_plain_requests(label: &str, backend: impl Fn() -> AiBackend) {
    let _ = env_logger::builder().is_test(true).try_init();
    let requests = [
        PlainRequest {
            request: "Open a new tab in the left pane.",
            wanted: ToolId::Tab,
            done: opens_a_new_tab,
        },
        PlainRequest {
            request: "Check Cmdr's logs: is there any warning from the last few minutes?",
            wanted: ToolId::ReadResource,
            done: reads_the_logs,
        },
        PlainRequest {
            request: "check the logs",
            wanted: ToolId::ReadResource,
            done: reads_the_logs,
        },
    ];
    let runs: usize = std::env::var("PLAIN_RUNS")
        .ok()
        .and_then(|runs| runs.parse().ok())
        .unwrap_or(3);
    let mut empty_handed = Vec::new();
    for PlainRequest { request, wanted, done } in requests {
        let mut reached = 0;
        for attempt in 0..runs {
            let run = run_deferred_tool_turn(backend(), request).await;
            let road: Vec<String> = run
                .calls
                .iter()
                .map(|(sent, ran, arguments)| format!("{}>{} {arguments}", sent.as_wire_name(), ran.as_wire_name()))
                .collect();
            log::info!(target: "agent::llm", "{label} #{attempt} {request:?}: {road:?}");
            log::info!(target: "agent::llm", "{label} #{attempt} answered: {}", run.answer);
            if run
                .calls
                .iter()
                .any(|(_, ran, arguments)| *ran == wanted && done(arguments))
            {
                reached += 1;
            }
        }
        log::info!(target: "agent::llm", "{label} TALLY {request:?}: {reached} of {runs}");
        if reached == 0 {
            empty_handed.push(request);
        }
    }
    assert!(
        empty_handed.is_empty(),
        "{label} never got these done: {empty_handed:?}"
    );
}

#[tokio::test]
#[ignore = "real API call: set OPENAI_API_KEY to run"]
async fn smoke_openai_plain_requests() {
    tally_plain_requests("gpt-4.1-mini", || {
        remote("OPENAI_API_KEY", "https://api.openai.com/v1/", "gpt-4.1-mini")
    })
    .await;
}

#[tokio::test]
#[ignore = "local server: set LLAMA_PORT to a running OpenAI-compatible local server to run"]
async fn smoke_local_plain_requests() {
    let port: u16 = env_or_skip("LLAMA_PORT")
        .parse()
        .expect("LLAMA_PORT must be a port number");
    tally_plain_requests("local", || AiBackend::local_for_tests(port)).await;
}
