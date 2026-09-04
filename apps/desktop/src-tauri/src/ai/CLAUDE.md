# AI subsystem

AI for folder-name suggestions and natural-language search, over a local LLM or a remote provider. Three modes: Off,
Cloud AI (BYOK, any OpenAI-compatible endpoint), and Local LLM (on-device llama-server, Apple Silicon only).
`ai.provider` is the single source of truth for whether AI is on.

Frontend counterpart: `apps/desktop/src/lib/ai/CLAUDE.md`.

## Module map

The local-AI lifecycle is split by concern around ONE shared singleton: `state.rs` owns the
`Mutex<Option<ManagerState>>` and `ai-state.json`, and `manager.rs` (facade), `install.rs`, `server.rs`,
`connection_check.rs`, and `stream_registry.rs` borrow through its `MANAGER` lock — don't add a second copy of that
state. `download.rs` / `extract.rs` / `process.rs` are its stateless leaves.

Cloud-side: `client.rs` is the `genai` chat client (`AiBackend`), tapped for logging into `llm_log/CLAUDE.md`;
`api_keys.rs`, `suggestions.rs`, `translate_error.rs`, and the test-only `smoke_providers.rs` sit beside it. Per-file
detail: DETAILS.md.

## Must-knows

- **Only local AI requires Apple Silicon.** Cloud AI (BYOK) works on Intel too, so gate only local-specific paths
  (`start_ai_server`, `start_ai_download`, `compute_ai_status`'s `Offer` branch) on `is_local_ai_supported()`. Gating
  `Offer` wrong offers Intel users a model they can't run.
- **Unrecognized model names fall onto OpenAI chat-completions, never Ollama.** `remote_model_iden` forces everything
  that isn't `claude-*` / `gemini-*` onto `openai::`; `genai`'s own default is its Ollama adapter, which 404s against an
  OpenAI endpoint.
- **A stored API key never crosses IPC to a webview.** `configure_ai` / `check_ai_connection` take a PROVIDER ID and
  read it backend-side; `get_ai_api_key_status` returns is-set + a fingerprint. ❌ Never add a key-returning command or
  key param back. `docs/security.md` § "AI API keys".
- **Don't relax the `http://` base-URL gate.** `validate_ai_base_url` rejects plaintext `http://` to a non-loopback
  host when a key is set, blocking exfil to a malicious "free proxy". Loopback keeps `http://` (Ollama/LM Studio), and
  an empty key is allowed. The rejection IS the gate, not a warning.
- **A model id lives in `smoke_providers.rs` only.** Groq's 2026-08 retirement cost three edits because the id sat in a
  doc comment and a unit assertion too. Refresh pins there; the header has the recipe (ask the live model list).
- **Classify a provider failure by HTTP status, never its sentence.** `map_genai_error` must reach
  `ai_error_for_status` from BOTH `genai` shapes: `Web{Adapter,Model}Call`, AND the `WebStream` wrapping a boxed
  `HttpError`. Missing the stream one degrades every streaming failure to `ServerError` — the agent's own path.
- **`genai` needs `base_url` ending in `/`.** Without it, `Url::join("chat/completions")` strips the last segment and you
  hit a 404. `build_client` appends `/` if missing.
- **Process spawn + `child_pid` assignment must be synchronous inside the MANAGER lock** (`spawn_and_track_server`):
  an async spawn orphans llama-servers on rapid provider switching. Keep `wait_for_server_health`'s cleanup, which
  stops the process on timeout or early death.
- **Two install flags both required**: `AiState.installed` AND `AiState.model_download_complete`, the second set only
  after file-size verification, so a truncated 2 GB download never launches llama-server.
- **`configure_ai` must NOT block**; blocking freezes the frontend on startup. Its health check, and
  `start_ai_server`, use `tauri::async_runtime::spawn` (not `tokio::spawn`): both may run before tokio is ready.
- **Cancellation needs the explicit `cancel_folder_suggestions` command** + `CancellationToken`, never `Channel::send`
  failure: `send` succeeds silently after the JS handler is GC'd, so the backend streams on (billing cloud, pegging
  local compute) past dialog close.
- **`get_folder_suggestions` returns `Ok(Vec::new())` on AI errors**, not `Err` (folder suggestions are nice-to-have).

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
