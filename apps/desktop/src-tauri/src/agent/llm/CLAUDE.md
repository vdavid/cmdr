# Agent LLM seam (`agent/llm/`)

The provider-agnostic `AgentLlm` boundary the whole chat runtime and UI test against. One method,
`AgentLlm::respond`, makes one cold, self-contained streaming call. Depth (mapping table, reasoning-blob shapes,
decision rationale): `DETAILS.md`.

## Module map

- `types.rs`: the typed message-part model (`AgentMessage`/`AgentPart`/`AgentToolCall`/`ReasoningState`/…), pure data +
  serde, no `genai`/`ai` dependency.
- `mod.rs`: the `AgentLlm` trait + `AgentDeltaStream`.
- `genai_impl.rs`: the genai-backed impl over `crate::ai::AiBackend` — the ONE place `genai` types touch the agent.
- `fake.rs`: the deterministic, zero-network `FakeAgentLlm` (scripted turns; records every `messages` slice).

## Must-knows

- **Never flatten a message to `content: String + reasoning: String`.** A turn is an ordered list of typed parts, and
  opaque reasoning state is **provider-tagged and rides on the part that owns it** (`ReasoningState` on the tool call,
  or a standalone `Reasoning` part). That lossy flat shape is exactly what breaks a multi-step tool loop on step 3
  (the genai reasoning-replay gaps). The
  `ReasoningState.blob` is opaque outside `genai_impl.rs`: persist and replay it untouched, never inspect or reshape it,
  and it NEVER crosses to the frontend.
- **Reasoning is on everywhere and replays verbatim, in wire order, to the model that made it.** Every reasoning part
  genai captures is kept (no dedupe: each adapter picks its own copy) and stamped `{adapter, model}`; a model switch
  drops it. ❌ Don't drop `capture_reasoning_content`: it's genai's opt-in for Anthropic signatures and OpenAI's
  encrypted items. Chat-completions reasoning goes back only to allowlisted hosts (DeepSeek 400s without it, Groq 400s
  with it). Per-provider rules and evidence: `DETAILS.md` § Reasoning round-trip.
- **A model that doesn't reason must say so: `CallSettings.model_reasons = Some(false)`.** An explicit effort 400s on
  it, and on the OpenAI Responses API so does asking for reasoning back, even under `ProviderDefault`.
  `DETAILS.md` § Reasoning effort.
- **`ToolId` comes from the tool table; `types.rs` only re-exports it.** A raw provider tool name resolves through
  `ToolId::from_wire_name`, and a name outside the table becomes `Unrecognized(raw)`, which is a refusal, not an error.
  ❌ Known doesn't mean allowed: `../tool_gate.rs` decides that with a typed view check, never a string match on the name.
- **A nameless tool call is dropped BOTH ways, and arguments always leave as a JSON object.** Qwen sometimes stops with
  a tool call that has an id and no `fn_name`; replaying it 400s that thread forever. Inbound keeps it out of the store,
  outbound drops what old threads already stored (the call plus its result). Keep both halves.
- **`ToolId` and errors classify by variant/HTTP status, never by message string**. Provider
  errors are status-classified upstream in `crate::ai` and mapped variant-to-variant here.
- **Every request asks for caching, in its provider's shape: one `CachePlan` (`genai_impl/cache.rs`).** A wrong ask
  isn't a no-op: a message mark on OpenAI GPT-5.6+ turns on genai's explicit mode, which caches nothing once the last
  message is a tool result. `DETAILS.md` § Prompt caching.
- **Tool declarations are never `strict: true`**: OpenAI strict also demands all-required, which genai doesn't
  enforce, so an optional prop 400s. `tool_declaration_to_genai` leaves strict unset.

The `AgentPart` ⇄ genai `ContentPart` mapping table, blob shapes, the reasoning round-trip, error mapping, and live
smokes: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
