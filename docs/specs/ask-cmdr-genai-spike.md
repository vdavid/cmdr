# genai 0.6.0-beta.19 capability spike — report

Agent-spec §18.1 / ask-cmdr-spec §3 step 0. Gating question: can the pinned `genai` crate drive real multi-step LLM tool
loops, and above all **round-trip opaque reasoning state**, for Anthropic + OpenAI + Gemini?

- **Subject**: `genai = "=0.6.0-beta.19"`. Source read at
  `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/genai-0.6.0-beta.19/` (all citations below are this pin unless
  marked "main").
- **Live coverage: the OpenAI chat-completions adapter is live-verified against a real server; the cloud providers are
  source-verified.** Every cloud key is dead (OpenAI out of quota → `429 insufficient_quota`, OpenRouter $0 credits, no
  Anthropic/Gemini key), so Anthropic, Gemini, and the OpenAI Responses API rest on the source audit. But Cmdr's **own
  local llama-server** (Ministral 3B, the production install) is OpenAI-compatible, so the harness live-drove genai's
  **OpenAI chat-completions adapter** end-to-end for free — all four scenarios passed, closing rows 1/5/6 for that
  adapter (`[LL]`, §2). Remaining cloud verification is a one-command-per-provider follow-up once keys refresh (§5).
- **Why source is conclusive for most of this**: request serialization and response parsing in genai are **pure,
  deterministic `serde_json` transforms** — a `ContentPart` either has a match arm that emits a wire field or it has
  `=> {}`. "Is the Anthropic thinking signature re-serialized?" is answered by reading the assistant match arm, not by a
  network call. §5 separates the findings that source settles outright from the few that genuinely need a live packet
  (provider-side acceptance/rejection of a shape genai _does_ emit).
- **Incidental evidence (not the briefed test):** before the keys were confirmed dead I got a few OpenRouter _free_-tier
  runs through, which exercise **genai's OpenAI chat-completions adapter** (OpenRouter normalizes to that wire format).
  A 4-turn `get_weather` loop completed clean and streaming surfaced tool-call chunks. This corroborates the OpenAI-cc
  plumbing but is **not** a direct-provider live test, so the matrix still marks those cells `[S]`; the runs are cited
  as supporting notes where relevant.
