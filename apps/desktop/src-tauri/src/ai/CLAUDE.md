# AI subsystem

AI features powered by local LLM (llama-server) or remote LLM providers. Currently used for folder name suggestions and natural-language search.

Three provider modes:
- **Off**: No AI features.
- **Cloud AI** (BYOK): OpenAI / Anthropic / Gemini / xAI / Groq / DeepSeek / OpenRouter / any OpenAI-compatible endpoint. Adapter is picked from the model name. Works on any hardware. Persisted as `ai.provider = "cloud"`; the Rust constructor is `AiBackend::remote(...)` because the same code path handles native Anthropic/Gemini protocols too.
- **Local LLM**: On-device llama-server. Requires Apple Silicon (aarch64).

## Key files

| File | Purpose |
|---|---|
| `mod.rs` | Types (`AiStatus`, `AiState`, `DownloadProgress`, `ModelInfo`), model registry (`AVAILABLE_MODELS`, `DEFAULT_MODEL_ID`), `is_local_ai_supported()` gate |
| `manager.rs` | Central coordinator. Global `Mutex<Option<ManagerState>>` singleton. Most Tauri commands live here. Stores provider + cloud-AI config (`cloud_api_key`/`cloud_base_url`/`cloud_model`). Exposes `resolve_backend() -> BackendResolution` so callers don't reinvent provider routing. Also owns the `STREAM_CANCEL_TOKENS` registry (`register_stream`/`unregister_stream`/`cancel_stream`) for in-flight `stream_folder_suggestions` cancellation. |
| `download.rs` | HTTP streaming download with Range-based resume. Emits `ai-download-progress` events (200ms throttle). Cooperative cancellation via function parameter (`Fn() -> bool`). |
| `extract.rs` | Copies bundled `llama-server` binary + dylibs from `resources/ai/` to the AI data dir. Sets Unix permissions, handles symlinks. |
| `process.rs` | Spawns child process with `DYLD_LIBRARY_PATH` set. Instant SIGKILL to stop (llama-server is stateless; macOS reclaims all GPU/mmap resources). `kill_process` for fire-and-forget (quit, orphans), `kill_and_reap_in_background` for normal operation (reaps zombie in bg thread). `kill_stale_llama_servers` for belt-and-suspenders orphan cleanup by process name. Port discovery via `bind(:0)`. |
| `client.rs` | `genai`-backed chat client. `AiBackend` is a struct bundling a long-lived `genai::Client` with a model name; built via `AiBackend::local(port)` or `AiBackend::remote(api_key, base_url, model)`. The model name picks the adapter (`claude-*` → Anthropic native, `gemini-*` → Gemini native, `gpt-5*`/`*-pro`/`*-codex` → OpenAI Responses API, etc.). Auto-omits `temperature`/`top_p` for OpenAI Responses adapter and for chat-completions reasoning models (`o1*`, `o3*`, `o4*`, `chatgpt-*`, `gpt-5*` defense-in-depth) and substitutes `ReasoningEffort::Low`. Local backend forces the OpenAI adapter via a `ServiceTargetResolver` pinning endpoint to `http://127.0.0.1:<port>/v1/`. Exposes both `chat_completion` (full response) and `chat_completion_stream` (returns a `BoxStream<Result<String, AiError>>` of content chunks; reasoning/thought-signature/tool-call chunks filtered out). |
| `client_integration_test.rs` | `wiremock`-based tests covering request shape per adapter (chat completions vs Responses API), parsing, error mapping. Always run in CI. |
| `client_streaming_test.rs` | `axum`-based SSE mock server tests for `chat_completion_stream`: chunks arrive in order, empty streams end cleanly, drop-mid-stream closes the connection, HTTP 5xx maps to `ServerError`. Always run in CI. (Wiremock can't chunk-deliver SSE bodies — see Gotchas.) |
| `client_real_openai_test.rs` | `#[ignore]`-gated smoke tests against `api.openai.com`, including streaming variants for `gpt-4o-mini`, `gpt-5-mini`, `o3-mini`. Run with `OPENAI_API_KEY=$(security find-generic-password -a "$USER" -s "OPENAI_API_KEY" -w) cargo nextest run --lib --run-ignored only ai::client_real_openai_test`. Costs ~$0.001 per full run. |
| `client_real_anthropic_test.rs` | `#[ignore]`-gated smoke tests against `api.anthropic.com` (chat + streaming variants of `claude-3-5-haiku-latest`). Anthropic's native streaming protocol differs from OpenAI's SSE shape; without this we'd only test the OpenAI lineage. Run with `ANTHROPIC_API_KEY=$(security find-generic-password -a "$USER" -s "ANTHROPIC_API_KEY" -w) cargo nextest run --lib --run-ignored only ai::client_real_anthropic_test`. |
| `suggestions.rs` | Builds few-shot prompt from listing cache, routes to configured backend, sanitizes response. Also exposes `stream_folder_suggestions` + `cancel_folder_suggestions` Tauri commands and a `StreamingSanitizer` that runs the per-line sanitizer on streamed chunks (line-buffers across chunk boundaries, dedupes case-insensitively against existing names + already-emitted, caps at `MAX_SUGGESTIONS`). |
| `suggestions_streaming_test.rs` | Tests for the `manager::register_stream`/`unregister_stream`/`cancel_stream` registry — concurrent ids don't interfere, double-cancel is idempotent, missing id is a no-op. |

### Tauri commands

Core: `get_ai_status`, `get_ai_model_info`, `get_ai_runtime_status`, `configure_ai`, `start_ai_server`, `stop_ai_server`, `check_ai_connection`, `start_ai_download`, `cancel_ai_download`, `get_folder_suggestions`, `stream_folder_suggestions`, `cancel_folder_suggestions`. Note: `get_system_memory_info` moved to top-level `system_memory.rs`.
Legacy (still wired, used by toast): `uninstall_ai`, `dismiss_ai_offer`, `opt_out_ai`, `opt_in_ai`, `is_ai_opted_out`.

## Startup flow

```
Tauri setup()
  -> ai::manager::init()           <- sets up dirs, cleans stale PIDs. Does NOT start server.

Frontend loads
  -> initializeSettings()           <- loads settings from tauri-plugin-store
  -> configureAi({                  <- pushes AI config to backend
       provider, contextSize,
       openaiApiKey, openaiBaseUrl, openaiModel
     })
       -> backend: if provider === 'local' && model installed && local AI supported
            -> spawn_and_track_server() (sync, inside lock — PID tracked immediately)
            -> wait_for_server_health() (async — polls up to 60s)
            -> emit 'ai-server-ready' when healthy
```

## Provider routing

Centralized in `manager::resolve_backend() -> BackendResolution`:
- `Off`: provider is `"off"`.
- `NotConfigured(reason)`: provider is set but missing config (local server not running, cloud key blank).
- `Ready(AiBackend)`: backend ready to call `chat_completion` on.
- `UnknownProvider(name)`: provider value isn't recognized.

Callers decide what to do per case. `suggestions.rs` returns empty on any non-Ready (folder suggestions are nice-to-have). `commands/search.rs::translate_search_query` returns the human-readable reason as an error so the UI can toast it.

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
- `opted_out` field in `AiState` is legacy. `ai.provider` in frontend settings store is the source of truth.
- OpenAI config (api_key, base_url, model) stored in `ManagerState` so suggestions.rs can read without settings files.
- `configure_ai` is idempotent -- frontend calls it on startup and whenever any AI setting changes.
- `ModelInfo` includes `kv_bytes_per_token` and `base_overhead_bytes` for frontend memory estimation.

## Adding a new model

1. Find the GGUF on HuggingFace.
2. Get exact file size: `curl -sIL "<url>" | grep -i content-length`
3. Add entry to `AVAILABLE_MODELS` in `mod.rs` (including `kv_bytes_per_token` and `base_overhead_bytes`).
4. Update `DEFAULT_MODEL_ID` if it should be the new default.

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
privacy-focused users. The architecture doesn't fight this switch — it's just a default value change.

**Decision**: Use `genai` crate as the chat client instead of hand-rolled `reqwest` JSON.
**Why**: We hit two production bugs that were per-provider quirks: (1) GPT-5/o-series chat models reject any non-default `temperature` (HTTP 400), and (2) `gpt-*-pro` / `*-codex` models only respond on `/v1/responses`, not `/v1/chat/completions` (HTTP 404). Each new model adds another quirk. `genai` normalizes ~20 providers, auto-routes Responses-API models, and gives us Anthropic / Gemini / xAI / OpenRouter for free with the same code path. Tradeoff: pinned at `0.5.3` (stable, ~3 months old) with a solo maintainer; mitigated by it being MIT/Apache-2.0 + small enough to fork if needed.

**Decision**: Streaming uses `tauri::ipc::Channel<T>` per call, not the global `app.emit` pattern that downloads use.
**Why**: User can open the new-folder dialog, cancel, and reopen quickly. Two streams could overlap if we used a global event — listeners from the second open would see chunks from the first. Channel scopes the events to a single command invocation, eliminating the race. Tauri 2 docs explicitly recommend `Channel<T>` for streaming events from a command.

**Decision**: Streaming command `stream_folder_suggestions` always returns `Ok(())`; all signaling (suggestions, completion, cancellation, failure) goes through `Channel<SuggestionStreamEvent>`.
**Why**: Mixing IPC `Result<_, String>` with channel events would split the error contract. One signaling path is simpler for both Rust and TypeScript callers. `#[tauri::command]` requires the `Result` return type purely for syntactic reasons here.

**Decision**: Line-buffering and sanitization happen in Rust (`StreamingSanitizer`), not in the frontend.
**Why**: AGENTS.md principle "smart backend, thin frontend." Sanitization rules (markdown stripping, numbering detection, dedupe by case-insensitive existing-names + emit-history) are non-trivial; replicating them in TypeScript would create two authorities that drift. Frontend just renders strings.

**Decision**: Cancellation via explicit `cancel_folder_suggestions` command + `tokio_util::sync::CancellationToken`, not implicit drop detection on the Channel.
**Why**: Tauri 2's `Channel::send` is fire-and-forget into the IPC queue. It does NOT report frontend handler GC or webview destruction back to the backend. Without an explicit cancel signal, the backend would keep streaming after the user closes the dialog — billing cloud providers and pegging local-LLM compute. `CancellationToken::cancel` is itself idempotent, so the same token can be canceled by an explicit cancel call AND by an implicit `Channel::send` failure in the same tick — both succeed.

**Decision**: Cancel-token registry (`STREAM_CANCEL_TOKENS`) is a separate `LazyLock<Mutex<HashMap>>` in `manager.rs`, not part of `ManagerState`.
**Why**: Streaming task lifecycle is orthogonal to file-manager AI state. Keeping it isolated lets us drop entries on task end without holding the wider `MANAGER` lock and without inflating `ManagerState`.

## Gotchas

**Gotcha**: `genai` requires `base_url` to end with `/`. Without the trailing slash, `Url::join("chat/completions")` strips the last segment and you'd hit `https://api.openai.com/chat/completions` (404) instead of `/v1/chat/completions`. `client.rs::build_client` normalizes by appending `/` if missing.

**Gotcha**: `genai 0.6` auto-routes `gpt-5*`, `*-codex`, `*-pro` to the Responses API, but `o1*`/`o3*`/`o4*`/`chatgpt-*` stay on Chat Completions even though they also reject custom `temperature`. We layer `is_openai_chat_reasoning_model()` on top to strip `temperature`/`top_p` and substitute `ReasoningEffort::Low` for those. The heuristic also matches `gpt-5*` as defense-in-depth in case `genai`'s routing rule changes.

**Gotcha**: For reasoning models, `max_tokens` (`max_output_tokens` on Responses API) covers reasoning + visible answer combined. Real-world finding: at `ReasoningEffort::Low`, `gpt-5-mini` consumed all 40 tokens thinking and emitted no `output_text` — `first_text()` returned `None`. `suggestions.rs` (`max_tokens=150`) and `commands/search.rs` (`max_tokens=200`) may occasionally produce empty results when the user picks a reasoning model. Bump to `max_tokens >= 300` if empty-result rate becomes a problem; the empty-result graceful degradation already covers it functionally.

**Gotcha**: `tauri::async_runtime::spawn` is used in `configure_ai` and `start_ai_server` instead of `tokio::spawn`.
**Why**: These may run during Tauri setup before the tokio runtime is fully available. `tauri::async_runtime::spawn` uses Tauri's own runtime which is always ready at that point.

**Gotcha**: `get_folder_suggestions` returns `Ok(Vec::new())` on AI errors, not `Err`.
**Why**: AI suggestions are a nice-to-have enhancement. Returning empty gracefully hides the failure.

**Gotcha**: `configure_ai` must NOT block. Only the health check runs async via `tauri::async_runtime::spawn`.
**Why**: Health check polling takes 5-60s. Blocking would freeze the frontend on startup.

**Gotcha**: The process spawn and `child_pid` assignment must happen synchronously inside the MANAGER lock.
**Why**: Previously, spawn happened inside an async task, creating a race window where a process existed but wasn't tracked in `child_pid`. Rapid provider switching (Local → OpenAI → Local → OpenAI) could orphan processes that survived app quit. Fixed by splitting into `spawn_and_track_server` (sync, inside lock) + `wait_for_server_health` (async).

**Gotcha**: `wait_for_server_health` kills the process on timeout or early death — don't remove that cleanup.
**Why**: Without it, a process that fails health check would be orphaned (PID tracked but never cleaned up until explicit stop).

**Gotcha**: `Channel::send` returns `Err` only when the webview itself is gone (window closed); it succeeds silently after the JS-side handler is GC'd. Don't rely on send failure for liveness — use the explicit `cancel_folder_suggestions` command. Send-error in the streaming-suggestion `try_emit` callback triggers the cancel token as defense-in-depth implicit cancel.

**Gotcha**: Cancel via `tokio::select!` drops the in-flight `stream.next()` future. For `genai`'s reqwest-backed SSE this is the desired terminal action — closes the connection, cuts billing. Single-poll cancel-safety is the only model we rely on; we never resume a previously-canceled stream.

**Gotcha**: `wiremock` does not chunk-deliver SSE bodies in distinct frames; it writes the whole body in one HTTP response. That gives false confidence we'd be exercising multi-chunk parse paths. `client_streaming_test.rs` uses an `axum`-based mock SSE server with `tokio::time::sleep` between frames instead.

## Dependencies

External: `genai` (chat normalization), `reqwest` (download streaming + `health_check`), `tokio`, `tokio-util` (`CancellationToken`), `libc`, `futures_util`
Dev: `wiremock` (HTTP mock for `client_integration_test.rs`); `axum` is used in test-only mode for `client_streaming_test.rs`'s SSE mock.
Internal: `crate::ignore_poison::IgnorePoison`, `crate::file_system::get_file_at`
