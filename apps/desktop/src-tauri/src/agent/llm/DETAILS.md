# Agent LLM seam details

Pull-tier docs for `agent/llm/`. Must-know invariants live in `CLAUDE.md`.

## The seam

`AgentLlm::respond(system, tools, messages, cancel)` makes ONE cold, self-contained streaming call. `messages` is the
fully-assembled prompt — context assembly (prefix stability, elision, the envelope) is the runtime's job, not the
LLM's. The call returns an `AgentDeltaStream` (`BoxStream<'static, Result<AgentDelta, AgentLlmError>>`); the terminal
`AgentDelta::End` carries the fully-assembled final `AgentMessage`, including any opaque provider state, for persistence
and replay.

The trait is written as a boxed-future return (`BoxFuture`) rather than `async fn` so it stays object-safe
(`Box<dyn AgentLlm>`) without pulling in `async-trait`.

Two implementations:
- `GenaiAgentLlm` (`genai_impl.rs`) wraps a configured `crate::ai::AiBackend` (the interactive model slot; the slot is
  resolved in the command layer). It reuses the backend's adapter routing (`remote_model_iden` — the load-bearing Ollama-fallback
  fix) and its stream-cancel model, via `pub(crate)` seam methods on `AiBackend`: `resolve_model`, `replays_chat_reasoning`, and
  `exec_chat_stream_request` (the prompt-only helpers can't express a multipart tool loop).
- `FakeAgentLlm` (`fake.rs`) is scripted with `ScriptedTurn`s (one consumed per `respond`, or picked per call from the
  prompt with `by_prompt`, which `latest_user_turn` serves) and records every `messages` slice via `calls_seen()`, so a
  test asserts the exact assembled prompts. `CallRawTool(name, args)` emits a raw tool name to exercise the read-only
  parse gate. `SayThenCall` narrates, then calls: its text and call starts interleave on the wire while its `End`
  message holds one text, then every call (genai's `StreamEnd` order). Each turn's `End` reports zero usage, or what `script_with_usage` gives it. Call ids are
  `fake-call-{index}` PER ROUND, so they collide across rounds on purpose: the tool-line tests rely on it (❌ don't
  make them unique).

## `AgentPart` ⇄ genai `ContentPart` mapping

`genai_impl.rs` owns the only translation between the typed part model and genai's wire model. The reasoning `blob` is
opaque everywhere else; its whole vocabulary is owned here. Each blob holds ONE genai reasoning part, in the position it
came: `thought_signatures` (an array), `reasoning_content` (a string), or `custom` (a provider block genai keeps whole),
plus an `origin` stamp `{adapter, model}` (`ReasoningOrigin`). Blobs stored before the stamp existed have no `origin`.

Agent → genai (request / replay), per `agent_part_to_genai`:

- `Text(s)` and `Envelope(s)` → `ContentPart::Text(s)` (the envelope is a user turn's rendered context, replayed verbatim; `chat/DETAILS.md` § Anatomy of one call).
- `MemoryNotes(notes)` → `ContentPart::Text(notes.render())`, the fenced memory block (`chat/DETAILS.md` § The memory block).
- `ToolCall{call_id, tool, arguments, reasoning}` → `ContentPart::ToolCall{call_id, fn_name = tool.as_wire_name(),
  fn_arguments = arguments, thought_signatures = blob.thought_signatures}` (the signatures only when the reasoning may
  replay, below), except that a nameless call
  (`ToolId::is_nameless`) maps to nothing, and `fn_arguments` that isn't a JSON object goes out as `{}`. See
  "Degenerate tool calls" below.
- `ToolResult{call_id, content}` → `ContentPart::ToolResponse{call_id, fn_name, content = JSON string}` (a bare string
  passes through unquoted so it round-trips; a structured value serializes to JSON text). `AgentToolResult` carries no
  tool name of its own, so `build_request` first indexes the whole assembled transcript by `call_id`
  (`tool_names_by_call_id`) and looks `fn_name` up from the originating `ToolCall`. Gemini's `functionResponse.name`
  keys on the function name rather than the call ID, so an unnamed result degrades tool calling there; every other
  adapter correlates by call ID and ignores the field. A result whose call already fell out of the assembled window
  resolves to `None` (unnamed beats wrongly named).
- `Reasoning(state)` → the genai part its blob holds (`ThoughtSignature`, `ReasoningContent`, or `Custom`), when
  `ReasoningOrigin::may_replay` allows it, else nothing. It allows a stamped blob only to the exact `{adapter, model}`
  that produced it, an unstamped one only within its `ProviderTag` family, and nothing at all to a chat-completions
  endpoint that doesn't take reasoning back (§ Reasoning round-trip).

genai → agent (parse), per `genai_content_to_agent_parts(content, origin)`, stamping every blob with `origin`:

- `Text` → `Text`; `ToolResponse` → `ToolResult`.
- `ThoughtSignature`, `ReasoningContent`, `Custom` → one `Reasoning` part each, in wire order. Nothing is deduped: genai
  leads a captured answer with its reasoning parts AND mirrors their signatures onto the first tool call, and each
  adapter picks its canonical copy on replay (Anthropic and OpenAI Responses read the leading parts and ignore the
  mirror; Gemini reads each call's own signature and skips a leading one that repeats it). Replaying exactly what came
  back is right for all three; any dedupe here would have to know which copy each adapter reads.
- `ToolCall` → `AgentToolCall`, with `tool = ToolId::from_wire_name(fn_name)` and its own `thought_signatures` kept as
  its reasoning. A call with a blank `fn_name` is dropped instead.
- `Binary` → dropped (the agent's read-only toolset never produces it).
- After the parts, `build_agent_message_from_stream_end` adds the stream's `captured_reasoning_content` as a LEADING
  `Reasoning{reasoning_content}` part, for a chat-completions endpoint that takes it back, when the content holds no
  `ReasoningContent` already: genai's chat-completions streamer keeps reasoning text out of `captured_content`.

Stream events → `AgentDelta` (`map_stream_event`): `Chunk` → `Text` (empty chunks skipped); `ReasoningChunk` /
`ThoughtSignatureChunk` → `ReasoningTick` (content never surfaced); `ToolCallChunk` → `ToolCallStarted`, **once per
call id** (`map_stream`); `Start` / `Heartbeat` → `KeepAlive` (shown as nothing; it resets the runtime's stall check,
`chat/DETAILS.md` § Constants table); `End` → `End{stop, usage, message}` built from the captured content, stop reason,
and usage; a stream error → mapped `AgentLlmError`.

Gotcha: genai's OpenAI streamer (verified on genai 0.6.5, reading its OpenAI adapter's streamer source, 2026-10-22) emits a
`ToolCallChunk` for EVERY argument fragment, each carrying the merged call's id. Passed through raw, a call whose
arguments stream in several fragments started several times, the rail keyed two tool lines on one id, and Svelte's
`each_key_duplicate` took the whole window down (`ERR-FRMAR`, local Ministral 3B on llama-server). The rail's
`applyToolStarted` is idempotent per call id too, as a second guard.

## Prompt caching

A caching provider bills a repeated prompt prefix at a fraction of the price, so every request asks for caching and
`chat/` keeps the bytes stable (`chat/DETAILS.md` § Budget enforcement, § The memory block). Providers want the ask in
different shapes, and the wrong one isn't a no-op, so `genai_impl/cache.rs` picks ONE `CachePlan` per request:

- **`Breakpoints` (the default): the last message carries `CacheControl::Ephemeral`** (5-minute TTL). That's the
  rolling breakpoint: each call caches the whole prompt, and the next one (the same bytes plus one round) reads it back,
  which Anthropic finds through its block lookback. A 1-hour TTL costs twice the write, so the 5-minute default stays
  until a thread's pauses are measured. **On Anthropic the system string also goes out as a marked system-role
  message** (genai lifts it into the request's `system` field; its own `system` slot can't take a marker). Memory never
  sits there, so this caches the tools plus the system prompt across every thread. Two of Anthropic's four
  breakpoints. Everyone else on this plan ignores the marks: OpenAI before GPT-5.6 caches automatically (genai drops a
  message mark there), Gemini caches implicitly, DeepSeek automatically.
- **`Automatic`: OpenAI's own GPT-5.6 and later get no marks**, and the intent rides the request level
  (`with_cache_control`), which makes genai send nothing, so OpenAI's automatic caching covers the growing prefix,
  tool results included. ❌ Don't mark messages there: any mark switches genai 0.7.1 to explicit mode
  (`prompt_cache_options: {mode: "explicit"}`, which turns OpenAI's own breakpoints off), and a breakpoint can sit only
  on a text block, so once the last message is a tool result the call caches nothing. Live on `gpt-6.1-terra`
  (2026-10-29), call 2 of a tool loop read 0 tokens and wrote 0. Which models: `gpt-N.M…` from 5.6 on without a maker
  prefix, mirroring genai's private `OpenAIModel::requires_explicit_cache`.
- **`TopLevel`: OpenRouter routing to Claude gets one top-level `cache_control: {type: "ephemeral"}`** (genai's
  `extra_body`). That route goes through genai's OpenAI adapter, which drops message marks, and Claude caches nothing
  unmarked, so every round re-billed the whole chat (#801). The top-level field is OpenRouter's automatic mode for
  Claude: the breakpoint lands on the last cacheable block and moves forward as the thread grows (OpenRouter's
  prompt-caching guide, "Anthropic Claude", checked 2026-10-29). Who gets it is `ai::client::routing::
  wants_top_level_cache_marker` (host `openrouter.ai`, model `anthropic/…` or `~anthropic/…`), carried as
  `AiBackend::wants_top_level_cache_marker`. Other OpenRouter models cache on their own (OpenAI, DeepSeek, Grok, Gemini
  2.5 implicit) or take per-block marks only, and an unknown top-level field risks a 400 elsewhere, so it stays off.

What the cache did comes back in `AgentUsage`: `map_usage` reads genai's `prompt_tokens_details` (`cached_tokens` →
`cache_read_tokens`, `cache_creation_tokens` → `cache_write_tokens`; every adapter normalizes to those, OpenRouter's
`cache_write_tokens` included), both shares of `prompt_tokens`, which stays the whole input. `agent::pricing` prices each
share at its own rate and the meter stores both (`../chat/DETAILS.md` § Crash / persistence model, case (d)).

Live check: `live_smoke_test.rs`'s `smoke_*_cache_reads` run the tool loop with a cacheable prefix and assert every call
after the first reads from the cache and prices under its uncached cost (`RUST_LOG=info` and `--no-capture` print each
call's split). Verified 2026-10-29 with the real keys:

- **`claude-sonnet-5-5`** (`Breakpoints`): call 1 wrote 3,345 tokens; calls 2 and 3 read 3,345 and 3,493 back and wrote
  only the new round (148, 162). Metered $0.0015 and $0.0014 against $0.0078 and $0.0080 uncached.
- **`gpt-6.1-terra`** (`Automatic`): calls 2 and 3 read 2,032 and 2,116 and wrote 84 and 105 (call 1 read too: the
  same prefix from a run minutes earlier, inside the 30-minute TTL). Metered $0.0004 and $0.0003 against $0.0023 and
  $0.0022.
- **OpenRouter to Claude** (`TopLevel`): not reached, the key has no credits (HTTP 402). The request body is pinned by
  the fixture test; `smoke_openrouter_claude_cache_reads` is the live check once the account has credits.

Tests: `genai_impl/cache.rs` (which plan each model gets), `genai_impl/tests.rs`
(`the_automatic_plan_marks_no_message`), `replay_fixtures.rs`
(`anthropic_marks_the_system_string_and_the_last_message_for_caching`, `openrouter_claude_asks_for_caching_at_the_top_level`,
`other_chat_completions_endpoints_get_no_top_level_marker`), `ai/client_unit_test.rs`
(`only_openrouter_claude_asks_for_caching_at_the_top_level`).

## Degenerate tool calls

A provider can end a stream with a tool-call stop reason and still hand back a call carrying an id and nothing else.
Observed from Qwen (`qwen3.7-plus` via DashScope's OpenAI-compatible endpoint, 2026-09-03, captured in the app's
`llm-logs/`): `call_id` set, `fn_name` empty, `fn_arguments` the empty string, `finish_reason: tool_calls`. Replaying
that identical request nine times produced a well-formed stream every time, so it's an intermittent provider emission
rather than a genai assembly bug; genai 0.6.5's `capture_tool_call` merges the normal DashScope chunk shape correctly.

Left alone it wedges a thread permanently. `ToolId::from_wire_name("")` gives `Unrecognized("")`, which dispatch
refuses (correctly), and the next turn replays the assistant part verbatim as `"arguments": ""`. DashScope answers that
with a 400 (`InternalError.Algo.InvalidParameter: The "function.arguments" parameter of the code model must be in JSON
format`), and since the broken part is persisted, every later message in that thread re-sends it and 400s again.

Three guards, and all three earn their place:

- **Inbound** (`genai_content_to_agent_parts`): a blank `fn_name` never becomes an `AgentPart`, so it never reaches the
  store. This is the one that prevents the problem.
- **Outbound** (`agent_part_to_genai` + `build_request`): a nameless call already in a persisted transcript is dropped
  from the replay, along with the tool result answering it (`answers_a_nameless_call`), because an orphan tool response
  is rejected in its own right; a message left with no parts is then filtered out of the request. This is the one that
  unwedges threads persisted before the inbound guard existed.
- **Arguments** (`object_arguments`): `fn_arguments` always goes out as a JSON object. It covers the near-miss where the
  name survives but the argument chunks don't; an empty object reaches the tool's own validation, which the model can
  recover from, instead of 400ing the whole request.

Downstream, `runtime/turn.rs` treats an assistant message with no parts as `UnfinishedReply` rather than an answer, so
what's left after the inbound drop fails honestly and persists nothing instead of showing a blank bubble.

## Reasoning round-trip

Reasoning is on for every provider. A multi-step tool loop only stays coherent (and, on some providers, only stays
legal) when each call carries the reasoning the model produced earlier in the turn back in the exact shape the
provider issued it. `build_options` sets `capture_reasoning_content`, which genai also reads as the opt-in for
Anthropic's thinking signatures and the OpenAI Responses `reasoning.encrypted_content` include, so it's load-bearing,
not cosmetic. What each provider needs (verified on genai 0.7.1, reading its adapter sources plus the live and fixture
runs below, 2026-10-27):

- **Anthropic** (native Messages API): every `thinking` block (its `thinking` text plus `signature`) and every
  `redacted_thinking` block (its `data`) of the assistant turn, in order, ahead of the text and `tool_use`. Claude 5.x
  thinks adaptively by default with the text omitted, so a block comes back as an empty `thinking` plus a signature,
  and goes back the same way. genai captures both kinds from the stream (`ThoughtSignature` + `ReasoningContent`
  pairs, `redacted_thinking` as a `Custom` part) and rebuilds them on replay. Anthropic
  accepts a turn with its thinking dropped, so a model switch losing it is safe. Upstream 0.7 does all this unpatched,
  so the Anthropic thinking-capture patch once planned for genai isn't needed (verified on genai 0.7.1,
  `anthropic_replays_signed_omitted_and_redacted_thinking_in_order` plus a live `claude-sonnet-5-5` tool loop at High,
  2026-10-29).
- **OpenAI Responses** (`gpt-5*`, `*-codex`, `*-pro`): each `reasoning` item's `encrypted_content`, sent back as a
  top-level `{type: "reasoning", encrypted_content}` input item ahead of the `function_call`. Requests run `store:
  false`, so the encrypted copy is the only one (`include: ["reasoning.encrypted_content"]`). genai captures it as
  a `ThoughtSignature` and sends it back as that input item.
- **OpenAI chat-completions** (`o3`, `gpt-4.1`, Azure OpenAI): stateless. OpenAI keeps the reasoning server-side and
  returns none, so nothing round-trips and nothing has to.
- **Gemini** (native API): the `thoughtSignature` on each `functionCall` part, back on that same part (Gemini 3
  validates the first call's signature in the current turn), and one on a text part back on that text. genai 0.7 keeps
  each call's own signature and skips the leading duplicate on replay.
- **DeepSeek** (V4, `api.deepseek.com`): thinking is on by default (`high`), the text comes back as
  `reasoning_content` beside `content`, and with `tools` in the request, EVERY earlier assistant turn's
  `reasoning_content` must come back, or the API 400s (verified on DeepSeek's thinking-mode guide,
  api-docs.deepseek.com/guides/thinking_mode, 2026-10-27). genai's chat-completions streamer keeps that text out of
  the captured content, so `build_agent_message_from_stream_end` rejoins it. The host routes to genai's DeepSeek
  dialect (`ai::client::routing::chat_dialect_for_host`), which also sends `thinking: {type}` for the effort. Not yet run
  live (no DeepSeek key); Fireworks' DeepSeek V4.1 below speaks the same format.
- **Mistral** (`magistral-*`): reasoning streams as `thinking` content chunks and must go back as a leading `thinking`
  chunk; a `reasoning_content` field 422s (extra fields forbidden). The host routes to genai's Mistral dialect, which
  does both. Not yet run live (the key was rate-limited on every call, 2026-10-27): `smoke_mistral_tool_loop`.
- **Groq**: returns reasoning in `reasoning`, and 400s an assistant turn carrying `reasoning_content` ("property
  'reasoning_content' is unsupported", verified on `openai/gpt-oss-120b`, live tool loop, 2026-10-27). Reasoning stays
  out of the replay.
- **Fireworks**: takes `reasoning_content` back (verified on `deepseek-v4p1-flash`, live tool loop, 2026-10-27), so
  DeepSeek and Kimi there keep their continuity.
- **OpenRouter**: continuity rides `reasoning_details` (signed, encrypted, and summary blocks), wanted back verbatim
  and in order; `reasoning_content` is accepted beside them (OpenRouter's reasoning-tokens guide, "Preserving
  Reasoning", checked 2026-10-27). Upstream genai 0.7.1 captures the blocks on the non-streaming path only, so Cmdr
  vendors a patch that merges the streamed pieces too (`vendor/genai/CMDR-PATCH.md`); they arrive as `Custom`
  parts and go back as `reasoning_details`. Unverified live (the key has no credits).
- **Everything else on chat-completions** (Together, xAI, Qwen/DashScope, Perplexity, Ollama, LM Studio, custom, and
  Cmdr's own llama-server): unverified, so they keep what they always got, no reasoning back. Wrongly sending it is a
  400 that wedges the loop (Groq); wrongly withholding it costs continuity, except on a provider that requires it.

Which chat-completions endpoints get reasoning back is ONE allowlist by host, `ai::client::
replays_chat_reasoning_for_host` (DeepSeek, Mistral, Fireworks, OpenRouter), carried as `AiBackend::replays_chat_reasoning` into
`ReasoningOrigin::takes_reasoning`. Off means the agent neither persists nor replays chat reasoning for that endpoint.
To add a host, run its live tool loop (`live_smoke_test.rs`) and anchor the result here.

**Replay goes only to the model that made it.** A signature is one provider's continuation token: Claude's sent to
OpenAI as `encrypted_content` (or one Claude's to another) is a 400. Every blob carries the `{adapter, model}` it came
from, and a thread that switches model mid-way drops the old model's reasoning. Blobs persisted before the stamp
replay within their `ProviderTag` family, as they always did.

## Reasoning effort

`ReasoningEffort` (`types.rs`) is a per-call input to `AgentLlm::respond`, inside `CallSettings { effort,
max_output_tokens }` on `TurnParams.call`. The interactive send resolves both from the Effort picker
(`../chat/DETAILS.md` § Reasoning effort and the reply's room); wakes and tests pass `CallSettings::default()`
(`ProviderDefault` in the base 12,000-token slot). `build_options` sets `max_tokens` from the call, never a constant
here. `genai_effort` maps the effort onto genai's levels, which each
adapter maps onto its provider's knob:

- `ProviderDefault`: nothing sent, so the provider decides (Claude 5.x adaptive, Gemini dynamic, DeepSeek `high`).
  The exception is OpenAI Responses, which gets `medium` (OpenAI's documented default), because `crate::ai`'s
  `adjust_for_model` fills an unset effort with `low` there.
- `Off` → genai `Zero`: Anthropic `thinking: {type: "disabled"}` on models thinking by default (Fable and Mythos can't
  stop and send nothing), OpenAI `reasoning_effort: "none"`, Gemini no level (its default; Pro models can't turn
  thinking off), DeepSeek `thinking: {type: "disabled"}`.
- `Low` / `Medium` / `High` / `Max`: Anthropic `output_config.effort` (`max` where the model has it, else `high`),
  OpenAI `reasoning.effort` / `reasoning_effort`, Gemini 3 `thinkingLevel` (`HIGH` tops out) and Gemini 2.5
  `thinkingBudget`, DeepSeek `reasoning_effort` (it maps `medium` to `high`).

Caveats for the picker:

- An explicit level sent to a model that doesn't reason (OpenAI `gpt-4.1`, most local models) is a 400 there, so the
  picker must know the model reasons (`chat::effort`, from the model-capabilities fetch in `ai/`).
- `ProviderDefault` is NOT enough on the OpenAI Responses API: it means `medium` there, and the reasoning capture adds
  `reasoning.summary` plus the encrypted include, and a model that doesn't reason 400s on each ("Unsupported
  parameter", verified on `gpt-4.1-mini` and `gpt-4o` via `/v1/responses`, `curl`, 2026-10-27). So
  `CallSettings.model_reasons = Some(false)` sends none of it, and its `capture_reasoning_content: false` keeps
  `adjust_for_model` from filling in `low`. No served OpenAI model hits this today (genai routes only `gpt-5*`,
  `*-codex`, and `*-pro` to Responses, and the chat-tuned `gpt-5*-chat-latest` ids 404, 2026-10-27); it guards the
  next one. Elsewhere `ProviderDefault` sends nothing and ran green live on Claude, `gpt-4o-mini`, Gemini, and
  Fireworks (2026-10-27).
- Reasoning spends the same `max_tokens` as the visible reply and the tool calls, so the picker raises the ceiling
  with the level (`chat::effort::reply_room`). A think that still outruns it ends the turn as `UnfinishedReply`. genai's legacy
  Anthropic `budget_tokens` (24,000 at `High`, pre-adaptive Claude only) exceeds the slot outright.
- Reasoning is billed as output, and genai's `completion_tokens` already counts it (Anthropic `output_tokens`, OpenAI
  `completion_tokens` / `output_tokens`, Gemini candidates plus thoughts), so `pricing.rs` meters it. `map_usage`
  covers the one gap: Gemini reporting thoughts with no candidates.

## Error mapping

Provider transport errors are classified by HTTP status once, upstream, in `crate::ai` (`ai_error_for_status`:
401/403 → auth, 429 → rate-limited). `genai_impl` maps that `AiError` to the seam's `AgentLlmError` variant-to-variant
(`impl From<AiError>`), so there is no message-string matching anywhere. `AiError::Managed` stays typed as
`AgentLlmError::Managed(refusal)` (the runtime's `ManagedByOrganization`), never a `Provider` string
(`../../ai/DETAILS.md` § Managed policy). `AgentLlmError::NoKey` /
`NotConfigured` / `BudgetExhausted` are pre-flight/runtime states the runtime raises, not transport errors.

## `ToolId` and the read-only gate

`ToolId` is a typed tool name with an `Unrecognized(String)` fallback, serialized transparently as its bare wire name
(the genai `fn_name`, the DB token, and the IPC string are one identical value). `from_wire_name` is total: an unknown
name resolves to `Unrecognized` rather than failing, so the raw name stays representable for the transparent UI and the
typed "tool not available" result. The read-only guarantee is that `Unrecognized` (and any future write tool) is never
in `agent_tool_view()`, so dispatch refuses it — a typed view-membership check, not a string match. The known variants
are the read-only families, pinned 1:1 to `agent_tool_view()` by a structural test in `tools/mod.rs` (so the catalog
lives in `tools/DETAILS.md` and this list can't rot beside it); `ToolId::KNOWN` excludes `Unrecognized` by design.
`ProposeRenamePlan` and `ProposeSuggestions` are the proposing variants: they stage a reviewable plan and remain subject
to the same typed-view gate, never an approval or write route. `NothingToSuggest` is the odd one out — a pure SIGNAL
whose handler changes nothing, so that a wake's outcome is observable typed rather than read out of the model's wording.

## Tests

- `types.rs`: `ToolId` serializes as a bare wire name; the `from_wire_name` gate shape; an `AgentMessage` with a
  tool-call-with-reasoning survives a serde round-trip with the blob intact (DB persistence fidelity).
- `genai_impl/tests.rs`: the make-or-break genai round-trip (a tool-call-with-reasoning survives agent → genai → agent
  with the blob untouched — the test that goes red the moment the mapping flattens); every reasoning part survives in
  wire order; replay only to the producing model; effort mapping; `AiError` → typed `AgentLlmError`; stop reason and
  usage mapping; tools are never strict.
- `genai_impl/tests/replay_fixtures.rs`: two-round tool loops through the REAL genai adapters against a loopback server
  playing back provider-shaped streams (Anthropic signed, omitted, and redacted thinking; OpenAI Responses encrypted
  items; Gemini call signatures; chat-completions `reasoning_content`, plus a host that takes none). Each asserts round
  two's request body, the bytes the provider validates.
- `fake.rs`: scripted-turn sequencing + `calls_seen` recording; `CallRawTool` yields an `Unrecognized` `ToolId`; a
  `Fail` turn returns its typed error; `SayThenCall` interleaves on the wire and ends text first; `by_prompt` picks from
  the latest user text and the rounds since.
- `live_smoke_test.rs`: `#[ignore]`-gated, one real `respond` call per Tier-1 cloud provider + the local slot, plus a
  real multi-round `list_dir` tool loop with reasoning on per reasoning dialect (`smoke_*_tool_loop`), run manually with the
  matching env key (never in CI's critical path). Verify current model ids from each provider's models endpoint at run
  time — never from training data. The Anthropic base URL must carry `/v1/` (the app's own default); without it genai
  404s.
