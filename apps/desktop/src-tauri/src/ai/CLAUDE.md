# AI subsystem

AI features powered by local LLM (llama-server) or remote LLM providers. Used for folder name suggestions and
natural-language search. Three provider modes: Off, Cloud AI (BYOK, any OpenAI-compatible endpoint), and Local LLM
(on-device llama-server, Apple Silicon only). `ai.provider` is the single source of truth for whether AI is on.

Frontend counterpart: [`apps/desktop/src/lib/ai/CLAUDE.md`](../../../src/lib/ai/CLAUDE.md).

## Module map

Local-AI lifecycle, split by concern around one shared singleton (depth in DETAILS.md):

- `state.rs`: owns the `Mutex<Option<ManagerState>>` singleton + `ai-state.json` persistence + derived install/model
  facts. Others borrow `&mut ManagerState` through its `MANAGER` lock; don't add a second copy of this state.
- `manager.rs`: thin facade — cross-cutting commands (`init`/`shutdown`, status, `configure_ai`) + `resolve_backend()`.
- `install.rs`: model download + verify + first-launch install, cancel, uninstall.
- `server.rs`: llama-server process orchestration over the stateless syscalls in `process.rs`.
- `connection_check.rs`: cloud-endpoint probe + the `validate_ai_base_url` key-safety gate.
- `stream_registry.rs`: the `STREAM_CANCEL_TOKENS` registry, kept off `ManagerState` on purpose.
- `client.rs`: `genai`-backed chat client (`AiBackend`); model name picks the provider adapter.
- `download.rs` / `extract.rs` / `process.rs`: stateless leaves (HTTP download, extraction, llama-server syscalls).
- `suggestions.rs`: folder-name prompt + streaming sanitizer. `api_keys.rs`: per-provider key storage.
  `translate_error.rs`: typed error for the two translate commands.
- `llm_log/`: on-disk log of every LLM request/response, tapped in `client.rs`. See
  [`llm_log/CLAUDE.md`](llm_log/CLAUDE.md).

## Must-knows

- **Only local AI requires Apple Silicon.** Cloud AI (BYOK) works on Intel too. Gate only local-specific paths
  (`start_ai_server`, `start_ai_download`, the `Offer` branch of `compute_ai_status`) on `is_local_ai_supported()`, never
  the whole subsystem. Gating `Offer` wrong shows Intel users a download toast for a model they can't run.
- **Unrecognized model names fall onto OpenAI chat-completions, never Ollama.** In `remote_model_iden`, everything that
  isn't `claude-*` (Anthropic) or `gemini-*` (Gemini) is forced onto the `openai::` namespace. `genai`'s default for
  unknown names is its Ollama adapter, which POSTs to `/api/chat` and 404s against an OpenAI endpoint. Every BYOK
  provider except Anthropic/Gemini speaks OpenAI chat-completions.
- **Don't relax the `http://` base-URL gate.** `validate_ai_base_url` rejects plaintext `http://` to a non-loopback host
  when an API key is set, blocking key exfil to a malicious "free proxy". Loopback keeps `http://` (Ollama/LM Studio);
  empty key is allowed. The rejection is the gate, not a warning.
- **`genai` needs `base_url` ending in `/`.** Without it, `Url::join("chat/completions")` strips the last segment and you
  hit a 404. `build_client` appends `/` if missing.
- **Process spawn + `child_pid` assignment must be synchronous inside the MANAGER lock** (`spawn_and_track_server`).
  Spawning inside an async task opens a race where a process exists untracked, orphaning llama-servers on rapid provider
  switching. `wait_for_server_health` (async) follows and kills the process on timeout or early death; don't remove that
  cleanup.
- **Two install flags both required**: `AiState.installed` AND `AiState.model_download_complete`. The second is set only
  after file-size verification, so a truncated 2 GB download never launches llama-server.
- **`configure_ai` must NOT block** (only the health check runs async via `tauri::async_runtime::spawn`); blocking
  freezes the frontend on startup. Use `tauri::async_runtime::spawn` (not `tokio::spawn`) in `configure_ai` /
  `start_ai_server`: they may run before the tokio runtime is ready.
- **Cancellation needs the explicit `cancel_folder_suggestions` command** + `CancellationToken`, not `Channel::send`
  failure. `Channel::send` succeeds silently after the JS handler is GC'd and only errs when the webview is gone, so
  relying on it for liveness keeps the backend streaming (billing cloud, pegging local compute) after dialog close.
- **`get_folder_suggestions` returns `Ok(Vec::new())` on AI errors**, not `Err` (folder suggestions are nice-to-have).

## Adding a new model

Add the GGUF to `AVAILABLE_MODELS` in `mod.rs` (with `kv_bytes_per_token` + `base_overhead_bytes`), and set
`DEFAULT_MODEL_ID` if it should be the default.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
