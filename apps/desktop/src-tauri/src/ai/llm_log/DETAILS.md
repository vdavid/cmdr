# LLM call logging details

Pull-tier docs for `ai/llm_log/`. Must-know invariants live in [CLAUDE.md](CLAUDE.md). This is the observability
gateway specified in the Ask Cmdr plan ([`docs/specs/ask-cmdr-plan.md`](../../../../../../docs/specs/ask-cmdr-plan.md)
§ M9).

## The seam: one tap, all traffic

All LLM traffic flows through [`crate::ai::client::AiBackend`](../client.rs) — the agent (via
`agent/llm/genai_impl.rs`'s `exec_chat_stream_request`) and the legacy one-shot helpers (folder suggestions, the two
translate commands, via `chat_completion` / `chat_completion_stream`). The tap lives in those three dispatch functions:
each serializes the outgoing genai `ChatRequest` and calls `log_request` before dispatch, then logs the response after
(non-stream) or on the stream's `End` event (streaming, via `wrap_stream_with_response_log`).

The logging context rides on the backend as `Option<LlmLogContext>`, set by the caller with `with_log_context`. This is
why no dispatch-function signatures changed and no test needed updating: a unit-test backend simply has no context and
logs nothing. The context carries the session directory (agent: `thread-{conversation_id}`; one-shot helpers: the job
name) and the `JobKind` (for the slug prefix and `gen_ai.operation` metadata). The latest user message, for the slug, is
extracted from the `ChatRequest` at log time.

### The fake logs nothing (decision)

`FakeAgentLlm` bypasses genai entirely, so a fake-driven turn produces no log files. The plan left this to the
implementer's call; keeping the tap strictly at the genai boundary keeps the `AgentLlm` seam clean (no logging
concern threaded through the trait) and is honest — dev debugging of real prompts uses a real provider, which logs. The
E2E fake path (`CMDR_E2E_ASK_CMDR_FAKE`) is likewise silent.

## Capture fidelity: why `request_struct`, not wire

The plan's research said genai `=0.6.0-beta.19` "publicly exposes"
`AdapterDispatcher::to_web_request_data(target, service_type, chat_req, options_set) -> WebRequestData`, the same
function `exec_chat`/`exec_chat_stream` call internally, so reproducing it would yield the byte-identical wire payload
with no network call. The function is `pub`, but the types it needs — `AdapterDispatcher`, `WebRequestData`,
`ServiceType`, and `ChatOptionsSet` — are all re-exported at `pub(crate)` in `genai/src/adapter/mod.rs` and
`chat_options.rs`, so they are **unreachable from Cmdr** as an external crate. (`pub fn` inside a `pub(crate)` path is
externally uncallable.) Verified against the vendored source at that exact pin.

So v1 logs the plan's documented fallback: the serialized genai `ChatRequest`, marked `fidelity: "request_struct"`. It
still carries the full assembled prompt — system, tools (with schemas), full message history, and the context envelope
— which answers all three of David's questions. What it lacks is only the per-adapter wire key layout (Anthropic's
top-level `system`, tool-call formatting, `store=false`, etc.).

Responses are logged as the assembled reply: for streams, the captured content when the caller set capture options (the
agent does), else the text accumulated from the chunk stream, plus stop reason and usage
(`fidelity: "assembled"`); for the non-stream helper, the parsed genai `ChatResponse` (`fidelity: "parsed"`).

**Follow-up for true wire fidelity:** a scoped `[patch.crates.io]` genai fork that re-exports those four items as `pub`
(or a thin `pub` wrapper calling `to_web_request_data`) would let the tap log the byte-identical wire `payload`
(`fidelity: "wire"`) with no other change. This is the same class of change as the already-tracked Anthropic
thinking-capture patch (plan § 13 follow-ups) — bundle them if that fork lands.

## File layout

`{app data dir}/llm-logs/{session}/{NNN}_{request|response}_{slug}.json`

- `session`: `thread-{conversation_id}` for the agent; the job name (`folder-suggestions`, `translate-search`,
  `translate-selection`) for the one-shot helpers.
- `NNN`: a three-digit zero-padded per-session counter reflecting call order. A request takes one value, its response
  the next, so a multi-turn tool loop reads `001_request`, `002_response`, `003_request`, `004_response`. The counter is
  seeded from the max existing `NNN_` prefix on first use per process, so numbering continues across restarts instead of
  clobbering.
- `slug`: deterministic (never model-generated) — the job prefix plus up to six sanitized words of the latest user
  message, lowercased, non-alphanumerics collapsed to dashes, length-bounded.

Each file is `{ "metadata": {...}, "body": {...} }`, pretty-printed. `body` is the `ChatRequest` (request) or the
assembled/parsed response.

### Metadata (OpenTelemetry GenAI semconv field names where they fit; no OTel dependency)

Request: `gen_ai.operation.name` (`"chat"`), `gen_ai.system` (provider), `gen_ai.request.model`, `cmdr.adapter_kind`,
`cmdr.job`, `cmdr.session`, `cmdr.seq`, `cmdr.direction` (`"request"`), `cmdr.fidelity`, `cmdr.timestamp` (ISO 8601).
Response adds `gen_ai.usage.input_tokens` / `gen_ai.usage.output_tokens` (when usage returned),
`gen_ai.response.finish_reasons`, and `cmdr.latency_ms`. The `gen_ai.*` names are borrowed so external tooling could
consume the files later; the `cmdr.*` names are ours.

## Failure isolation

`log_request`/`log_response` assign the counter and slug synchronously (fixing `NNN` to call order) and hand the actual
file write to a detached `std::thread`. A write error (disk full, unwritable dir) is swallowed with one `log::warn!`; it
never reaches the caller. A stream dropped before `End` (cancellation) writes a request file but no response file, which
honestly reflects that nothing came back. Covered by the `an_unwritable_root_never_panics_the_caller` test.

## Privacy

These files contain everything the provider saw: file and folder names, paths, sizes, dates, and the app-state
envelope. They are **local only, never transmitted** — the app data dir, under `llm-logs/`. No file contents are ever
included (Ask Cmdr is read-only by construction; there is no content-read tool). No API key is written (see CLAUDE.md).
The consent/settings copy points at this folder.

## The setting

`advanced.logLlmCalls`, a boolean in Settings › Advanced (logging & diagnostics card).

- **Default**: dev-on, prod-off. The Rust global is `AtomicBool::new(cfg!(debug_assertions))`; the frontend registry
  default is `import.meta.env.DEV`. Both inline to the same build-mode boolean, so they agree with or without the
  frontend push. No per-mode default mechanism existed in the settings system before this — `import.meta.env.DEV` as a
  registry `default` is the first.
- **Runtime toggle**: the frontend pushes changes through `settings-applier.ts`'s `passthroughBackendHandlers` →
  `setLogLlmCalls` → the `set_log_llm_calls` Tauri command → `llm_log::set_enabled`. No restart.
- **Startup**: `lib.rs`'s setup calls `llm_log::init(&data_dir)` to record `{data dir}/llm-logs/`. Until then logging
  no-ops (no dir).

## Settings and certification notes

- The settings row's i18n keys: `settings.advanced.logLlmCalls.label` and `.description` (English-only
  today, in `messages/en/settings.json`).
- These log files are the debugging companion for the live provider-certification runs: with the setting on (dev
  default), each certification call's request and response are on disk, so "what did we send, what came back" is
  directly inspectable — useful for the reasoning-off Anthropic loop and the OpenAI-Responses/Gemini checks.
- The privacy line here should stay mirrored in the consent/settings human-facing copy.
