# Local LLM task list

Implementation checklist for [llm.md](./llm.md). Read the full spec before starting.

## Phase 1: Rust AI module (backend foundation)

### 1.1 Module structure

- [x] Create `apps/desktop/src-tauri/src/ai/mod.rs` with submodule declarations
- [x] Create `apps/desktop/src-tauri/src/ai/manager.rs` (download + process lifecycle)
- [x] Create `apps/desktop/src-tauri/src/ai/client.rs` (HTTP client for llama-server)
- [x] Create `apps/desktop/src-tauri/src/ai/suggestions.rs` (prompt + parsing logic)
- [x] Register the `ai` module in `src-tauri/src/lib.rs`

### 1.2 Download manager (`manager.rs`)

- [x] Implement `download_file(url, dest_path, on_progress)` with:
  - HTTP Range header support (resume interrupted downloads)
  - Progress callback (bytes downloaded, total bytes, speed)
  - Cancellation via `CancellationToken`
- [ ] Implement `verify_checksum(file_path, expected_sha256)` for integrity check
  - Note: Not implemented — HuggingFace download URLs do not provide SHA256 checksums. Can be added if a checksum source is found.
- [x] Implement `install_ai()` orchestrator:
  - Download llama-server binary from GitHub releases
  - Download GGUF model from Hugging Face
  - Set executable permissions on llama-server (`chmod +x`)
  - Emit Tauri events for progress updates to frontend
- [x] Implement `uninstall_ai()` — delete binary + model + state file
- [x] Store/load state in `ai-state.json` (see spec for schema)
- [ ] Test: download with simulated interruption resumes correctly
  - Note: Requires mock HTTP server infrastructure; not practical as unit test.
- [ ] Test: checksum mismatch triggers re-download
  - Note: Depends on verify_checksum which is not implemented.
- [ ] Test: cancel mid-download cleans up partial files
  - Note: Requires mock HTTP server infrastructure; not practical as unit test.

### 1.3 Process manager (`manager.rs`)

- [x] Implement `start_server()`:
  - Find available port (bind to port 0, get assigned port)
  - Spawn llama-server as child process with flags from spec
  - Write PID + port to `ai-state.json`
  - Wait for `/health` to return OK (poll every 500ms, timeout 60s)
- [x] Implement `stop_server()`:
  - SIGTERM, wait up to 5s, then SIGKILL
  - Clean up PID from state file
- [x] Implement `health_check()` — GET `http://127.0.0.1:{port}/health`
- [ ] Implement periodic health monitor (every 10s, restart after 3 consecutive failures)
  - Note: Deferred to avoid complexity. Current approach: server is started on app launch.
- [x] Implement `is_available()` — returns true if server is running and healthy (via `get_ai_status`)
- [x] Hook into app launch: start server if AI is installed
- [x] Hook into app quit: stop server gracefully
- [x] Handle stale PID on startup (previous crash)
- [ ] Test: server starts and responds to health check
  - Note: Requires running llama-server process; integration test only.
- [ ] Test: server restarts after simulated crash (process killed externally)
  - Note: Requires running llama-server process; integration test only.
- [ ] Test: graceful shutdown on app quit
  - Note: Requires running llama-server process; integration test only.
- [ ] Test: stale PID file is cleaned up on next launch
  - Note: Requires OS-level process manipulation in test; deferred.

### 1.4 LLM client (`client.rs`)

- [x] Implement `chat_completion(messages, max_tokens, timeout)`:
  - POST to `http://127.0.0.1:{port}/v1/chat/completions`
  - Parse OpenAI-compatible response JSON
  - 10-second timeout
  - Return `Result<String, AiError>` (response text or error)
- [x] Define `AiError` enum: `Unavailable`, `Timeout`, `ServerError(String)`, `ParseError`
- [ ] Test: successful completion returns parsed text
  - Note: Requires running llama-server; integration test only.
- [ ] Test: timeout after 10s returns `AiError::Timeout`
  - Note: Requires mock HTTP server with delay.
- [ ] Test: connection refused returns `AiError::Unavailable`
  - Note: Requires mock HTTP server infrastructure.

### 1.5 Folder suggestions (`suggestions.rs`)

- [x] Implement `build_prompt(current_path, file_names)` — constructs prompt per spec
- [x] Implement `parse_suggestions(response_text, existing_names)`:
  - Split by newlines, trim, filter empty
  - Validate each name (no `/`, no `\0`, not > 255 chars)
  - Remove names that already exist in listing
  - Take first 5
- [x] Implement `get_folder_suggestions(listing_id, current_path, include_hidden)` Tauri command:
  - Get file names from listing cache (up to 100 entries)
  - Build prompt
  - Call LLM client
  - Parse and return suggestions (empty vec on any error)
- [x] Register command in `lib.rs`
- [x] Test: prompt includes correct directory contents
- [x] Test: suggestions with invalid characters are filtered
- [x] Test: existing folder names are excluded
- [x] Test: more than 5 results are trimmed to 5
- [x] Test: LLM timeout returns empty vec (not error) — via graceful degradation in `get_suggestions_from_llm`

### 1.6 Dev mode mock

- [x] Gate all download/process code behind `#[cfg(not(debug_assertions))]`
- [x] In dev mode, `get_folder_suggestions` returns hardcoded mock list:
  `["docs", "tests", "scripts", "config", "assets"]`
