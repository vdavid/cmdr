# AI subsystem details

Pull-tier docs for `src-tauri/src/ai/`: architecture, flows, and decision rationale. Must-know invariants and gotchas
live in [CLAUDE.md](CLAUDE.md).

AI features powered by local LLM (llama-server) or remote LLM providers. Currently used for folder name suggestions and
natural-language search.

Frontend counterpart: [`apps/desktop/src/lib/ai/CLAUDE.md`](../../../src/lib/ai/CLAUDE.md) for the AI status surface,
model picker, BYOK key management, and the inline suggestion UI.

Three provider modes:
- **Off**: No AI features.
- **Cloud AI** (BYOK): OpenAI / Anthropic / Gemini / xAI / Groq / DeepSeek / OpenRouter / any OpenAI-compatible endpoint. Adapter is picked from the model name. Works on any hardware. Persisted as `ai.provider = "cloud"`; the Rust constructor is `AiBackend::remote(...)` because the same code path handles native Anthropic/Gemini protocols too.
- **Local LLM**: On-device llama-server. Requires Apple Silicon (aarch64).

## Key files

| File | Purpose |
|---|---|
| `mod.rs` | Types (`AiStatus`, `AiState`, `DownloadProgress`, `ModelInfo`), model registry (`AVAILABLE_MODELS`, `DEFAULT_MODEL_ID`), `is_local_ai_supported()` gate |
| `api_keys.rs` | Per-provider cloud API key storage. Delegates to `crate::secrets::store()` (macOS Keychain, Linux Secret Service, or encrypted-file fallback). One entry per provider under key `ai.apiKey.<providerId>`. Exposes `save_ai_api_key` / `get_ai_api_key` / `delete_ai_api_key` / `has_ai_api_key` Tauri commands. Keys are NOT stored in `settings.json`. |
| `manager.rs` | Central coordinator. Global `Mutex<Option<ManagerState>>` singleton. Most Tauri commands live here. Stores provider + cloud-AI config (`cloud_api_key`/`cloud_base_url`/`cloud_model`). Exposes `resolve_backend() -> BackendResolution` so callers don't reinvent provider routing. Also owns the `STREAM_CANCEL_TOKENS` registry (`register_stream`/`unregister_stream`/`cancel_stream`) for in-flight `stream_folder_suggestions` cancellation. |
| `download.rs` | HTTP streaming download with Range-based resume. Emits `ai-download-progress` events (200ms throttle). Cooperative cancellation via function parameter (`Fn() -> bool`). |
| `extract.rs` | Copies bundled `llama-server` binary + dylibs from `resources/ai/` to the AI data dir. Sets Unix permissions, handles symlinks. |
| `process.rs` | Spawns child process with `DYLD_LIBRARY_PATH` set. Instant SIGKILL to stop (llama-server is stateless; macOS reclaims all GPU/mmap resources). `kill_process` for fire-and-forget (quit, orphans), `kill_and_reap_in_background` for normal operation (reaps zombie in bg thread). `kill_stale_llama_servers` for belt-and-suspenders orphan cleanup by process name. Port discovery via `bind(:0)`. |
| `client.rs` | `genai`-backed chat client. `AiBackend` is a struct bundling a long-lived `genai::Client` with a model name; built via `AiBackend::local(port)` or `AiBackend::remote(api_key, base_url, model)`. For `remote`, the model name picks the adapter via the pure `remote_model_iden`: `claude-*` → Anthropic native, `gemini-*` → Gemini native, `gpt-*`/`o1*`/`o3*`/`o4*`/`chatgpt-*` → OpenAI (with `genai`'s `gpt-5*`/`*-codex`/`*-pro` → Responses-API auto-routing), and EVERYTHING ELSE is forced onto the OpenAI chat-completions adapter via the `openai::` namespace. That last rule is load-bearing: `genai` falls back to its **Ollama** adapter for unrecognized model names, so a bare `llama-3.1-8b-instant` (Groq), `deepseek-chat`, or `google/gemma-…:free` (OpenRouter) would POST to Ollama's `/api/chat` against an OpenAI endpoint and 404 — every BYOK provider except Anthropic/Gemini speaks OpenAI chat-completions. Auto-omits `temperature`/`top_p` for the OpenAI Responses adapter and for chat-completions reasoning models (`o1*`, `o3*`, `o4*`, `chatgpt-*`, `gpt-5*` defense-in-depth) and substitutes `ReasoningEffort::Low`. Local backend forces the OpenAI adapter via a `ServiceTargetResolver` pinning endpoint to `http://127.0.0.1:<port>/v1/`. Exposes `chat_completion` (full response), `chat_completion_with_empty_retry` (retries once with 4× the token budget on `EmptyResponse` — the translate commands use this), and `chat_completion_stream` (returns a `BoxStream<Result<String, AiError>>` of content chunks; reasoning/thought-signature/tool-call chunks filtered out). `AiError` is typed by HTTP status via the pure `ai_error_for_status` (401/403 → `AuthFailed`, 429 → `RateLimited`, else `ServerError`); a `None` `first_text()` → `EmptyResponse`. |
| `client_real_groq_test.rs` | `#[ignore]`-gated real-API smoke against Groq (OpenAI-compatible, free tier) through `AiBackend::remote` + `chat_completion_with_empty_retry`. The cheap always-available real-provider gate — catches adapter-routing / auth / parse regressions the wiremock tests can't (it's what caught the Ollama-fallback bug above). The `groq-smoke` check (Go runner) resolves `GROQ_API_KEY` from env or the macOS Keychain and runs it with `--run-ignored only`, self-skipping when no key. CI: `slow-checks.yml` passes the `GROQ_API_KEY` secret. |
| `translate_error.rs` | `AiTranslateError { kind, message }` + `AiTranslateErrorKind` enum, the typed error the two translate IPC commands return so the frontend branches on `kind` (not the message string). `From<AiError>` maps transport variants; the commands map `BackendResolution` non-ready cases. Mirror enum: `lib/ai/translate-error-toast.ts`. |
| `client_integration_test.rs` | `wiremock`-based tests covering request shape per adapter (chat completions vs Responses API), parsing, error mapping. Always run in CI. |
| `client_streaming_test.rs` | `axum`-based SSE mock server tests for `chat_completion_stream`: chunks arrive in order, empty streams end cleanly, drop-mid-stream closes the connection, HTTP 5xx maps to `ServerError`. Always run in CI. (Wiremock can't chunk-deliver SSE bodies. See Gotchas.) |
| `client_real_openai_test.rs` | `#[ignore]`-gated smoke tests against `api.openai.com`, including streaming variants for `gpt-4o-mini`, `gpt-5-mini`, `o3-mini`. Run with `OPENAI_API_KEY=$(security find-generic-password -a "$USER" -s "OPENAI_API_KEY" -w) cargo nextest run --lib --run-ignored only ai::client_real_openai_test`. Costs ~$0.001 per full run. |
| `client_real_anthropic_test.rs` | `#[ignore]`-gated smoke tests against `api.anthropic.com` (chat + streaming variants of `claude-3-5-haiku-latest`). Anthropic's native streaming protocol differs from OpenAI's SSE shape; without this we'd only test the OpenAI lineage. Run with `ANTHROPIC_API_KEY=$(security find-generic-password -a "$USER" -s "ANTHROPIC_API_KEY" -w) cargo nextest run --lib --run-ignored only ai::client_real_anthropic_test`. |
| `suggestions.rs` | Builds few-shot prompt from listing cache, routes to configured backend, sanitizes response. Also exposes `stream_folder_suggestions` + `cancel_folder_suggestions` Tauri commands and a `StreamingSanitizer` that runs the per-line sanitizer on streamed chunks (line-buffers across chunk boundaries, dedupes case-insensitively against existing names + already-emitted, caps at `MAX_SUGGESTIONS`). |
| `suggestions_streaming_test.rs` | Tests for the `manager::register_stream`/`unregister_stream`/`cancel_stream` registry: concurrent ids don't interfere, double-cancel is idempotent, missing id is a no-op. |

### Tauri commands

Core: `get_ai_status`, `get_ai_model_info`, `get_ai_runtime_status`, `configure_ai`, `start_ai_server`, `stop_ai_server`, `check_ai_connection`, `start_ai_download`, `cancel_ai_download`, `get_folder_suggestions`, `stream_folder_suggestions`, `cancel_folder_suggestions`. Note: `get_system_memory_info` moved to top-level `system_memory.rs`.
API keys: `save_ai_api_key`, `get_ai_api_key`, `delete_ai_api_key`, `has_ai_api_key` (in `api_keys.rs`).
Also: `uninstall_ai` (the Uninstall button in `AiLocalSection.svelte`). The dead opt-out machinery (`opt_in_ai`, `is_ai_opted_out`, `dismiss_ai_offer`, `opt_out_ai`, and the `AiState.opted_out` field) was removed with the onboarding revamp — `ai.provider` is the single source of truth for whether AI is on.

## Startup flow

```
Tauri setup()
  -> ai::manager::init()           <- sets up dirs, cleans stale PIDs. Does NOT start server.

Frontend loads
  -> initializeSettings()           <- loads settings from tauri-plugin-store
  -> getAiApiKey(providerId)        <- fetches API key from OS secret store
  -> configureAi({                  <- pushes AI config to backend
       provider, contextSize,
       cloudApiKey, cloudBaseUrl, cloudModel
     })
       -> backend: if provider === 'local' && model installed && local AI supported
            -> spawn_and_track_server() (sync, inside lock, PID tracked immediately)
            -> wait_for_server_health() (async, polls up to 60s)
            -> emit 'ai-server-ready' when healthy
```

## Provider routing

Centralized in `manager::resolve_backend() -> BackendResolution`:
- `Off`: provider is `"off"`.
- `NotConfigured(reason)`: provider is set but missing config (local server not running, cloud key blank).
- `Ready(AiBackend)`: backend ready to call `chat_completion` on.
- `UnknownProvider(name)`: provider value isn't recognized.

Callers decide what to do per case. `suggestions.rs` returns empty on any non-Ready (folder suggestions are nice-to-have). The two translate commands (`commands/search.rs::translate_search_query`, `commands/selection.rs::translate_selection_query`) return a typed `AiTranslateError { kind, message }` (in `translate_error.rs`) so the frontend can branch on `kind` and show a SPECIFIC toast (key rejected vs. out of quota vs. timed out vs. empty answer) without string-matching the message. The `kind` set maps both the `BackendResolution` non-ready cases (`off` / `notConfigured` / `unknownProvider`) and the `AiError` transport variants (`authFailed` / `rateLimited` / `timeout` / `unavailable` / `emptyResponse` / `serverError` / `parseError`). Frontend counterpart: `lib/ai/translate-error-toast.ts`; keep the two enums in lockstep.

## Download/install event sequence

`do_download()` emits events for each install step so the frontend can show progress:
1. `ai-extracting` -- binary extraction from bundled archive (usually instant)
2. `ai-download-progress` (repeated) -- model download with bytes/total/speed/eta
3. `ai-verifying` -- file size verification after download completes
4. `ai-installing` -- server startup begins (health check polling)
5. `ai-install-complete` -- server is healthy and ready

The frontend (`AiSection.svelte`) tracks `installStep` state and displays "Step N of 4" labels.

## Key patterns

- Two install flags: `AiState.installed` AND `AiState.model_download_complete` -- both must be true.
- State persisted to `ai-state.json` in the app data dir (`~/Library/Application Support/.../ai/`).
- Stale PIDs from previous sessions are stopped on startup (alive -> SIGTERM/SIGKILL, dead -> state cleared).
- Stale partial downloads (>24 hours) cleaned up at startup.
- Binary re-extraction is possible if model exists but binary is missing.
- Download guard: `download_in_progress` flag prevents concurrent downloads.
- Server logs written to `llama-server.log` in the AI dir for debugging.
- OpenAI config (api_key, base_url, model) stored in `ManagerState` so suggestions.rs can read without settings files. The api_key originates from the OS secret store (`api_keys.rs`), pushed in via `configure_ai`.
- `configure_ai` is idempotent -- frontend calls it on startup and whenever any AI setting changes.
- `ModelInfo` includes `kv_bytes_per_token` and `base_overhead_bytes` for frontend memory estimation.

## Key decisions

**Decision**: Global `Mutex<Option<ManagerState>>` singleton instead of Tauri managed state.
**Why**: AI state needs to be accessed from both Tauri commands and internal init/shutdown paths. Tauri managed state requires an `AppHandle` to access, but `shutdown()` is called from the quit handler where threading constraints make it simpler to use a plain global. The `Option` allows lazy init -- `None` until `init()` runs.

**Decision**: Two separate install flags (`installed` + `model_download_complete`) rather than a single boolean.
**Why**: The download can be interrupted (crash, cancel, network loss). A partial 2 GB file on disk looks "installed" but is corrupt. `model_download_complete` is only set after file-size verification passes. This prevents launching llama-server with a truncated model, which would crash silently or produce garbage.

**Decision**: Frontend pushes AI config to backend via `configure_ai` -- Rust never reads settings files.
**Why**: The frontend is the single source of truth for settings via `tauri-plugin-store`. Having Rust also read `settings.json` directly would create a second reader with potential format/timing mismatches.

**Decision**: `init()` only sets up directories and cleans stale PIDs. Server start is deferred to `configure_ai`.
**Why**: The frontend needs to load settings before the backend knows which provider to use. The ~500ms delay is negligible.

**Decision**: Port discovery via `bind(:0)` then pass to llama-server, instead of letting llama-server pick its own port.
**Why**: llama-server doesn't have a reliable way to report its chosen port back to the parent.

**Decision**: Bundle pre-extracted individual binaries in `resources/ai/` instead of a `.tar.gz` archive.
**Why**: Apple notarization inspects inside archives and rejects unsigned binaries.

**Decision**: Suggestion sanitization strips bullets, markdown, numbering, and deduplicates case-insensitively.
**Why**: Small LLMs (3B params) inconsistently follow formatting instructions.

**Decision**: Default provider is `local` (temporary). **Why**: Matches the current onboarding toast flow. Long-term
direction is to make OpenAI-compatible the primary recommended path, with local LLM as the secondary option for
privacy-focused users. The architecture doesn't fight this switch: it's just a default value change.

**Decision**: Use `genai` crate as the chat client instead of hand-rolled `reqwest` JSON.
**Why**: We hit two production bugs that were per-provider quirks: (1) GPT-5/o-series chat models reject any non-default `temperature` (HTTP 400), and (2) `gpt-*-pro` / `*-codex` models only respond on `/v1/responses`, not `/v1/chat/completions` (HTTP 404). Each new model adds another quirk. `genai` normalizes ~20 providers, auto-routes Responses-API models, and gives us Anthropic / Gemini / xAI / OpenRouter for free with the same code path. Tradeoff: pinned at `0.5.3` (stable, ~3 months old) with a solo maintainer; mitigated by it being MIT/Apache-2.0 + small enough to fork if needed.

**Decision**: Streaming uses `tauri::ipc::Channel<T>` per call, not the global `app.emit` pattern that downloads use.
**Why**: User can open the new-folder dialog, cancel, and reopen quickly. Two streams could overlap if we used a global event: listeners from the second open would see chunks from the first. Channel scopes the events to a single command invocation, eliminating the race. Tauri 2 docs explicitly recommend `Channel<T>` for streaming events from a command.

**Decision**: Streaming command `stream_folder_suggestions` always returns `Ok(())`; all signaling (suggestions, completion, cancellation, failure) goes through `Channel<SuggestionStreamEvent>`.
**Why**: Mixing IPC `Result<_, String>` with channel events would split the error contract. One signaling path is simpler for both Rust and TypeScript callers. `#[tauri::command]` requires the `Result` return type purely for syntactic reasons here.

**Decision**: Line-buffering and sanitization happen in Rust (`StreamingSanitizer`), not in the frontend.
**Why**: AGENTS.md principle "smart backend, thin frontend." Sanitization rules (markdown stripping, numbering detection, dedupe by case-insensitive existing-names + emit-history) are non-trivial; replicating them in TypeScript would create two authorities that drift. Frontend just renders strings.

**Decision**: Cancellation via explicit `cancel_folder_suggestions` command + `tokio_util::sync::CancellationToken`, not implicit drop detection on the Channel.
**Why**: Tauri 2's `Channel::send` is fire-and-forget into the IPC queue. It does NOT report frontend handler GC or webview destruction back to the backend. Without an explicit cancel signal, the backend would keep streaming after the user closes the dialog, billing cloud providers and pegging local-LLM compute. `CancellationToken::cancel` is itself idempotent, so the same token can be canceled by an explicit cancel call AND by an implicit `Channel::send` failure in the same tick. Both succeed.

**Decision**: Cancel-token registry (`STREAM_CANCEL_TOKENS`) is a separate `LazyLock<Mutex<HashMap>>` in `manager.rs`, not part of `ManagerState`.
**Why**: Streaming task lifecycle is orthogonal to file-manager AI state. Keeping it isolated lets us drop entries on task end without holding the wider `MANAGER` lock and without inflating `ManagerState`.

## Gotchas

**Gotcha**: Only **local** AI requires Apple Silicon. Cloud AI (BYOK OpenAI / Anthropic / Gemini / any OpenAI-compatible endpoint) works on Intel Macs too. Don't gate the whole AI subsystem on `is_local_ai_supported()` — gate only the local-specific code paths (`start_ai_server`, `start_ai_download`, and the `Offer` branch of `compute_ai_status` in `manager.rs`). The frontend has its own short-circuit: `ai-state.svelte.ts::initAiState` returns early when `ai.provider === "cloud"` so the install toast never fires for cloud users, regardless of arch. A previous version of `get_ai_status` returned `Offer` on Intel because the default provider is `"local"`; users saw the download toast and only learned their hardware couldn't run it after clicking Download and hitting the `start_ai_download` rejection. Now `compute_ai_status` gates `Offer` on `local_ai_supported`.

**Gotcha**: `genai` requires `base_url` to end with `/`. Without the trailing slash, `Url::join("chat/completions")` strips the last segment and you'd hit `https://api.openai.com/chat/completions` (404) instead of `/v1/chat/completions`. `client.rs::build_client` normalizes by appending `/` if missing.

**Gotcha**: `configure_ai` / `check_ai_connection` reject a `cloud_base_url` whose scheme is plain `http://` when the host is non-loopback AND an API key is set (`validate_ai_base_url` in `manager.rs`). This stops a BYOK key from being POSTed in an `Authorization: Bearer` header over plaintext to an arbitrary host (a real key-exfil path if a user pastes a malicious "free proxy" endpoint). Loopback hosts (`localhost`/`127.0.0.1`/`::1`) keep `http://` so the Ollama / LM Studio presets work, and an empty key is allowed over `http://` (no secret to leak). Don't remove the check or relax it to "warn only" — the rejection is the gate.

**Gotcha**: `genai 0.6` auto-routes `gpt-5*`, `*-codex`, `*-pro` to the Responses API, but `o1*`/`o3*`/`o4*`/`chatgpt-*` stay on Chat Completions even though they also reject custom `temperature`. We layer `is_openai_chat_reasoning_model()` on top to strip `temperature`/`top_p` and substitute `ReasoningEffort::Low` for those. The heuristic also matches `gpt-5*` as defense-in-depth in case `genai`'s routing rule changes.

**Gotcha**: For reasoning models, `max_tokens` (`max_output_tokens` on Responses API) covers reasoning + visible answer combined. Real-world finding: at `ReasoningEffort::Low`, `gpt-5-mini` consumed all 40 tokens thinking and emitted no `output_text`, so `first_text()` returned `None`. Both translate commands now request `max_tokens=300` (search bumped from 200; selection already 300) AND call `chat_completion_with_empty_retry`, which on an `EmptyResponse` retries ONCE with 4× the budget (capped at 2000) — a provider-agnostic guard that reacts to the symptom instead of maintaining a never-complete reasoning-model name list. When the retry is still empty, `chat_completion` surfaces the typed `AiError::EmptyResponse`, which becomes a specific "the AI came back empty, try a faster model" toast. `suggestions.rs` (`max_tokens=150`) stays graceful-empty since folder suggestions are nice-to-have and don't use the retry helper. Picking a non-reasoning model (the default `gpt-4.1-mini`) sidesteps this entirely.

**Gotcha**: `tauri::async_runtime::spawn` is used in `configure_ai` and `start_ai_server` instead of `tokio::spawn`.
**Why**: These may run during Tauri setup before the tokio runtime is fully available. `tauri::async_runtime::spawn` uses Tauri's own runtime which is always ready at that point.

**Gotcha**: `get_folder_suggestions` returns `Ok(Vec::new())` on AI errors, not `Err`.
**Why**: AI suggestions are a nice-to-have enhancement. Returning empty gracefully hides the failure.

**Gotcha**: `configure_ai` must NOT block. Only the health check runs async via `tauri::async_runtime::spawn`.
**Why**: Health check polling takes 5-60s. Blocking would freeze the frontend on startup.

**Gotcha**: The process spawn and `child_pid` assignment must happen synchronously inside the MANAGER lock.
**Why**: Previously, spawn happened inside an async task, creating a race window where a process existed but wasn't tracked in `child_pid`. Rapid provider switching (Local → OpenAI → Local → OpenAI) could orphan processes that survived app quit. Fixed by splitting into `spawn_and_track_server` (sync, inside lock) + `wait_for_server_health` (async).

**Gotcha**: `wait_for_server_health` kills the process on timeout or early death. Don't remove that cleanup.
**Why**: Without it, a process that fails health check would be orphaned (PID tracked but never cleaned up until explicit stop).

**Gotcha**: `Channel::send` returns `Err` only when the webview itself is gone (window closed); it succeeds silently after the JS-side handler is GC'd. Don't rely on send failure for liveness: use the explicit `cancel_folder_suggestions` command. Send-error in the streaming-suggestion `try_emit` callback triggers the cancel token as defense-in-depth implicit cancel.

**Gotcha**: Cancel via `tokio::select!` drops the in-flight `stream.next()` future. For `genai`'s reqwest-backed SSE this is the desired terminal action: closes the connection, cuts billing. Single-poll cancel-safety is the only model we rely on; we never resume a previously-canceled stream.

**Gotcha**: `wiremock` does not chunk-deliver SSE bodies in distinct frames; it writes the whole body in one HTTP response. That gives false confidence we'd be exercising multi-chunk parse paths. `client_streaming_test.rs` uses an `axum`-based mock SSE server with `tokio::time::sleep` between frames instead.

## Dependencies

External: `genai` (chat normalization), `reqwest` (download streaming + `health_check`), `tokio`, `tokio-util` (`CancellationToken`), `libc`, `futures_util`
Dev: `wiremock` (HTTP mock for `client_integration_test.rs`); `axum` is used in test-only mode for `client_streaming_test.rs`'s SSE mock.
Internal: `crate::ignore_poison::IgnorePoison`, `crate::file_system::get_file_at`