- **Total API spend: $0.00** (OpenAI 429'd unbilled; OpenRouter usage stayed $0 on free models).

---

## 1. Verdict matrix

Markers: `[LL]` **live-local** — verified against Cmdr's own local llama-server (Ministral 3B, the production install)
through genai's real OpenAI chat-completions adapter, zero keys/spend. `†` — additionally corroborated by an incidental
OpenRouter free-tier run. Unmarked = **source-verified only** (no direct-provider live run was possible; see §5).
Anthropic, Gemini, and OpenAI-Responses verdicts are entirely source-verified — no key exercises them. "OpenAI-cc" =
chat-completions adapter (`gpt-4.1*`, `gpt-4o*`, the local server, and every OpenAI-compatible BYOK provider).
"OpenAI-resp" = Responses API adapter, which genai auto-routes `gpt-5*` / `*-codex` / `*-pro` to, and which Cmdr's
`client.rs` relies on.

| Capability                                                 | Anthropic  | OpenAI-cc                                            | OpenAI-resp       | Gemini            |
| ---------------------------------------------------------- | ---------- | ---------------------------------------------------- | ----------------- | ----------------- |
| 1. Multi-step tool loop (replay history, all ids answered) | works      | **works `[LL]`** †                                   | works             | works             |
| 2. Parallel tool calls (batch-answer semantics)            | works      | works (loop `[LL]`; batch-in-one-turn source-only) † | works             | workaround-needed |
| 3. **Opaque reasoning-state round-trip**                   | **broken** | n/a — nothing to round-trip                          | **broken**        | works             |
| 4. Tool schema handling (strict / dialect)                 | works      | works; strict all-required gap (source-only) †       | works, strict gap | works, transforms |
| 5. Streaming with tool calls                               | works      | **works `[LL]`** †                                   | works             | works             |
| 6. Stop reasons / usage / typed errors                     | works      | **works `[LL]`** †                                   | works             | works             |

**Headline:** the tool-loop _plumbing_ is solid on all four adapters — and the OpenAI-cc adapter's loop replay,
streaming, and stop-reason/usage handling are now **live-verified end-to-end** against a real server (rows 1, 5, 6). The
make-or-break row 3 splits hard: **Gemini round-trips reasoning state correctly; Anthropic and the OpenAI Responses API
drop it** — and this is not a pin artifact, it persists on genai `main` (open issue
[#213](https://github.com/jeremychone/rust-genai/issues/213)).

---

## 2. Evidence per verdict

`[LL]` = live-verified against Cmdr's local llama-server (Ministral 3B) through genai's OpenAI chat-completions adapter.
`[OR]` = corroborated by an incidental OpenRouter free-tier run (same adapter). `[S]` = source-verified. No direct
**cloud** provider live run was possible; the local server exercises the OpenAI-cc adapter plumbing for real.

### The genai message model (foundation)

`MessageContent` is a `Vec<ContentPart>` (`chat/message_content.rs:10`). `ContentPart`
(`chat/content_part/common.rs:13-38`) has the variants that matter for reasoning state: `ToolCall`, `ToolResponse`,
`ThoughtSignature(String)`, `ReasoningContent(String)`, plus `Text`/`Binary`/`Custom`. `ToolCall`
(`chat/tool/tool_call.rs:7-27`) carries `call_id`, `fn_name`, `fn_arguments: Value`, **and**
`thought_signatures: Option<Vec<String>>`. `ToolResponse` (`chat/tool/tool_response.rs:5-11`) is just
`{call_id, content: String}`. `Tool` (`chat/tool/tool_base.rs:7-50`) has `schema: Option<Value>` and
`strict: Option<bool>`.

So the _type model can hold_ an opaque per-message reasoning blob (as `ThoughtSignature` / `ReasoningContent` parts, or
on `ToolCall.thought_signatures`). The question is entirely whether each adapter **captures it on parse** and
**re-serializes it on replay**. That is where the three providers diverge.

### Row 1 — Multi-step tool loops

- **OpenAI-cc `[LL]`**: **live-verified end-to-end against Cmdr's local llama-server** (Ministral 3B). A 5-turn
  `sequential` loop (`advance` 0→1→2→3→done) ran clean: each turn parsed a tool call with a real id and correct args,
  the harness replayed the full assistant turn + a `Tool`-role message of `ToolResponse`s, the next call resumed
  correctly, and the final turn returned `stop_reason=Completed("stop")` with no tool calls. Notably, two middle turns
  returned **mixed `["Text", "ToolCall"]`** assistant content and genai serialized _both_ parts back correctly
  (`openai/adapter_shared.rs:344-385`: texts joined into `content`, tool calls into the `tool_calls` array). Also
  corroborated `[OR]` earlier on OpenRouter `gpt-oss-120b`. Serialization: assistant `tool_calls` array keyed by `id`,
  tool answers as `role:"tool"` keyed by `tool_call_id` (`openai/adapter_shared.rs:388-397`).
- **Anthropic `[S]`**: `into_anthropic_request_parts` serializes assistant `tool_use` blocks
  (`anthropic/adapter_impl.rs:687-704`) and maps `ChatRole::Tool` → a `user` message of `tool_result` blocks keyed by
  `tool_use_id` (`:732-746`). Response parse pushes each `tool_use` as a `ToolCall` (`:398-412`). All ids the caller
  supplies are answered. Correct.
- **OpenAI-resp `[S]`**: assistant tool calls become flat `function_call` items, tool answers become
  `function_call_output` items keyed by `call_id` (`openai_resp/adapter_impl.rs:479-528`). Correct plumbing.
- **Gemini `[S]`**: assistant `functionCall` parts, tool answers as `functionResponse` parts, with a post-pass
  `merge_consecutive_tool_response_entries` that folds all tool answers into one `user` turn as Gemini requires
  (`gemini/adapter_impl.rs:594-753`, `:829-858`). Correct.

### Row 2 — Parallel tool calls

All adapters can emit and replay several calls per turn. The **Gemini caveat**: Gemini omits call ids, so genai
**synthesizes** `call_id = "call#{fn_name}#{counter}"` (`gemini/adapter_impl.rs:322-326`). On replay, the
`functionResponse` is written with `"name": tool_response.call_id` (`:602-611`, `:713-722`) — i.e. the _synthetic id_
(`call#read_file#0`), **not** the function name (`read_file`) that the matching `functionCall` was sent under
(`:594-601`, `:640-648`). Gemini matches responses to calls by `name`/order; a name that matches neither the function
nor any id is a real mismatch risk for parallel calls. `merge_consecutive_tool_response_entries` keeps them in one turn
(order preserved), which may save single-call turns, but this is the classic "works in a demo, degrades on step 3 of a
parallel loop" shape. **Must be verified live once a Gemini key exists.**

**Live note (OpenAI-cc, `[LL]`)**: the local-server `parallel` run completed the loop correctly, but Ministral 3B
emitted **one** `get_weather` call per turn rather than several in one turn — a **model-competence** outcome, not a
plumbing one. So the multi-call-in-one-turn _batch-answer_ path (assistant with N `tool_calls`, answered by N `tool`
messages) was **not exercised live**; its correctness rests on the source (`openai/adapter_shared.rs:351-397` handles a
`Vec` of tool calls and a per-response `tool` message). Worth a live re-check with a model that actually parallelizes.

### Row 3 — Opaque reasoning-state round-trip (the make-or-break)

- **Gemini — works `[S]`.** Response parse extracts per-part `thoughtSignature` → `GeminiChatContent::ThoughtSignature`
  (`gemini/adapter_impl.rs:311-312`) and attaches the collected signatures to the first tool call's `thought_signatures`
  _and_ as leading `ThoughtSignature` parts (`:171-178`). Replay re-attaches the signature to the **exact**
  `functionCall` part via the `pending_thought` mechanism — injecting `thoughtSignature` alongside `functionCall` in the
  same Part object (`:640-668`), including the Gemini-3 `"skip_thought_signature_validator"` sentinel when there's no
  thought on the first call (`:659-664`). This is exactly Gemini's documented requirement. Correct and carefully
  engineered.

- **Anthropic — broken `[S]`.** Two independent failures:
  1. **Capture drops the signature.** Response parse handles a `thinking` item as
     `"thinking" => reasoning_content.push(item.x_take("thinking")?)` (`anthropic/adapter_impl.rs:397`) — it takes only
     the `thinking` _text_ and **discards the `signature` field entirely**. The text lands in the flat
     `ChatResponse.reasoning_content: Option<String>` (`:421-425`), not as a structured part.
  2. **Replay never re-sends thinking.** In the assistant arm, `ContentPart::ThoughtSignature(_) => {}` and
     `ContentPart::ReasoningContent(_) => {}` are **explicitly ignored** (`:708-709`). No `thinking` block is ever
     emitted on the request. Anthropic **validates thinking-block signatures server-side and requires** the assistant
     turn that carries `tool_use` to begin with its `thinking` block (with signature) whenever extended/adaptive
     thinking is active — otherwise it returns HTTP 400. So: **with thinking enabled, multi-step tool loops on Claude
     break.** With thinking _disabled_, there are no thinking blocks and loops work fine. This is confirmed on genai
     `main` (well past the pin): assistant serialization still ignores both parts
     (`anthropic/adapter_shared.rs:238-239, 281-282` on main), response parse still drops the signature (`:557` on
     main), and it is tracked as **open issue #213** ("Anthropic adapter drops assistant ReasoningContent when
     serializing requests"), whose own text calls out tool-use continuation as the visible break. It's getting _worse_:
     `main` shows newer Claude models (Sonnet 5 family) default thinking **on** (`anthropic/adapter_shared.rs:743-744`),
     and a code note states **"Fable/Mythos thinking is always-on and cannot be disabled"** (`:841-842`) — so the "just
     disable thinking" escape hatch won't exist for the newest models.

- **OpenAI Responses API — broken `[S]`.** The Responses API is where OpenAI reasoning models keep state, and genai
  routes `gpt-5*` / `*-codex` / `*-pro` here (and Cmdr's `client.rs` depends on that routing).
  - **Non-stream parse ignores reasoning items.** `from_resp_output_item` only handles `message` and `function_call`
    item types (`openai_resp/resp_types/resp_output_helper.rs:56-69`); a `type:"reasoning"` item falls through to `None`
    and is dropped. `to_chat_response` even hard-codes `reasoning_content = None` (`openai_resp/adapter_impl.rs:280`).
  - **Stream parse captures it but only as an opaque blob, opt-in.** The streamer, _when_ `capture_reasoning_content` is
    set, reads `encrypted_content` off `type:"reasoning"` output items into `captured_thought_signatures`
    (`openai_resp/streamer.rs:233-247`). So the encrypted blob _can_ be captured — but only while streaming, only
    opt-in.
  - **Replay never re-serializes it.** The assistant arm ignores `ThoughtSignature` and `ReasoningContent`
    (`openai_resp/adapter_impl.rs:501-502`); there is **no code path that emits a `reasoning` item** into the `input`
    array. Combined with genai forcing **`store=false` by default** ("Privacy first: we never implicitly set
    store=true", `:113-122`), a stateless Responses tool loop **drops the reasoning chain between steps**. Depending on
    model/version this ranges from silent degradation to a 400 ("reasoning item provided without its required following
    item" class).
  - **Escape hatch exists but conflicts with Cmdr's design.** genai supports **stateful** Responses sessions —
    `previous_response_id` + `store=true` on `ChatRequest` (`chat/chat_request.rs:22-32`,
    `openai_resp/adapter_impl.rs:104,116,137-139`). In that mode OpenAI keeps reasoning server-side, so no client
    round-trip is needed. But it means transcripts live on OpenAI's servers — against ask-cmdr-spec §2 principle 3
    ("every message is a cold, self-contained API call assembled from state") and the privacy posture.

- **OpenAI chat-completions — n/a `[OR]`.** Chat-completions is inherently stateless for reasoning (OpenAI doesn't
  preserve it; there's nothing to round-trip), so loops "work" with expected reasoning loss. genai does hoist a
  `ReasoningContent` part back into a sibling `reasoning_content` request field for providers that want it (Kimi,
  DeepSeek) (`openai/adapter_shared.rs:363, 381-383`). Live loop confirmed `reasoning_content` is captured per turn and
  loops complete.

### Row 4 — Tool schema handling

- One JSON-Schema `Tool.schema` in; each adapter lints per provider.
- **OpenAI (cc + resp) — strict gap `[LL]/[S]`.** When `Tool.strict == true`, genai walks the schema and injects
  `additionalProperties: false` on every `type:"object"` node (`openai/adapter_shared.rs:414-424`;
  `openai_resp/adapter_impl.rs:581-591`) — but it does **not** force every property into `required`, which OpenAI strict
  mode also demands. A strict schema with an **optional** property will be rejected by OpenAI-direct strict enforcement.
  My strict-hostile tool (optional `hint`, no `additionalProperties`, `strict=true`) was _accepted and called_ both by
  the local llama-server `[LL]` and earlier by OpenRouter→`gpt-oss` — but **neither enforces OpenAI strict mode**, so
  this only proves genai _emits_ the schema without erroring; the **all-required rejection remains source-verified
  only** for the real OpenAI endpoint. Mitigation is trivial: **don't set `strict:true`**, or make all tool params
  required (Cmdr controls its own tool schemas).
- **Gemini — transforms `[S]`.** genai runs `to_openapi_schema()` over both tool params and response schema
  (`gemini/adapter_impl.rs:452, 815`), coercing to Gemini's OpenAPI-ish subset. Complex schemas (unusual keywords,
  `$ref`, etc.) should be checked live.
- **Anthropic — permissive `[S]`.** Schema passed through as `input_schema` unchanged (`anthropic/adapter_impl.rs:846`).

### Row 5 — Streaming with tool calls

- **OpenAI-cc `[LL]`**: live against the local server, the stream surfaced 7 `ToolCallChunk` deltas and the `End` event
  carried the fully-assembled tool call (`captured_tool_calls=["get_weather"]`) plus usage (`thought_sig=0`, as expected
  for a non-reasoning model). Corroborated `[OR]` on `gpt-oss-120b` (11 `ToolCallChunk` + 6 `ReasoningChunk`).
- The event enum (`chat/chat_stream.rs:68-87`) is
  `Start | Chunk | ReasoningChunk | ThoughtSignatureChunk | ToolCallChunk | End(StreamEnd)`. Thinking deltas interleave
  as `ReasoningChunk` / `ThoughtSignatureChunk`. `StreamEnd` (`:105-124`) can carry captured content (incl. tool calls +
  thought signatures), usage, stop reason, and `response_id`, gated by `capture_*` options. The Responses streamer
  accumulates `function_call` args and emits `ToolCallChunk` (`openai_resp/streamer.rs:145-208`). Anthropic emits
  incremental `ToolCallChunk` (changelog).
- **Cancellation**: drop the stream → reqwest body closes; this is exactly what Cmdr's `client.rs`
  `chat_completion_stream` already does (its doc-comment: "drop the returned stream … billing stops").

### Row 6 — Stop reasons, usage, errors

- **Stop reasons `[LL]`**: typed + normalized. `StopReason` (`chat/chat_response.rs:22-61`) maps all providers' strings
  to `Completed | MaxTokens | ToolCall | ContentFilter | StopSequence | Other`, keeping the raw string. Live against the
  local server the loop reported `ToolCall("tool_calls")` on each tool turn and `Completed("stop")` at the end — exactly
  the mapping the source predicts.
- **Usage `[LL]`**: per-call, normalized OpenAI-style. Each adapter fills `Usage` (`prompt_tokens`, `completion_tokens`,
  `total_tokens`, cache + reasoning detail breakdowns) — `anthropic/adapter_impl.rs:504-545`,
  `gemini/adapter_impl.rs:474-543`, `openai/adapter_shared.rs:215-243`. Live: every local-server turn returned populated
  `prompt`/`completion` token counts.
- **Errors**: genai does **not** expose a distinct `RateLimited`/`AuthFailed` enum variant, but it carries the HTTP
  **status code** in `webc::Error::ResponseFailedStatus{status, body}` — which Cmdr's `map_genai_error` already branches
  on (429→RateLimited, 401/403→AuthFailed). Live: a 429 came back cleanly classified by status. This satisfies the
  `no-string-matching` rule (branch on status, not body text).

---

## 3. Gap list (precise) + fix options

### Gap A — Anthropic thinking-state is neither captured nor replayed (SEVERITY: high if thinking on)

- **What's dropped**: the `thinking` block's `signature` (never parsed) and the whole thinking block (never
  re-serialized). `anthropic/adapter_impl.rs:397` (parse), `:708-709` (replay). Persists on `main`; open issue #213.
- **Consequence**: multi-step tool loops on Claude **with thinking enabled** hit Anthropic 400 (assistant tool_use turn
  must start with its thinking block). Thinking-disabled loops are fine.
- **Fix options**:
  1. _Keep thinking disabled for Anthropic in v1_ (set `ReasoningEffort::Zero`/`None`). **~0 effort**, but forfeits
     extended thinking, and won't work for always-thinking models (Fable/Mythos per the `main` note).
  2. _Local patch via `[patch.crates.io]`_: capture `{thinking, signature}` into a `ContentPart` (a `ReasoningContent`
     carrying both, or reuse `ThoughtSignature` for the signature) and re-serialize a `thinking` block in the assistant
     arm before `tool_use`. **~0.5-1 day**; must track upstream #213.
  3. _Upstream PR_ to genai (issue #213 is open and the maintainer agrees on the shape). **~1 day + review latency**;
     cleanest long-term.
  4. _Per-provider adapter behind the trait for Anthropic only_. **~2-3 days**; heaviest, only if 2/3 stall.

### Gap B — OpenAI Responses API reasoning items not round-tripped (SEVERITY: high for reasoning models on Cmdr's path)

- **What's dropped**: `type:"reasoning"` items — ignored on non-stream parse (`resp_types/resp_output_helper.rs:56-69`),
  captured only opt-in while streaming (`openai_resp/streamer.rs:233-247`), and **never** re-serialized into `input`
  (`openai_resp/adapter_impl.rs:501-502`). `store=false` forced by default.
- **Consequence**: stateless multi-step tool loops on `gpt-5*` (Cmdr's Responses-routed models) lose the reasoning chain
  between steps → degradation, and 400s on stricter model versions.
- **Fix options**:
  1. _Use genai's stateful `previous_response_id` + `store=true`_. **~0.5 day**, but stores transcripts on OpenAI
     (against §2-3 + privacy) — likely rejected.
  2. _Local patch / upstream PR_: parse reasoning items (incl. `encrypted_content`) into a `ContentPart` on non-stream
     too, and emit them back into `input` on replay (with `include:["reasoning.encrypted_content"]`, `store=false`).
     **~1-1.5 days**. This is the design OpenAI documents for stateless reasoning continuation.
  3. _Route OpenAI reasoning models via chat-completions instead of Responses_ (accept reasoning loss like any stateless
     CC model). **~0.5 day** (adjust Cmdr's `remote_model_iden`), degrades gracefully, no patch — a pragmatic v1 stance.

### Gap C — Gemini functionResponse name = synthetic call_id, not fn_name (SEVERITY: medium, unverified)

- **What's wrong**: `gemini/adapter_impl.rs:602-611` / `:713-722` write `functionResponse.name = call_id` (`call#fn#N`),
  mismatching the `functionCall.name = fn_name`.
- **Consequence**: possible response/call mismatch on parallel Gemini tool turns. Unverified (no Gemini key).
- **Fix options**: verify live first. If real: local patch to carry `fn_name` on `ToolResponse` (or map by order).
  **~0.5 day** + a genai change (`ToolResponse` has no `fn_name` field today).

### Gap D — OpenAI strict mode omits all-required (SEVERITY: low, trivially avoidable)

- `strict:true` adds `additionalProperties:false` but not all-required (`openai/adapter_shared.rs:414-424`). Optional
  props → OpenAI-direct 400. **Fix: don't use `strict:true`, or make all params required.** ~0 effort. Cmdr owns its
  schemas.

**Does a newer genai fix any of this?** No. Latest is `v0.7.0-beta`; its reasoning changelog entries are about
_disabling_ thinking (`ReasoningEffort::Zero`), not round-tripping it. Gaps A and B are unchanged on `main`, and A is an
open issue (#213). Bumping the pin does not buy a fix.

---

## 4. Recommendation

**Is an opaque per-message provider-state blob necessary and sufficient? — Necessary yes, but genai does not yet make it
sufficient, and the blob must be structured per-provider, not a single flat field.**

- The `AgentLlm` trait **must** carry, on each assistant message, an **opaque, provider-tagged reasoning-state payload**
  that survives DB persistence and replay untouched — because Gemini (per-`functionCall` `thoughtSignature`), Anthropic
  (per-`thinking`-block text **+ signature**), and OpenAI-resp (per-`reasoning`-item `encrypted_content`) each need
  their blob **re-attached to the right structural position**, not concatenated into a text field. genai's own type
  model already points the way: `ToolCall.thought_signatures` + `ContentPart::{ThoughtSignature, ReasoningContent}`.
  Model the trait's assistant turn as an ordered list of typed parts (text, tool-calls-with-attached-state,
  reasoning-blob) plus a provider tag — do **not** flatten to `content: String + reasoning: String`, which is exactly
  the lossy shape that breaks on step 3.
- **genai 0.6.0-beta.19 is usable, but not as-is for reasoning models.** It works today, unpatched, for: **Gemini**
  (full reasoning round-trip), **OpenAI chat-completions** (loops corroborated on OpenRouter free models; the
  serialization is source-conclusive), and **Anthropic/OpenAI-resp with thinking disabled**. It is **not sufficient**
  for Anthropic-with-thinking or OpenAI-Responses-reasoning multi-step loops — those need a **local `[patch.crates.io]`
  patch or upstream PR** to capture + replay the reasoning blob (Gaps A, B). The right sequencing: build the trait now
  with the opaque-blob seam designed in from day one; ship v1 with **thinking/reasoning kept minimal or off on the
  Anthropic and OpenAI-Responses paths** (graceful degradation, matching agent-spec §10.4's stance), and land the genai
  patches (or a per-provider adapter behind the trait for Anthropic + OpenAI-resp only) before certifying those
  providers with reasoning on. Do **not** hand-roll adapters in parallel to genai (agent-spec §10.2) — the gaps are two
  well-scoped patches, not a rewrite.
- **On the local-model slot (agent-spec §10.4)** — first real datapoint: through Cmdr's _own_ production server recipe
  (llama-server + Ministral 3B + the app's args), the model drove a coherent **5-step** `advance` tool loop on the first
  try — valid tool-call ids and args every turn, narrated its progress in the mixed text+tool turns, and stopped
  correctly at completion; streaming surfaced tool-call deltas too. So the **plumbing for the local interactive slot is
  proven end-to-end** (genai OpenAI-cc adapter ↔ llama-server), and even a 3B model is coherent on a simple loop.
  Caveats that keep this a _first_ signal, not a certification: it emitted tools **serially, never in a parallel
  batch**, and this was a trivial deterministic toolset — judgment on real agent tasks (ambiguous tool choice, long
  context near the 8k window, honesty caveats) is untested. Consistent with §10.4's "allowed in both slots, labeled
  honestly, degrades gracefully" stance.
- **Live re-verification** of the cloud providers is deferred to a follow-up run (cloud keys are dead now). The exact
  list, and where source is already conclusive, is §5. The local-server pass already closed OpenAI-cc rows 1/5/6.

---

## 5. Pending live verification + how to run the harness

### What source settles outright vs what needs a live packet

genai's request/response handling is pure deterministic `serde_json`, so **whether genai emits or drops a given wire
field is fully answered by the source** — no network needed. What source **cannot** settle is how a provider _reacts_ to
a shape genai does emit (accept vs 400, degrade vs error). Split:

**Conclusively answered from source alone (a live run cannot change these verdicts):**

- Row 3, Anthropic: the thinking `signature` is not parsed and neither `ThoughtSignature` nor `ReasoningContent` is
  serialized in the assistant arm (`anthropic/adapter_impl.rs:397, 708-709`; identical on `main`; issue #213). genai
  _cannot_ round-trip Anthropic thinking state — this is a code fact, not an empirical one.
- Row 3, OpenAI-resp: `type:"reasoning"` items are never serialized into `input` (`openai_resp/adapter_impl.rs:501-502`)
  and are dropped by the non-stream parser (`resp_types/resp_output_helper.rs:56-69`). genai _cannot_ round-trip
  Responses reasoning items statelessly.
- Row 3, Gemini: `thoughtSignature` is captured per-part and re-attached to the exact `functionCall` on replay
  (`gemini/adapter_impl.rs:311-312, 640-668`). The plumbing is present and correct.
- Rows 1, 5, 6 (all providers): message/tool-call/tool-response serialization, streaming event surface, stop-reason and
  usage normalization are all readable in full and correct.
- Gap D (OpenAI strict omits all-required) and Gap C (Gemini `functionResponse.name` = synthetic `call_id`) are code
  facts; what's _unknown_ is only the provider's tolerance of them.

**Needs a live packet to confirm the real-world consequence (verdict of genai unchanged, but severity/behavior TBD):**

1. **Anthropic, thinking ON + tool loop** — confirm the expected 400 ("assistant turn must start with a thinking
   block"), i.e. that the source gap actually breaks the loop and isn't silently tolerated. `REASON=1` +
   `claude-sonnet-4-5`, scenario `sequential`.
2. **OpenAI Responses, reasoning + tool loop** — does a stateless `gpt-5-mini` loop _degrade_ or _400_ when reasoning
   items are dropped? `REASON=1` + `gpt-5-mini`, scenario `sequential`.
3. **Gemini parallel calls (Gap C)** — does the `functionResponse.name = "call#fn#N"` mismatch cause an error or a
   silent mis-pairing? Scenario `parallel` + `gemini-2.5-flash`.
4. **Gemini thoughtSignature across a real ≥3-step loop** — confirm the correct plumbing actually satisfies Gemini
   server-side (incl. the Gemini-3 `skip_thought_signature_validator` path). `REASON=1` + a `gemini-3*` model, scenario
   `sequential`.
5. **OpenAI-direct strict schema with an optional prop (Gap D)** — confirm the 400 that OpenRouter's lenient providers
   masked. Scenario `strict` + `gpt-4.1-mini` on a funded OpenAI key.

### Harness

Built and compiling at `scratchpad/genai-spike/` (pins `genai = "=0.6.0-beta.19"`; `cargo build` is green).
`src/main.rs` wires all three cloud providers through genai's **native** adapters — `Client::default()` picks the
adapter and the API-key env var from the model name, so there is nothing per-provider to configure — **plus a local /
OpenAI-compatible mode** (`LLAMA_BASE_URL`) that forces the OpenAI chat-completions adapter onto any base URL with empty
auth (Cmdr's `AiBackend::local` shape: trailing-slash normalized, `openai::` namespace so an unknown model name doesn't
fall back to Ollama). The `sequential` scenario replays the **full** assistant turn (text + reasoning content + thought
signatures + tool calls) before each tool answer, so it exercises the round-trip exactly as a real `AgentLlm` loop
would. `REASON=1` turns reasoning on (effort High + `capture_reasoning_content`) to force the round-trip path.

**Already run (zero-cost, `[LL]`) — the local llama-server, all four scenarios pass:**

```
# Cmdr's local llama-server (Ministral 3B) — no key, no spend
LLAMA_BASE_URL=http://127.0.0.1:18437/v1/ ./target/debug/spike sequential ministral-3b-instruct-q4km.gguf
LLAMA_BASE_URL=http://127.0.0.1:18437/v1/ ./target/debug/spike parallel   ministral-3b-instruct-q4km.gguf
LLAMA_BASE_URL=http://127.0.0.1:18437/v1/ ./target/debug/spike stream     ministral-3b-instruct-q4km.gguf
LLAMA_BASE_URL=http://127.0.0.1:18437/v1/ ./target/debug/spike strict     ministral-3b-instruct-q4km.gguf
```

One command per cloud provider (each needs a funded key in env; run all four scenarios per provider):

```
# OpenAI chat-completions (standard tool calling)
OPENAI_API_KEY=…    ./target/debug/spike sequential gpt-4.1-mini
OPENAI_API_KEY=…    ./target/debug/spike parallel   gpt-4.1-mini
OPENAI_API_KEY=…    ./target/debug/spike stream     gpt-4.1-mini
OPENAI_API_KEY=…    ./target/debug/spike strict     gpt-4.1-mini    # Gap D

# OpenAI Responses API + reasoning (genai auto-routes gpt-5*)
OPENAI_API_KEY=…    REASON=1 ./target/debug/spike sequential gpt-5-mini   # Gap B

# Anthropic + extended thinking
ANTHROPIC_API_KEY=… REASON=1 ./target/debug/spike sequential claude-sonnet-4-5   # Gap A
ANTHROPIC_API_KEY=… ./target/debug/spike parallel claude-sonnet-4-5              # thinking off — should pass

# Gemini + thoughtSignature
GEMINI_API_KEY=…    REASON=1 ./target/debug/spike sequential gemini-2.5-flash
GEMINI_API_KEY=…    ./target/debug/spike parallel gemini-2.5-flash               # Gap C
```

Verify current model ids from each provider's models endpoint at run time (never from training data). `OPENROUTER=1`
routes via OpenRouter's OpenAI-compatible endpoint (chat-completions adapter only) if that's ever useful for isolating
an OpenAI-cc finding.

---

## 6. API spend

**$0.00.** The one set of live runs that succeeded — all four scenarios against Cmdr's **local** llama-server — cost
nothing (on-device). OpenAI: every call `429 insufficient_quota` (unbilled). OpenRouter: free-tier `:free` models only,
usage stayed `$0` (`limit_remaining=$10`
untouched). The source audit carries the cloud verdicts, and the harness is staged for a one-command-per-provider cloud
pass once keys are refreshed.
