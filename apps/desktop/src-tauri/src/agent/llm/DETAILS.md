# Agent LLM seam details

Pull-tier docs for `agent/llm/`. Must-know invariants live in [CLAUDE.md](CLAUDE.md). Contract:
[`docs/specs/ask-cmdr-plan.md`](../../../../../../docs/specs/ask-cmdr-plan.md) §6; capability spike:
[`docs/specs/ask-cmdr-genai-spike.md`](../../../../../../docs/specs/ask-cmdr-genai-spike.md).

## The seam

`AgentLlm::respond(system, tools, messages, cancel)` makes ONE cold, self-contained streaming call. `messages` is the
fully-assembled prompt — context assembly (prefix stability, elision, the envelope) is the runtime's job (M5), not the
LLM's. The call returns an `AgentDeltaStream` (`BoxStream<'static, Result<AgentDelta, AgentLlmError>>`); the terminal
`AgentDelta::End` carries the fully-assembled final `AgentMessage`, including any opaque provider state, for persistence
and replay.

The trait is written as a boxed-future return (`BoxFuture`) rather than `async fn` so it stays object-safe
(`Box<dyn AgentLlm>`) without pulling in `async-trait`.

Two implementations:
- `GenaiAgentLlm` (`genai_impl.rs`) wraps a configured `crate::ai::AiBackend` (the interactive model slot; slot
  resolution is M8). It reuses the backend's adapter routing (`remote_model_iden` — the load-bearing Ollama-fallback
  fix) and its stream-cancel model, via two `pub(crate)` seam methods added to `AiBackend`: `resolve_adapter` and
  `exec_chat_stream_request` (the prompt-only helpers can't express a multipart tool loop).
- `FakeAgentLlm` (`fake.rs`) is scripted with `ScriptedTurn`s (one consumed per `respond`) and records every `messages`
  slice via `calls_seen()`, so a test asserts the exact assembled prompts. `CallRawTool(name, args)` emits a raw tool
  name to exercise M4's read-only parse gate.

## `AgentPart` ⇄ genai `ContentPart` mapping

`genai_impl.rs` owns the only translation between the typed part model and genai's wire model. The reasoning `blob` is
opaque everywhere else; its whole vocabulary is two keys, owned here: `thought_signatures` (an array) and
`reasoning_content` (a string).

Agent → genai (request / replay), per `agent_part_to_genai`:

- `Text(s)` → `ContentPart::Text(s)`.
- `ToolCall{call_id, tool, arguments, reasoning}` → `ContentPart::ToolCall{call_id, fn_name = tool.as_wire_name(),
  fn_arguments = arguments, thought_signatures = blob.thought_signatures}`.
- `ToolResult{call_id, content}` → `ContentPart::ToolResponse{call_id, content = JSON string}` (a bare string passes
  through unquoted so it round-trips; a structured value serializes to JSON text).
- `Reasoning(state)` → `ThoughtSignature` part(s) if the blob carries signatures, else a `ReasoningContent` part if it
  carries reasoning text, else nothing. An Anthropic thinking blob has no lossless genai representation (Gap A) and maps
  to nothing — v1 runs Anthropic reasoning-off, so it doesn't arise.

genai → agent (parse), per `genai_content_to_agent_parts(content, provider)`:

- `Text` → `Text`; `ToolResponse` → `ToolResult`; `ReasoningContent` → `Reasoning{reasoning_content}`.
- `ToolCall` → `AgentToolCall`, with `tool = ToolId::from_wire_name(fn_name)` and reasoning captured from
  `thought_signatures` (tagged with `provider`).
- `ThoughtSignature` (standalone) → a `Reasoning` part **only when the message has no tool calls**. genai attaches
  captured thought signatures BOTH to the first tool call AND as leading standalone parts (see genai `StreamEnd::from`),
  so mapping both would duplicate the reasoning; the tool call is the canonical home when tool calls exist.
- `Binary` / `Custom` → dropped (the agent's read-only toolset never produces them).

Stream events → `AgentDelta` (`map_stream_event`): `Chunk` → `Text` (empty chunks skipped); `ReasoningChunk` /
`ThoughtSignatureChunk` → `ReasoningTick` (content never surfaced); `ToolCallChunk` → `ToolCallStarted`; `End` →
`End{stop, usage, message}` built from the captured content, stop reason, and usage; a stream error → mapped
`AgentLlmError`.

## Reasoning posture (spike verdict)

`build_options` captures content + tool calls + usage + reasoning content, and sets the reasoning effort per adapter:

- Anthropic, OpenAI-Responses: `ReasoningEffort::None`. genai drops their reasoning state on replay (Gaps A/B: the
  Anthropic adapter never re-serializes the thinking block + signature; the OpenAI-Responses adapter never re-serializes
  reasoning items and forces `store=false`), so a reasoning-on multi-step loop 400s or silently degrades. Off is the
  honest v1 posture. `crate::ai`'s `adjust_for_model` won't override an explicit effort.
- Gemini and everything else: left unset (provider default). Gemini round-trips its per-`functionCall`
  `thoughtSignature` correctly, so its reasoning is preserved end to end; OpenAI chat-completions is stateless (nothing
  to round-trip).

Follow-up (plan §13): a scoped local `[patch.crates.io]` genai patch (or upstream PR to issue #213) for Anthropic
thinking capture+replay, needed before Tier-1-certifying Anthropic with thinking on — newer Claude models default
thinking on, so "just disable it" won't hold long-term.

## Error mapping

Provider transport errors are classified by HTTP status once, upstream, in `crate::ai` (`ai_error_for_status`:
401/403 → auth, 429 → rate-limited). `genai_impl` maps that `AiError` to the seam's `AgentLlmError` variant-to-variant
(`impl From<AiError>`), so there is no message-string matching anywhere (`no-string-matching`). `AgentLlmError::NoKey` /
`NotConfigured` / `BudgetExhausted` are pre-flight/runtime states the runtime raises (M5), not transport errors.

## `ToolId` and the read-only gate

`ToolId` is a typed tool name with an `Unrecognized(String)` fallback, serialized transparently as its bare wire name
(the genai `fn_name`, the DB token, and the IPC string are one identical value). `from_wire_name` is total: an unknown
name resolves to `Unrecognized` rather than failing, so the raw name stays representable for the transparent UI and the
typed "tool not available" result. The read-only guarantee is that `Unrecognized` (and any future write tool) is never
in `agent_tool_view()`, so dispatch refuses it — a typed view-membership check, not a string match. The known variants
are the read-only families (`AppState`, `ListDir`, `LargestDirs`, `ImportantFolders`, `FolderImportance`, `ListVolumes`,
`OperationsList`, `OperationsGet`), pinned 1:1 to `agent_tool_view()` by a structural test in `agent/tools`;
`ToolId::KNOWN` excludes `Unrecognized` by design.

## Tests

- `types.rs`: `ToolId` serializes as a bare wire name; the `from_wire_name` gate shape; an `AgentMessage` with a
  tool-call-with-reasoning survives a serde round-trip with the blob intact (DB persistence fidelity).
- `genai_impl.rs`: the make-or-break genai round-trip (a tool-call-with-reasoning survives agent → genai → agent with
  the blob untouched — the test that goes red the moment the mapping flattens); `AiError` → typed `AgentLlmError`; stop
  reason and usage mapping; tools are never strict.
- `fake.rs`: scripted-turn sequencing + `calls_seen` recording; `CallRawTool` yields an `Unrecognized` `ToolId`; a
  `Fail` turn returns its typed error.
- `live_smoke_test.rs`: `#[ignore]`-gated, one real `respond` call per Tier-1 cloud provider + the local slot, run
  manually with the matching env key (never in CI's critical path). The M1 preview of M8's full certification pass.
  Verify current model ids from each provider's models endpoint at run time — never from training data.
