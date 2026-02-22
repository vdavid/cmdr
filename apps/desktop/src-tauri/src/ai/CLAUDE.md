# AI subsystem

Local on-device AI features powered by llama.cpp's `llama-server`. Currently used for folder name suggestions.

AI requires Apple Silicon (aarch64). Intel Macs are not supported — the bundled binary is ARM64-only.

## Key files

| File | Purpose |
|---|---|
| `mod.rs` | Types (`AiStatus`, `AiState`, `DownloadProgress`, `ModelInfo`), model registry (`AVAILABLE_MODELS`, `DEFAULT_MODEL_ID`), gate functions |
| `manager.rs` | Central coordinator. Global `Mutex<Option<ManagerState>>` singleton. Most Tauri commands live here. `get_folder_suggestions` is in `suggestions.rs`. Handles startup recovery. |
| `download.rs` | HTTP streaming download with Range-based resume. Emits `ai-download-progress` events (200ms throttle). Cooperative cancellation via function parameter (`Fn() -> bool`). |
| `extract.rs` | Extracts `llama-server` binary + dylibs from a bundled `.tar.gz`. Sets Unix permissions, handles symlinks. |
| `process.rs` | Spawns child process with `DYLD_LIBRARY_PATH` set. SIGTERM → 5s wait → SIGKILL. Port discovery via `bind(:0)`. |
| `client.rs` | reqwest client: `POST /v1/chat/completions` (15s timeout), `GET /health` (2s timeout). |
| `suggestions.rs` | Builds few-shot prompt from listing cache, calls LLM, sanitizes response (strips bullets/markdown/numbering, rejects `/` and `\0`, deduplicates case-insensitively, enforces 255-char limit). Also hosts `get_folder_suggestions` Tauri command. |

### Additional Tauri commands

Beyond the core start/stop/status flow, the module also exposes: `uninstall_ai`, `dismiss_ai_offer`, `opt_out_ai`, `opt_in_ai`, `is_ai_opted_out`, `get_ai_model_info`.

## Dev gate

`use_real_ai()` returns `false` in debug builds unless `CMDR_REAL_AI=1` is set. In release builds it returns `true` on supported hardware. All Tauri commands check this at entry and return `Unavailable`/empty when false.

## Architecture / data flow

```
Frontend                    manager.rs              process.rs / download.rs / client.rs
   |                           |
   |-- get_ai_status --------> |
   |<- AiStatus ─────────────  |
   |                           |
   |-- start_ai_download ----> |
   |                           |-- extract_bundled_llama_server()
   |<- ai-download-progress    |-- download_file()  (streams, emits events)
   |<- ai-installing           |-- spawn_llama_server()
   |                           |-- poll /health (up to 60s)
   |<- ai-install-complete     |
   |                           |
   |-- get_folder_suggestions  | (suggestions.rs → client.rs → llama-server)
   |<- Vec<String>            |
```

## Key patterns

- Two install flags: `AiState.installed` AND `AiState.model_download_complete` — both must be true.
- State persisted to `ai-state.json` in the app data dir (`~/Library/Application Support/…/ai/`).
- Stale PIDs cleaned up at startup (`is_process_alive` via `kill(pid, 0)`).
- Stale partial downloads (>24 hours) cleaned up at startup.
- Binary re-extraction is possible if model exists but binary is missing.
- Download guard: `download_in_progress` flag prevents concurrent downloads.
- Server logs written to `llama-server.log` in the AI dir for debugging.

## Adding a new model

1. Find the GGUF on HuggingFace.
2. Get exact file size: `curl -sIL "<url>" | grep -i content-length`
3. Add entry to `AVAILABLE_MODELS` in `mod.rs`.
4. Update `DEFAULT_MODEL_ID` if it should be the new default.

## Dependencies

External: `reqwest`, `tokio`, `flate2`, `tar`, `libc`, `futures_util`
Internal: `crate::ignore_poison::IgnorePoison`, `crate::file_system::get_file_at`
