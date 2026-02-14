# AI features (frontend)

UI and state for local LLM-powered features. Backend: `src-tauri/src/ai/` (download, llama-server process, inference).

## Architecture

- **State**: `ai-state.svelte.ts` — Reactive AI status, download progress, Tauri event listeners
- **Notification**: `AiNotification.svelte` — Install flow UI (offer → downloading → installing → ready)
- **Backend**: See `src-tauri/src/ai/` for model download, llama-server lifecycle, inference client

## Key decisions

### Dev mode disabled by default

In dev mode, AI returns `Unavailable` unless `CMDR_REAL_AI=1` env var set. Prevents large downloads during development.
Use `pnpm dev:ai-debug` to test real AI.

### Apple Silicon only

AI features completely hidden on Intel Macs (no notification, no error). llama-server binary is ARM64-only to save 60 MB
bundle size.

### 7-day dismissal, permanent opt-out

"Not now" hides offer for 7 days (`dismissedUntil` timestamp in state). "I don't want AI" sets `opted_out: true`
(permanent). Re-enable via Settings.

### Model registry is extensible

`AVAILABLE_MODELS` constant in backend (`src-tauri/src/ai/mod.rs`) defines available models. Current default: Ministral
3B (~2.0 GB). Falcon H1R 7B still in registry as fallback.

### Folder suggestions: 10s timeout, graceful failure

When opening "New folder" dialog, calls `getFolderSuggestions()`. If LLM doesn't respond in 10s, returns empty array. UI
hides suggestions section (no error shown). Feature degrades gracefully.

### Download resumption via HTTP Range

Model download supports HTTP Range header for resume after interruption. No SHA256 verification (HuggingFace doesn't
provide checksums) — file size check only.

## Gotchas

- **Status transitions are frontend-driven**: Backend emits `ai-download-progress` and `ai-install-complete` events.
  Frontend interprets these to update `aiStatus`. Backend has no "status" concept — just `AiState` (installed/port/pid).
- **llama-server is NOT auto-restarted**: Health monitoring (periodic restart on crash) is deferred. If server crashes,
  it stays down until app restart. User sees "AI unavailable."
- **Model switch requires app restart**: Changing selected model in Settings requires download + restart. No hot-swap.
- **`opted_out` flag is sticky**: After opting out, re-enabling clears the flag but doesn't auto-start download. User
  must click "Download" again. Prevents surprise 2 GB download.

## Development

**Run with mock license**:

```bash
CMDR_MOCK_LICENSE=commercial pnpm tauri dev
```

**Test with real AI**:

```bash
pnpm dev:ai-debug
```

**llama-server update**: See `apps/desktop/scripts/download-llama-server.go` (version + SHA256) **Model registry**: See
`apps/desktop/src-tauri/src/ai/mod.rs` (`AVAILABLE_MODELS`) **Attribution**: Ministral 3B by Mistral AI (Apache 2.0),
attribution in About window
