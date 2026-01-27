# Local AI

Optional AI features powered by a local LLM (Falcon-H1R-7B). No cloud, no accounts — everything runs on-device.

## Requirements

**Apple Silicon (M1 or later) required.** Intel Macs are not supported for AI features.

This is because:
- The bundled llama-server binary is ARM64-only (avoids ~60 MB bundle size for universal binary)
- Modern ML inference relies heavily on Metal GPU acceleration only available on Apple Silicon
- Apple stopped selling Intel Macs in 2020; the remaining Intel user base is small and shrinking

On Intel Macs, AI features are completely hidden — no notification, no download prompt, nothing. The app works
normally without AI.

## Overview

Cmdr can download and run a local language model to provide smart suggestions. The model is managed as a background
process — the user doesn't need to install anything externally. AI is entirely optional; the app works fully without it.

## Install flow

On Apple Silicon Macs, when the model files are missing, a notification appears in the top-right:

1. **Offer**: "AI features available — download the AI model (4.3 GB) to enable smart suggestions."
   - **Download**: Starts the download
   - **Not now**: Hides for 7 days (will re-appear)
   - **I don't want AI**: Permanent opt-out (can re-enable in settings)
2. **Downloading**: Progress bar with speed and ETA. Cancellable.
3. **Ready**: "AI ready — try creating a new folder (F7) to see AI-powered name suggestions."

## Opt-out

Users can permanently opt out of AI features by clicking "I don't want AI" in the offer notification.
When opted out:
- AI status returns `unavailable`
- No AI notification is shown
- Folder suggestions section is hidden

Users can re-enable AI in Settings. This clears the opt-out flag and shows the offer again.

## AI features

### Folder name suggestions

When creating a new folder (F7), the dialog shows AI-generated folder name suggestions below the input field.
Suggestions are based on the current directory's contents — the model looks at existing files and folders and proposes
names that fit the structure.

- Clicking a suggestion fills the folder name input (doesn't submit).
- Up to 5 suggestions shown.
- If AI is unavailable, the suggestions section is hidden (no errors shown).

## Implementation

### Model registry

Models are defined in `apps/desktop/src-tauri/src/ai/mod.rs` in the `AVAILABLE_MODELS` array.
Each model has an ID, display name, filename, HuggingFace URL, and expected file size.

Current default model:
- **Ministral 3B** (Q4_K_M, ~2.0 GB GGUF) — Mistral's efficient edge model
- Runs via **llama-server** (llama.cpp) — bundled with the app, not downloaded
- Apple Silicon only (M1+), Metal GPU acceleration
- Fast responses (1-3 seconds for folder suggestions)

### Switching to a new model

To add or switch to a new model:

1. **Find the GGUF file** on HuggingFace (must be compatible with llama.cpp)

2. **Get the exact file size** — this is critical for download verification:
   ```bash
   curl -sIL "<huggingface-url>" | grep -i content-length
   ```

3. **Add to the model registry** in `src/ai/mod.rs`:
   ```rust
   pub const AVAILABLE_MODELS: &[ModelInfo] = &[
       ModelInfo {
           id: "new-model-id",
           display_name: "New Model Name",
           filename: "new-model.gguf",
           url: "https://huggingface.co/...",
           size_bytes: 1234567890, // From step 2
       },
       // ... existing models
   ];
   ```

4. **Update `DEFAULT_MODEL_ID`** if the new model should be the default for new installs

5. **Test the full flow** with `CMDR_REAL_AI=1 pnpm dev`

**TODO**: Implement model upgrade notification (prompt user when a new model is available in a newer app version).

### Backend

- **Module**: `apps/desktop/src-tauri/src/ai/`
  - `mod.rs` — Module declarations, `use_real_ai()` runtime check
  - `manager.rs` — Model download, runtime extraction, process lifecycle (start/stop/health)
  - `client.rs` — HTTP client for the local llama-server API
  - `suggestions.rs` — Prompt construction and response parsing
- **Bundled resource**: `src-tauri/resources/llama-server.tar.gz` — llama-server binary + dylibs
- **Runtime storage**: `~/Library/Application Support/com.cmdr.app/ai/` (extracted binary + model + state)
- **Commands**:
  - `get_ai_status` — Returns current AI state (unavailable/offer/downloading/installing/available)
  - `get_ai_model_info` — Returns current model info (id, display name, size)
  - `start_ai_download` — Begins download + install
  - `cancel_ai_download` — Cancels in-progress download
  - `dismiss_ai_offer` — Hides the notification for 7 days
  - `opt_out_ai` — Permanent opt-out (sets `opted_out: true` in state)
  - `opt_in_ai` — Re-enables AI after opting out
  - `is_ai_opted_out` — Returns whether user has opted out
  - `uninstall_ai` — Removes model and binary, resets state
  - `get_folder_suggestions` — Returns AI-generated folder names for a directory

### Frontend

- **Notification**: `apps/desktop/src/lib/AiNotification.svelte`
- **State**: `apps/desktop/src/lib/ai-state.svelte.ts`
- **Dialog integration**: Suggestions section in `NewFolderDialog.svelte`
- **Tauri wrappers**: `getFolderSuggestions()` and AI state functions in `$lib/tauri-commands.ts`

### Dev mode

In dev mode, AI features are **disabled by default**:
- `get_ai_status()` returns `unavailable`
- `get_folder_suggestions()` returns empty array
- No AI notification is shown

This follows the [security policy](../security.md#withglobaltauri) — dev mode doesn't make external network requests.

#### Testing real AI in dev mode

To test the full AI flow during development, set the `CMDR_REAL_AI` environment variable:

```bash
CMDR_REAL_AI=1 pnpm dev
```

Or use the convenience script with AI debug logging enabled:

```bash
pnpm dev:ai-debug
```

With this env var (on Apple Silicon only):
- AI notification appears on first launch
- llama-server is extracted from bundled archive
- Model downloads from HuggingFace (~4.3 GB)
- llama-server process starts and runs inference

**Note**: Requires Apple Silicon. On Intel Macs, AI remains unavailable even with this env var.

To reset and test fresh:
```bash
rm -rf ~/Library/Application\ Support/com.cmdr.app/ai/
```

#### Debugging AI features

Use these RUST_LOG configurations to debug AI issues:

```bash
# Debug logging for AI module (recommended)
RUST_LOG=cmdr_lib::ai=debug pnpm dev

# Trace logging to see prompts and LLM responses
RUST_LOG=cmdr_lib::ai=trace pnpm dev

# Or use the convenience script
pnpm dev:ai-debug
```

**Log levels:**
- `debug` — Shows function calls, file counts, suggestion counts
- `trace` — Additionally shows full prompts and raw LLM responses

**What to look for:**
```
[DEBUG] AI manager: ready=true                           # Server started
[DEBUG] AI suggestions: calling LLM on port 61409...    # Request sent
[TRACE] AI suggestions: prompt: ...                      # Full prompt (trace only)
[TRACE] AI suggestions: raw response: ...                # LLM output (trace only)
[DEBUG] AI suggestions: got 5 suggestions: [...]         # Parsed results
[WARN] AI suggestions: LLM call failed: timeout          # Errors
```

**llama-server logs** are written to:
```
~/Library/Application Support/com.cmdr.app/ai/llama-server.log
```

## Attribution

Ministral 3B is developed by Mistral AI under the Apache 2.0 license (permissive open source).
Attribution is displayed in the About window.
