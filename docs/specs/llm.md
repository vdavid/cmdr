# Local LLM specification

Optional AI features powered by a locally-running LLM. The model runs as a managed background process — no cloud, no
accounts, no external dependencies.

## Design principles

- **Optional**: The app works fully without AI. AI is a recommended add-on.
- **Zero dependencies**: The user doesn't need Ollama, Python, or Homebrew. We download and manage everything.
- **Privacy-first**: All inference is local. No data leaves the machine.
- **Minimal UI**: No settings page. A single notification drives the install flow.

## Model

**Falcon-H1R-7B** (Q4_K_M quantization, 4.6 GB GGUF)

- Architecture: Hybrid Transformer + Mamba2 (linear-time sequence processing, efficient long outputs)
- Source: [tiiuae/Falcon-H1R-7B-GGUF](https://huggingface.co/tiiuae/Falcon-H1R-7B-GGUF)
- License: Falcon LLM License 1.0 (royalty-free commercial, attribution required)
- Context: 256K tokens
- Target hardware: Apple Silicon only (M1+), Metal GPU acceleration

### Performance expectations (Apple Silicon, Q4_K_M)

- Prompt processing: ~200-500 tok/s
- Token generation: ~50-100 tok/s
- RAM usage: ~6-8 GB during inference
- A typical 100-token response takes 1-2 seconds

## Runtime

**llama-server** from [llama.cpp](https://github.com/ggml-org/llama.cpp)

- Pre-built ARM64 macOS binary (~15 MB)
- Provides OpenAI-compatible HTTP API on localhost
- Metal GPU acceleration out of the box
- Source: [llama.cpp releases](https://github.com/ggml-org/llama.cpp/releases) (asset: `llama-*-bin-macos-arm64.zip`)

### Launch configuration

```bash
./llama-server \
  -m falcon-h1r-7b-q4km.gguf \
  --port <random-available-port> \
  --host 127.0.0.1 \
  --temp 0.6 \
  --top-p 0.95 \
  -n 4096 \
  --jinja \
  -ngl 99
```

Key flags:
- `--host 127.0.0.1`: Only accept local connections
- `--jinja`: Required for Falcon-H1R's chat template
- `-ngl 99`: Offload all layers to Metal GPU
- `-n 4096`: Max tokens per response (keep responses focused; 65536 is the model max but unnecessary here)

## File structure

```
~/Library/Application Support/com.cmdr.app/ai/
├── llama-server              (ARM64 binary, ~15 MB)
├── falcon-h1r-7b-q4km.gguf  (model, 4.6 GB)
└── ai-state.json             (port, PID, install status)
```

## Install flow

### Trigger

On first launch (or after AI files are missing), show a notification in the top-right corner:

```
┌─────────────────────────────────────────────┐
│ AI features available                       │
│ Download the AI model (4.6 GB) to enable    │
│ smart suggestions in Cmdr.                  │
│                                             │
│                    [Not now]  [Download]     │
└─────────────────────────────────────────────┘
```

If dismissed ("Not now"), don't show again for 7 days. Store the dismissal timestamp in `settings.json`.

### Download phase

Replace notification content with download progress:

```
┌─────────────────────────────────────────────┐
│ Downloading AI model...                     │
│ ████████████░░░░░░░░░░░░  2.1 / 4.6 GB     │
│ 45% — 12 MB/s — ~3 min remaining           │
│                                [Cancel]     │
└─────────────────────────────────────────────┘
```

Download order:
1. `llama-server` binary (~15 MB) — from GitHub releases
2. `falcon-h1r-7b-q4km.gguf` (4.6 GB) — from Hugging Face

The download must:
- Support resumption (HTTP Range headers) in case of interruption
- Verify integrity (SHA-256 checksum comparison after download)
- Be cancellable at any point
- Show speed + ETA

### Install phase

After download, set executable permissions on `llama-server` and start the process:

```
┌─────────────────────────────────────────────┐
│ Setting up AI...                            │
│ Starting inference server                   │
└─────────────────────────────────────────────┘
```

### Ready

Once `/health` returns OK:

```
┌─────────────────────────────────────────────┐
│ AI ready                                    │
│ Try creating a new folder (F7) to see       │
│ AI-powered name suggestions.                │
│                              [Got it]       │
└─────────────────────────────────────────────┘
```

No restart needed — the server starts in the background and becomes available immediately.

## Process management (Rust backend)

### Module: `src-tauri/src/ai/mod.rs`

Submodules:
- `manager.rs` — Download, start, stop, health check
- `client.rs` — HTTP client for llama-server API
- `suggestions.rs` — Prompt construction and response parsing for folder name suggestions

### Lifecycle

1. **App launch**: Check if AI is installed (binary + model exist). If yes, spawn llama-server.
2. **Health monitoring**: Poll `/health` every 10s. If unhealthy for 3 consecutive checks, restart.
3. **App quit**: Send SIGTERM to llama-server, wait up to 5s, then SIGKILL.
4. **Crash recovery**: On next launch, clean up stale PID file and restart.

### State management

`ai-state.json`:
```json
{
  "installed": true,
  "port": 52847,
  "pid": 12345,
  "modelVersion": "falcon-h1r-7b-q4km",
  "installedAt": "2026-01-20T10:30:00Z",
  "dismissedUntil": null
}
```

### Dev mode behavior

In dev mode (`cfg(debug_assertions)`), AI features use **mock responses** instead of actually downloading/running the
model. The mock returns a hardcoded list of folder suggestions. This avoids large downloads during development and
follows the [security policy](../security.md#withglobaltauri).

## AI feature: folder name suggestions

### UX in the "New folder" dialog

When AI is available, the dialog shows suggestions below the input:

```
┌─────────────────────────────────────────────┐
│               New folder                    │
│    Create folder in current-directory       │
│                                             │
│  ┌─────────────────────────────────────┐    │
│  │ my-project                          │    │
│  └─────────────────────────────────────┘    │
│                                             │
│  AI suggestions:                            │
│    docs                                     │
│    tests                                    │
│    scripts                                  │
│    config                                   │
│    assets                                   │
│                                             │
│          [Cancel]          [OK]             │
└─────────────────────────────────────────────┘
```

- Suggestions appear as a list of clickable items below the input.
- Clicking a suggestion fills the input with that name (does NOT confirm/submit).
- The suggestions section shows "Loading..." with a subtle animation while waiting for the LLM response.
- If AI is unavailable or errors, the section is hidden (no error shown — graceful degradation).
- Suggestions are fetched once when the dialog opens (not re-fetched on input changes).
- Maximum 5 suggestions displayed.

### Prompt design

The LLM receives the current directory listing (file/folder names only, up to 100 entries) and is asked to suggest
sensible new folder names:

```
You are a file organization assistant. Given the contents of a directory, suggest 5 new folder names
that would make sense to create here. Consider the existing structure, naming conventions, and common
project patterns.

Current directory: /Users/name/projects/my-app
Contents:
- src/ (directory)
- package.json
- tsconfig.json
- README.md
- .gitignore

Respond with exactly 5 folder names, one per line, no numbering, no explanation.
```

### API call

```
POST http://127.0.0.1:{port}/v1/chat/completions
Content-Type: application/json

{
  "model": "falcon-h1r-7b",
  "messages": [{"role": "user", "content": "<prompt above>"}],
  "temperature": 0.6,
  "top_p": 0.95,
  "max_tokens": 100,
  "stream": false
}
```

### Response parsing

Parse the response text: split by newlines, trim whitespace, filter empty lines, take first 5. Validate each name:
- Not empty
- No `/` or null characters
- Not longer than 255 characters
- Doesn't already exist in the directory listing

### Tauri command

```rust
#[tauri::command]
pub async fn get_folder_suggestions(listing_id: String, current_path: String, include_hidden: bool) -> Result<Vec<String>, String>
```

The frontend calls this when opening the "New folder" dialog. The command:
1. Gets file names from the listing cache (up to 100)
2. Constructs the prompt
3. Calls the local llama-server
4. Parses and validates the response
5. Returns the suggestion list (or empty vec on failure)

### Timeout

The LLM call has a 10-second timeout. If it doesn't respond in time, return an empty list (the dialog works fine
without suggestions).

## Attribution

The Falcon LLM License 1.0 requires attribution. Add to the "About" window:

> AI powered by Falcon-H1R-7B by Technology Innovation Institute (TII)

## Future extensions (out of scope for v1)

- Smart rename suggestions
- File search with natural language
- Directory organization recommendations
- Multiple model support (pick size vs quality)
- Cloud model fallback option
