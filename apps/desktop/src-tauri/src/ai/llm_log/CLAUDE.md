# LLM call logging (`ai/llm_log/`)

Local, on-disk log of every LLM request and response, so "what did we send, was it set up to succeed, and what came
back?" is answerable for both Ask Cmdr and the legacy one-shot AI features. Depth (fidelity investigation, file layout,
metadata schema, privacy, setting wiring): `DETAILS.md`.

## Module map

- `mod.rs`: the logger — `JobKind`/`LlmLogContext`, `log_request` → `CallLog::log_response`, the enabled + dir globals,
  the pure counter/slug/file-name helpers, and the detached-thread writer.
- `redact.rs`: defense-in-depth secret scrubbing (`redact_secrets`), run over every file before writing.

## Must-knows

- **The tap is at the `AiBackend`/genai boundary in `../client.rs`, not here.** `client.rs` calls
  `log_request`/`log_response` around each `exec_chat*`. A backend logs only when a caller attached a context via
  `AiBackend::with_log_context` AND the `logLlmCalls` setting is on. Every production caller (the agent per
  conversation, folder-suggestions, the two translate commands) attaches one; adding a new LLM call means attaching a
  `JobKind` too, or it silently won't log.
- **The `FakeAgentLlm` logs nothing** — it never touches the genai seam, so a fake-driven turn writes no files. Dev
  debugging uses a real provider (local llama-server or cloud), which does log. Don't move the tap into the runtime to
  "fix" this; the genai boundary is the point.
- **Never break or delay the LLM call.** `log_request`/`log_response` assign the counter + slug synchronously (so `NNN`
  reflects call order) and offload the file write to a detached thread; any write error is swallowed with one
  `log::warn!`. Keep it that way — a full disk must not surface to the user.
- **No API key ever reaches disk.** The logged request body is the genai `ChatRequest` (system + messages + tools), and
  auth is applied downstream of it by genai's web client, so the body carries no secret. Every file still passes through
  `redact::redact_secrets` as a belt — don't remove it, and don't start logging headers/URLs without keeping it.
- **Request fidelity is `request_struct`, not wire.** The byte-identical per-adapter wire payload sits behind genai's
  `to_web_request_data`, whose types are `pub(crate)` at the pinned `genai =0.6.0-beta.19` and unreachable from Cmdr
  (see DETAILS). The `ChatRequest` serialization still carries the full assembled prompt (system, tools, history,
  envelope). Reaching true wire needs a genai patch — a named follow-up, not a v1 gap.
- **Enabled + dir are process globals.** `ENABLED` defaults to `cfg!(debug_assertions)` (dev-on, prod-off); the
  frontend `logLlmCalls` setting pushes the user's choice via `set_log_llm_calls`. `init(app_data_dir)` (called once at
  setup) records where. Logging no-ops until the dir is set.

Depth: `DETAILS.md`.
