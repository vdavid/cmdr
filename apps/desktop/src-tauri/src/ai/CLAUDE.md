# AI subsystem

AI features powered by local LLM (llama-server) or OpenAI-compatible APIs. Currently used for folder name suggestions.

Three provider modes:
- **Off**: No AI features.
- **OpenAI-compatible** (BYOK): Any OpenAI-compatible API. Works on any hardware.
- **Local LLM**: On-device llama-server. Requires Apple Silicon (aarch64).

## Key files

| File | Purpose |
|---|---|
| `mod.rs` | Types (`AiStatus`, `AiState`, `DownloadProgress`, `ModelInfo`), model registry (`AVAILABLE_MODELS`, `DEFAULT_MODEL_ID`), `is_local_ai_supported()` gate |
| `manager.rs` | Central coordinator. Global `Mutex<Option<ManagerState>>` singleton. Most Tauri commands live here. Stores provider + OpenAI config in `ManagerState`. |
| `download.rs` | HTTP streaming download with Range-based resume. Emits `ai-download-progress` events (200ms throttle). Cooperative cancellation via function parameter (`Fn() -> bool`). |
| `extract.rs` | Copies bundled `llama-server` binary + dylibs from `resources/ai/` to the AI data dir. Sets Unix permissions, handles symlinks. |
| `process.rs` | Spawns child process with `DYLD_LIBRARY_PATH` set. SIGTERM -> 5s wait -> SIGKILL. Port discovery via `bind(:0)`. Takes `ctx_size` param. |
| `client.rs` | reqwest client with `AiBackend` enum: `Local { port }` or `OpenAi { api_key, base_url, model }`. Routes requests accordingly. |
| `suggestions.rs` | Builds few-shot prompt from listing cache, routes to configured backend, sanitizes response. |

### Tauri commands

Core: `get_ai_status`, `get_ai_model_info`, `get_ai_runtime_status`, `configure_ai`, `start_ai_server`, `stop_ai_server`, `check_ai_connection`, `start_ai_download`, `cancel_ai_download`, `get_folder_suggestions`.
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
            -> start_server_inner(ctx_size)
            -> emit 'ai-server-ready' when healthy
```

## Provider routing in suggestions

`get_folder_suggestions` reads `provider` from `ManagerState`:
- `off` -> returns empty
- `local` -> uses local llama-server (if running)
- `openai-compatible` -> builds `AiBackend::OpenAi` from stored config, calls `chat_completion`

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

## Gotchas

**Gotcha**: `tauri::async_runtime::spawn` is used in `configure_ai` and `start_ai_server` instead of `tokio::spawn`.
**Why**: These may run during Tauri setup before the tokio runtime is fully available. `tauri::async_runtime::spawn` uses Tauri's own runtime which is always ready at that point.

**Gotcha**: `get_folder_suggestions` returns `Ok(Vec::new())` on AI errors, not `Err`.
**Why**: AI suggestions are a nice-to-have enhancement. Returning empty gracefully hides the failure.

**Gotcha**: `configure_ai` must NOT block. Server start is spawned in background via `tauri::async_runtime::spawn`.
**Why**: `start_server_inner` takes 5-60s for health check polling. Blocking would freeze the frontend on startup.

## Dependencies

External: `reqwest`, `tokio`, `libc`, `futures_util`
Internal: `crate::ignore_poison::IgnorePoison`, `crate::file_system::get_file_at`