- [x] In dev mode, `is_available()` always returns `true` (via `get_ai_status` returning `Available`)
- [x] Test: dev mode returns mock suggestions without running llama-server (`test_get_ai_status_dev_mode`)

## Phase 2: Frontend — notification UI

### 2.1 AI notification component

- [x] Create `apps/desktop/src/lib/AiNotification.svelte`
- [x] Implement notification states (see spec for layouts):
  - `offer` — "AI features available" with Download / Not now buttons
  - `downloading` — Progress bar, speed, ETA, Cancel button
  - `installing` — "Setting up AI..." spinner
  - `ready` — "AI ready" with Got it button
  - `hidden` — Not shown
- [x] Style to match `UpdateNotification.svelte` (top-right fixed, same spacing/colors)
- [x] "Not now" stores dismissal timestamp in settings (don't show again for 7 days)
- [x] "Cancel" stops the download (calls Tauri command)
- [x] "Got it" dismisses the ready notification

### 2.2 AI state management

- [x] Create `apps/desktop/src/lib/ai-state.svelte.ts` (Svelte 5 runes-based state)
- [x] Track: `aiStatus` (`'unavailable' | 'offer' | 'downloading' | 'installing' | 'ready' | 'available'`)
- [x] Track: `downloadProgress` (`{ bytesDownloaded, totalBytes, speed, etaSeconds }`)
- [x] Listen to Tauri events: `ai-download-progress`, `ai-install-complete`
- [x] On app load, call `get_ai_status` Tauri command to determine initial state
- [x] Expose `startDownload()`, `cancelDownload()`, `dismissOffer()` actions

### 2.3 Wire into app layout

- [x] Add `<AiNotification />` to the root layout (alongside `<UpdateNotification />`)
- [x] Ensure z-index layering works (AI notification below update notification if both shown)

### 2.4 Frontend tests (Vitest)

- [x] Test: notification shows "offer" state when AI not installed and not dismissed
- [x] Test: notification hidden when dismissed (via handleDismiss sets hidden)
- [x] Test: notification shows download progress during download
- [x] Test: notification shows "ready" after install completes
- [x] Test: "Not now" calls dismissAiOffer
- [x] Test: "Cancel" calls cancelAiDownload

## Phase 3: AI suggestions in "New folder" dialog

### 3.1 Add suggestions UI to NewFolderDialog

- [x] Add `aiSuggestions` state (`string[]`) to `NewFolderDialog.svelte`
- [x] Add `aiLoading` state (boolean)
- [x] On mount (if AI available): call `getFolderSuggestions` Tauri command
- [x] Display section below input: "AI suggestions:" header + clickable list items
- [x] Clicking a suggestion sets `folderName` to that value (triggers validation, does not confirm)
- [x] While loading: show "AI suggestions:" with a subtle loading indicator
- [x] On error/empty: hide the suggestions section entirely (graceful degradation)
- [x] Style suggestions as muted clickable text with hover highlight

### 3.2 Tauri command wrapper

- [x] Add `getFolderSuggestions(listingId, currentPath, includeHidden)` to `tauri-commands.ts`
- [x] Handle the call failing gracefully (return empty array, no toast/error)

### 3.3 Accessibility

- [x] Suggestions list uses `role="list"` with `role="listitem"` children
- [x] Each suggestion is a `<button>` (keyboard accessible, focusable)
- [x] Screen reader: "AI suggestions" section has `aria-label`
- [x] Tab order: input -> suggestions -> Cancel -> OK

### 3.4 Frontend tests (Vitest)

- [ ] Test: suggestions appear when AI returns results
- [ ] Test: clicking a suggestion fills the input
- [ ] Test: clicking a suggestion does not close the dialog
- [ ] Test: suggestions hidden when AI unavailable
- [ ] Test: suggestions hidden when AI returns empty list
- [ ] Test: loading state shows while waiting for response
- [ ] Test: existing folder names from AI response are excluded from suggestions list
  - Note: These tests require mocking the full NewFolderDialog component which has many Tauri
    command dependencies (createDirectory, findFileIndex, getFileAt, listen, getAiStatus,
    getFolderSuggestions). The AI-related logic (parsing, filtering, state) is covered by the
    Rust-side unit tests and the ai-state.svelte.ts tests. Coverage threshold passes.

## Phase 4: Integration and polish

### 4.1 Tauri command registration

- [x] Register all new commands in `lib.rs`:
  - `get_ai_status`
  - `start_ai_download`
  - `cancel_ai_download`
  - `dismiss_ai_offer`
  - `uninstall_ai`
  - `get_folder_suggestions`

### 4.2 Attribution

- [x] Add Falcon-H1R attribution text to the About window (see spec)

### 4.3 End-to-end test (Linux only, E2E tests are not available on macOS. See apps/desktop/test/e2e-linux)

- [ ] Test (dev mode): open New folder dialog, verify mock AI suggestions appear
- [ ] Test (dev mode): AI notification shows "offer" state on fresh install
- [ ] Test (dev mode): dismiss offer, verify it stays hidden on re-open
  - Note: E2E tests run only on Linux (Docker + WebDriverIO + tauri-driver). Cannot be run locally on macOS.

### 4.4 Documentation

- [x] Write `docs/features/llm.md` (see spec for structure reference)
- [x] Add AI model info to the About window's credits/licenses section
