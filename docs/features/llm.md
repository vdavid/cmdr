# Local AI

Optional AI features powered by a local LLM (Falcon-H1R-7B). No cloud, no accounts — everything runs on-device.

## Overview

Cmdr can download and run a local language model to provide smart suggestions. The model is managed as a background
process — the user doesn't need to install anything externally. AI is entirely optional; the app works fully without it.

## Install flow

On first launch (or when the model files are missing), a notification appears in the top-right:

1. **Offer**: "AI features available — download the AI model (4.6 GB) to enable smart suggestions."
2. **Downloading**: Progress bar with speed and ETA. Cancellable.
3. **Ready**: "AI ready — try creating a new folder (F7) to see AI-powered name suggestions."

Dismissing the offer hides it for 7 days.

## AI features

### Folder name suggestions

When creating a new folder (F7), the dialog shows AI-generated folder name suggestions below the input field.
Suggestions are based on the current directory's contents — the model looks at existing files and folders and proposes
names that fit the structure.

- Clicking a suggestion fills the folder name input (doesn't submit).
- Up to 5 suggestions shown.
- If AI is unavailable, the suggestions section is hidden (no errors shown).

## Implementation

### Model

- **Falcon-H1R-7B** (Q4_K_M, 4.6 GB GGUF) — hybrid Transformer + Mamba2 architecture
- Runs via **llama-server** (llama.cpp, ~15 MB binary)
- Apple Silicon only (M1+), Metal GPU acceleration
- ~50-100 tok/s generation speed, responses in 1-2 seconds

### Backend

- **Module**: `apps/desktop/src-tauri/src/ai/`
  - `mod.rs` — Module declarations
  - `manager.rs` — Download orchestration, process lifecycle (start/stop/health)
  - `client.rs` — HTTP client for the local llama-server API
  - `suggestions.rs` — Prompt construction and response parsing
- **Storage**: `~/Library/Application Support/com.cmdr.app/ai/` (binary + model + state)
- **Commands**:
  - `get_ai_status` — Returns current AI state (unavailable/available/downloading)
  - `start_ai_download` — Begins download + install
  - `cancel_ai_download` — Cancels in-progress download
  - `dismiss_ai_offer` — Hides the notification for 7 days
  - `get_folder_suggestions` — Returns AI-generated folder names for a directory

### Frontend

- **Notification**: `apps/desktop/src/lib/AiNotification.svelte`
- **State**: `apps/desktop/src/lib/ai-state.svelte.ts`
- **Dialog integration**: Suggestions section in `NewFolderDialog.svelte`
- **Tauri wrappers**: `getFolderSuggestions()` and AI state functions in `$lib/tauri-commands.ts`

### Dev mode

In dev mode, AI features use mock responses (hardcoded suggestions list). No model download or llama-server process.
This follows the [security policy](../security.md#withglobaltauri).

## Attribution

Falcon-H1R-7B is developed by the Technology Innovation Institute (TII) under the Falcon LLM License 1.0
(royalty-free commercial, attribution required). Attribution is displayed in the About window.
