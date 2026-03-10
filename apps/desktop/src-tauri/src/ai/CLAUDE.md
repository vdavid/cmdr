# AI subsystem

Local on-device AI features powered by llama.cpp's `llama-server`. Currently used for folder name suggestions.

AI requires Apple Silicon (aarch64). Intel Macs are not supported — the bundled binary is ARM64-only.

## Key files

| File | Purpose |
|---|---|
| `mod.rs` | Types (`AiStatus`, `AiState`, `DownloadProgress`, `ModelInfo`), model registry (`AVAILABLE_MODELS`, `DEFAULT_MODEL_ID`), gate functions |
| `manager.rs` | Central coordinator. Global `Mutex<Option<ManagerState>>` singleton. Most Tauri commands live here. `get_folder_suggestions` is in `suggestions.rs`. Handles startup recovery. |
| `download.rs` | HTTP streaming download with Range-based resume. Emits `ai-download-progress` events (200ms throttle). Cooperative cancellation via function parameter (`Fn() -> bool`). |
| `extract.rs` | Copies bundled `llama-server` binary + dylibs from `resources/ai/` to the AI data dir. Sets Unix permissions, handles symlinks. |
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
- Stale PIDs from previous sessions are stopped on startup (alive → SIGTERM/SIGKILL, dead → state cleared).
- Stale partial downloads (>24 hours) cleaned up at startup.
- Binary re-extraction is possible if model exists but binary is missing.
- Download guard: `download_in_progress` flag prevents concurrent downloads.
- Server logs written to `llama-server.log` in the AI dir for debugging.

## Adding a new model

1. Find the GGUF on HuggingFace.
2. Get exact file size: `curl -sIL "<url>" | grep -i content-length`
3. Add entry to `AVAILABLE_MODELS` in `mod.rs`.
4. Update `DEFAULT_MODEL_ID` if it should be the new default.

## Key decisions

**Decision**: Global `Mutex<Option<ManagerState>>` singleton instead of Tauri managed state.
**Why**: AI state needs to be accessed from both Tauri commands and internal init/shutdown paths. Tauri managed state requires an `AppHandle` to access, but `shutdown()` is called from the quit handler where threading constraints make it simpler to use a plain global. The `Option` allows lazy init — `None` until `init()` runs.

**Decision**: Two separate install flags (`installed` + `model_download_complete`) rather than a single boolean.
**Why**: The download can be interrupted (crash, cancel, network loss). A partial 2 GB file on disk looks "installed" but is corrupt. `model_download_complete` is only set after file-size verification passes. This prevents launching llama-server with a truncated model, which would crash silently or produce garbage.

**Decision**: Dev gate via `use_real_ai()` that returns `false` in debug builds unless `CMDR_REAL_AI=1`.
**Why**: AI features spawn a child process, download multi-GB files, and consume GPU resources. Enabling this by default in dev would make every `cargo run` slow and resource-heavy. The env var opt-in keeps the dev loop fast while still allowing manual AI testing.

**Decision**: Port discovery via `bind(:0)` then pass to llama-server, instead of letting llama-server pick its own port.
**Why**: llama-server doesn't have a reliable way to report its chosen port back to the parent. Binding port 0, reading the OS-assigned port, closing the listener, then passing it to llama-server avoids the tiny race window while keeping the architecture simple. The 100ms startup delay before the health check loop makes collisions practically impossible.

**Decision**: Cancellation via `Fn() -> bool` parameter rather than `Arc<AtomicBool>`.
**Why**: `download_file` lives in a separate module from the manager's cancel state. Passing a closure (`is_cancel_requested`) decouples the download logic from the global `MANAGER` mutex — the download module doesn't need to know about `ManagerState` at all.

**Decision**: `SIGTERM` then 5s wait then `SIGKILL` for process shutdown.
**Why**: llama-server may be mid-inference holding GPU memory. `SIGTERM` gives it a chance to release resources cleanly. The 5s timeout prevents hanging on app quit if the server is stuck.

**Decision**: `shutdown()` called from both `on_window_event` (CloseRequested/Destroyed) and `RunEvent::Exit`.
**Why**: `on_window_event` handles normal quit, but force-quit/crash/SIGTERM bypass it. `RunEvent::Exit` fires on app-level exit regardless of how it was triggered. `shutdown()` is idempotent (`child_pid.take()` returns `None` on subsequent calls), so double-calling is safe.

**Decision**: Context window (`-c 4096`) explicitly set on llama-server.
**Why**: Without `-c`, llama-server defaults to the model's trained max context (256K for Ministral), creating a ~27 GB KV cache. Folder suggestions need at most 2K context. 4K is generous and keeps memory under ~400 MB.

**Decision**: Bundle pre-extracted individual binaries in `resources/ai/` instead of a `.tar.gz` archive.
**Why**: Apple notarization inspects inside archives and rejects unsigned binaries. By extracting and signing at build time (in the Go download script when `APPLE_SIGNING_IDENTITY` is set), each binary is individually codesigned with hardened runtime + secure timestamp. This also removes the `tar` and `flate2` Rust dependencies — `extract.rs` just copies files instead of decompressing.

**Decision**: Suggestion sanitization strips bullets, markdown, numbering, and deduplicates case-insensitively.
**Why**: Small LLMs (3B params) inconsistently follow formatting instructions. The same model that returns clean `docs\ntests\n` on one prompt may return `1. **Docs**\n2. tests` on the next. Aggressive sanitization makes the output reliable regardless of LLM mood.

## Gotchas

**Gotcha**: `tauri::async_runtime::spawn` is used in `init()` instead of `tokio::spawn`.
**Why**: `init()` runs during Tauri setup before the tokio runtime is fully available. `tauri::async_runtime::spawn` uses Tauri's own runtime which is always ready at that point.

**Gotcha**: `get_folder_suggestions` returns `Ok(Vec::new())` on LLM errors, not `Err`.
**Why**: AI suggestions are a nice-to-have enhancement. Propagating errors would force the frontend to show error UI for a non-critical feature. Returning empty gracefully hides the failure — the user just sees no suggestions, same as if AI were not installed.

## Dependencies

External: `reqwest`, `tokio`, `libc`, `futures_util`
Internal: `crate::ignore_poison::IgnorePoison`, `crate::file_system::get_file_at`
