# Agent LLM seam (`agent/llm/`)

The provider-agnostic `AgentLlm` boundary the whole chat runtime and UI test against. One method,
`AgentLlm::respond`, makes one cold, self-contained streaming call. Depth (mapping table, reasoning-blob shapes,
decision rationale): `DETAILS.md`.

## Module map

- `types.rs`: the typed message-part model (`AgentMessage`/`AgentPart`/`AgentToolCall`/`ReasoningState`/`ToolId`/…),
  pure data + serde, no `genai`/`ai` dependency.
- `mod.rs`: the `AgentLlm` trait + `AgentDeltaStream`.
- `genai_impl.rs`: the genai-backed impl over `crate::ai::AiBackend` — the ONE place `genai` types touch the agent.
- `fake.rs`: the deterministic, zero-network `FakeAgentLlm` (scripted turns; records every `messages` slice).

## Must-knows

- **Never flatten a message to `content: String + reasoning: String`.** A turn is an ordered list of typed parts, and
  opaque reasoning state is **provider-tagged and rides on the part that owns it** (`ReasoningState` on the tool call,
  or a standalone `Reasoning` part). That lossy flat shape is exactly what breaks a multi-step tool loop on step 3
  (spike Gaps A/B — `docs/specs/ask-cmdr-genai-spike.md`). The
  `ReasoningState.blob` is opaque outside `genai_impl.rs`: persist and replay it untouched, never inspect or reshape it,
  and it NEVER crosses to the frontend.
- **Reasoning is OFF on the Anthropic and OpenAI-Responses paths in v1.** genai drops their reasoning state on replay
  (Gaps A/B), so a reasoning-on loop 400s / degrades. Gemini round-trips its per-`functionCall` `thoughtSignature`
  correctly and keeps reasoning. `build_options` sets `ReasoningEffort::None` for those two adapters — don't "helpfully"
  turn it on before the genai capture+replay patch lands.
- **`ToolId::Unrecognized` is the read-only choke point, not an error.** A raw provider tool name resolves through
  `ToolId::from_wire_name`; an unknown name becomes `Unrecognized(raw)`, which is never in the agent's tool view, so
  dispatch refuses it. The gate is a typed variant/view check, never a string match on the name.
- **`ToolId` and errors classify by variant/HTTP status, never by message string** (`no-string-matching`). Provider
  errors are status-classified upstream in `crate::ai` and mapped variant-to-variant here.
- **Tool declarations are never `strict: true`** (Gap D): OpenAI strict also demands all-required, which genai doesn't
  enforce, so an optional prop 400s. `tool_declaration_to_genai` leaves strict unset.

Depth (the `AgentPart` ⇄ genai `ContentPart` mapping table, blob shapes, the thought-signature dedupe, live smokes):
`DETAILS.md`.
