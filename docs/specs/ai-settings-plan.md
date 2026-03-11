# AI settings section plan

## Intention

Give users full visibility and control over AI features. Today, AI is a black box: a toast that downloads 2 GB, starts a
server using ~400 MB of memory (was 27 GB before the context size fix!), and provides folder name suggestions — with no
way to see what's happening, stop it, or configure it after onboarding. The toast even says "You can add or remove AI
later in settings" but that UI doesn't exist.

We want AI to be a transparent, configurable tool with three provider modes:

- **Off**: No AI features. Clean, no resources used.
- **OpenAI-compatible** (BYOK): The primary recommended path long-term. Faster, no disk/memory cost, works on Intel
  Macs. Supports any OpenAI-compatible API: OpenAI, Groq, Together, Anthropic (via proxy), Azure OpenAI, or even a local
  Ollama/LM Studio server the user is running outside of Cmdr.
- **Local LLM**: The privacy-first fallback. Runs entirely on-device, no network needed, no API key needed. Bundled
  llama-server + downloadable model. Currently requires Apple Silicon.

**Strategic direction**: The default is `local` for now to match the current onboarding toast flow. Once this settings UI
is tested and stable, the onboarding flow will be reworked to make OpenAI-compatible the primary recommended path, with
local LLM as the secondary option for privacy-focused users. The implementing agent should NOT add extra complexity for
this future change — just be aware of the direction so the architecture doesn't fight it later.

**No existing users**: The app has not been publicly released yet. There are no migration concerns — we can make breaking
changes freely.

Users should be able to:

1. Choose their AI provider (off / OpenAI-compatible / local LLM)
2. See exactly what's installed, running, and consuming resources (radical transparency)
3. Configure provider-specific options
4. Tear down the local model to reclaim disk space

## Design decisions

**Decision**: AI is a flat special section (like License), not under General. Positioned prominently in the sidebar.
**Why**: AI is a distinct subsystem with its own lifecycle, not a tweak to general behavior. It deserves top-level
visibility. Flat (no subsections) because the content is conditional on provider — you never see everything at once.
"Off" shows nothing extra, "OpenAI-compatible" shows 3 fields, "Local" shows a status card + context size + actions.
Subsections would add navigation overhead for 3-5 fields. If AI grows later (model selection, prompt templates, usage
stats), we can split into subsections then.

**Decision**: Hybrid section — part dynamic state (Tauri commands), part registry settings.
**Why**: Provider selection, API key, context size are persistent settings (registry). Server status, download progress,
disk usage are dynamic runtime state (Tauri commands, like LicenseSection's license info).

**Decision**: Store OpenAI API key in settings.json (not keychain).
**Why**: Simpler first version. The file is already in the user's private app support directory. Keychain integration
can come later if needed. The key is never sent anywhere except the user's configured endpoint.

**Decision**: "OpenAI-compatible" label, not "OpenAI".
**Why**: Works with any OpenAI-compatible API. The base URL field makes this explicit. The tooltip should list popular
options so users understand the breadth.

**Decision**: Rich tooltips on each provider option to help users choose.
**Why**: Most users won't know the tradeoffs. The tooltips educate without cluttering the main UI.

**Decision**: Context window size goes up to 256K in 2x increments, with live memory estimate.
**Why**: Power users may want large context for heavier tasks. But they need to understand the memory cost. The live
estimate (displayed below the selector) makes the tradeoff visible and prevents surprises. The estimate is model-specific
and calculated from the current model's KV cache scaling factor.

**Decision**: Default provider is `local` (temporary, in the settings registry).
**Why**: Matches the current onboarding toast flow. Will change to `openai-compatible` once onboarding is reworked.

**Decision**: Frontend pushes AI config to backend after settings load — no disk reading in Rust.
**Why**: The frontend is the single source of truth for settings via `tauri-plugin-store`. Having Rust also read
`settings.json` directly would create a second reader with potential format/timing mismatches. Instead, `init()` in
Rust just sets up the manager state (stale PID cleanup, directory init) but does NOT start the server. After the
frontend loads settings, it calls `configure_ai({ provider, contextSize })` which triggers server start if appropriate.
This adds ~500ms delay to server availability (frontend load time) but is architecturally clean. On a fresh install
(no settings yet), the frontend sends the registry default (`local`) — but the model isn't installed, so nothing starts.

**Decision**: All settings auto-apply immediately (no "Apply" button). Context size changes restart the server.
**Why**: Every other setting in the app auto-applies. AI should be no different. Context size changes require a server
restart — debounce 2 seconds, then stop + restart llama-server in the background. If the user changes the value again
within the debounce window, the previous restart is cancelled. The UI shows a brief "Restarting..." status in the status
card. Multiple rapid restarts are fine — llama-server is a simple child process, stopping it mid-startup is harmless.

**Decision**: Switching provider away from `local` auto-stops the server immediately.
**Why**: If the user switches to "Off" or "OpenAI-compatible", leaving the server running wastes memory for nothing.
Stop it immediately. Switching back to `local` auto-starts it (if model is installed).

**Decision**: "Delete model" requires a confirmation dialog.
**Why**: Deleting 2 GB is a significant, slow-to-undo action (requires re-download). Per style guide: title = verb+noun
question, body = plain irreversibility warning, buttons = outcome verbs. Example: "Delete AI model?" / "This frees up
2.0 GB of disk space. You'll need to re-download it to use local AI again." / **Cancel** · **Delete**.

**Decision**: Toast onboarding respects `ai.provider`.
**Why**: If `ai.provider === 'off'`, the toast should not appear. The user has explicitly opted out via settings — don't
nag them. The toast should only appear when provider is `local` and the model isn't installed yet.

**Decision**: Rename `use_real_ai()` to `is_local_ai_supported()`. Remove it as a blanket gate.
**Why**: The old `use_real_ai()` gates ALL AI commands — it returns false on Intel Macs and in dev mode (without env
var). This blocks the OpenAI-compatible path on Intel Macs, which is wrong. The fix: rename to
`is_local_ai_supported()` and only use it to gate local-specific operations (`start_ai_server`, `start_ai_download`,
`spawn_llama_server`). Provider-agnostic commands (`get_folder_suggestions`, `get_ai_status`, `get_ai_runtime_status`)
check `ai.provider` directly and work on any hardware. In the settings UI, the "Local LLM" toggle option is **disabled**
on Intel Macs with a tooltip: "Local AI requires Apple Silicon (M1 or later)."

**Decision**: "Local LLM" toggle is disabled (greyed out, unselectable) on Intel Macs.
**Why**: The bundled llama-server binary is ARM64-only. Rather than hiding the option entirely (which would confuse
users who've heard about it), show it disabled with an explanatory tooltip. This is more transparent than silently
removing it.

## UI layout

### AI section in settings sidebar

Position: between License and Advanced in the special sections list. Label: "AI".

### Provider toggle tooltips

Each provider option in the toggle group has a rich tooltip:

- **Off**: "AI features are turned off. Cmdr works fully without AI — suggestions and smart features are simply hidden."
- **OpenAI-compatible**: "Bring your own API key for fast, high-quality AI. Works with OpenAI, Groq, Together AI,
  Azure OpenAI, Anthropic (via proxy), or any local server you're running (Ollama, LM Studio, etc.). Requires an
  internet connection (unless using a local server). No disk space or memory used by Cmdr."
- **Local LLM**: "Runs a small language model entirely on your device. Maximum privacy — nothing leaves your computer.
  Works offline. Uses ~2 GB disk space and ~400 MB memory (varies with context size). Requires Apple Silicon (M1+)."
- **Local LLM (disabled, Intel Mac)**: "Local AI requires Apple Silicon (M1 or later). Use OpenAI-compatible instead."

### Section content

```
┌─────────────────────────────────────────────────────────────────┐
│ AI                                                              │
│                                                                 │
│ Provider                                                        │
│ ┌───────────────────────────────────────────────────┐           │
│ │ [Off] [OpenAI-compatible] [Local LLM]             │           │
│ └───────────────────────────────────────────────────┘           │
│ Each option has a rich tooltip (see above)                      │
│ On Intel Macs, "Local LLM" is greyed out + disabled            │
│                                                                 │
│ ─── When "OpenAI-compatible" selected: ───────────────────────  │
│                                                                 │
│ API key          [••••••••••••••••••sk-1234]   ← password input │
│ Base URL         [https://api.openai.com/v1]   ← text input    │
│ Model            [gpt-4o-mini               ]  ← text input    │
│                                                                 │
│ ─── When "Local LLM" selected + installed + running: ────────── │
│                                                                 │
│ ┌─ Status ─────────────────────────────────────────────────────┐│
│ │ Model             Ministral 3B (2.0 GB on disk)              ││
│ │ Server            Running · PID 53218 · port 62199           ││
│ └──────────────────────────────────────────────────────────────┘│
│                                                                 │
│ Context window     [4096  ▾]                                    │
│ Estimated memory use: ~3.9 GB  ← updates live as user changes  │
│                                                                 │
│ [Stop server]  [Delete model]                                   │
│                                                                 │
│ ─── When "Local LLM" + installed + stopped: ─────────────────── │
│                                                                 │
│ (same as above but status shows "Stopped" and button is         │
│ [Start server] instead of [Stop server])                        │
│                                                                 │
│ ─── When "Local LLM" + installed + starting: ────────────────── │
│                                                                 │
│ (same layout but status shows "Starting..." with spinner.       │
│ This appears on app launch while the health check is running.   │
│ All buttons disabled during this state.)                        │
│                                                                 │
│ ─── When "Local LLM" + installed + restarting: ──────────────── │
│                                                                 │
│ (same as starting but status shows "Restarting..." — triggered  │
│ by context size change. Context select remains interactive.)    │
│                                                                 │
│ ─── When "Local LLM" + not installed: ───────────────────────── │
│                                                                 │
│ ┌─ Status ─────────────────────────────────────────────────────┐│
│ │ Not installed. The local model (Ministral 3B, 2.0 GB)        ││
│ │ runs entirely on your device for maximum privacy.            ││
│ │ Requires Apple Silicon.                                      ││
│ └──────────────────────────────────────────────────────────────┘│
│                                                                 │
│ [Download model]                                                │
│                                                                 │
│ ─── When "Local LLM" + downloading: ────────────────────────── │
│                                                                 │
│ ┌─ Status ─────────────────────────────────────────────────────┐│
│ │ Downloading Ministral 3B...                                  ││
│ │ ████████████░░░░░░░░  62% · 1.2 GB / 2.0 GB · 45 MB/s       ││
│ └──────────────────────────────────────────────────────────────┘│
│                                                                 │
│ [Cancel download]                                               │
│                                                                 │
│ ─── When "Off" selected: ────────────────────────────────────── │
│                                                                 │
│ (nothing else shown)                                            │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Password input component (`SettingPasswordInput`)

A new reusable setting component for sensitive string values. Design goals:

- Input with `type="password"` by default
- Toggle button (eye icon) to reveal/hide the value
- When hidden, show masked text (dots) but reveal the last 4 characters if the value is long enough — this helps users
  confirm they entered the right key (like `••••••••sk-1234`)
- Placeholder text: "Example: sk-abc123..." (per style guide: give examples in placeholder text)
- Paste-friendly: should work well with paste (no accidental trim, no character restrictions)
- Same visual style as other setting inputs (consistent borders, focus ring, sizing)
- Accessible: proper `aria-label`, the toggle button has a tooltip ("Show/Hide API key")

This component should be general-purpose (not AI-specific) so it can be reused for any future sensitive settings.

## Architecture

### New settings (registry)

| ID | Type | Default | Constraints | Component |
|----|------|---------|-------------|-----------|
| `ai.provider` | enum | `'local'` | `['off', 'openai-compatible', 'local']` | `SettingToggleGroup` |
| `ai.openaiApiKey` | string | `''` | — | `SettingPasswordInput` (new) |
| `ai.openaiBaseUrl` | string | `'https://api.openai.com/v1'` | — | text input |
| `ai.openaiModel` | string | `'gpt-4o-mini'` | — | text input |
| `ai.localContextSize` | enum | `'4096'` | `['2048', '4096', '8192', '16384', '32768', '65536', '131072', '262144']` | `SettingSelect` |

Section path: `['AI']` (flat, no subsections).

### Context window memory estimation

The KV cache scales linearly with context size. For Ministral 3B (26 layers, f16 KV):

- From empirical measurement: 262144 tokens → 26624 MiB KV cache
- Scaling factor: `ctx_size × 0.1016 MiB` for KV cache alone
- Add ~3.5 GB base overhead (model weights ~2 GB + Metal compute ~800 MB + host compute ~500 MB + misc)

| Context size | KV cache | Total estimated |
|-------------|----------|-----------------|
| 2048 | ~208 MB | ~3.7 GB |
| 4096 | ~416 MB | ~3.9 GB |
| 8192 | ~832 MB | ~4.3 GB |
| 16384 | ~1.7 GB | ~5.2 GB |
| 32768 | ~3.3 GB | ~6.8 GB |
| 65536 | ~6.7 GB | ~10.2 GB |
| 131072 | ~13.3 GB | ~16.8 GB |
| 262144 | ~26.6 GB | ~30.1 GB |

Display format: "Estimated memory use: ~3.9 GB" (updates live as user changes the select). The estimate is calculated
from a per-model constant stored in the model registry, not hardcoded in the frontend. This way it stays accurate if we
add models later.

The implementing agent should add a `kv_cache_bytes_per_token` field to `ModelInfo` in `mod.rs` and a
`base_overhead_bytes` constant. The frontend calls a Tauri command or calculates from model info returned by
`get_ai_model_info`.

### New Tauri commands

| Command | Returns | Purpose |
|---------|---------|---------|
| `configure_ai` | `()` | Frontend pushes full AI config to backend. Triggers server start in background if provider is `local` and model is installed. |
| `get_ai_runtime_status` | `AiRuntimeStatus` | Server PID, port, running state, model info, disk usage, hardware support |
| `stop_ai_server` | `()` | Gracefully stop llama-server without uninstalling |
| `start_ai_server` | `(ctx_size: u32)` | Start llama-server with the given context size (if model is installed) |

`configure_ai` accepts `{ provider, context_size, openai_api_key, openai_base_url, openai_model }`. On app startup, the
frontend calls this once after `initializeSettings()` completes. The backend stores all config in `ManagerState`. If
provider is `local` and the model is installed, it **spawns the server start as a background task** (via
`tauri::async_runtime::spawn`, same pattern as the current `init()`) and **returns immediately**. The frontend learns
about server readiness via the `ai-server-ready` event. This is important — `start_server_inner` takes 5-60s for health
check polling, and blocking `configure_ai` on that would freeze the frontend on startup.

The frontend also calls `configure_ai` whenever any AI setting changes at runtime (provider switch, OpenAI key update,
etc.). This is idempotent — the backend overwrites the stored config and takes appropriate action (start/stop server,
update OpenAI credentials).

`start_ai_server` takes `ctx_size` as a parameter directly from the frontend, avoiding any need for Rust to read
settings files. The frontend always knows the current value. Like `configure_ai`, it spawns the server start in the
background and returns immediately.

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRuntimeStatus {
    pub server_running: bool,
    pub server_starting: bool,  // true during health check polling
    pub pid: Option<u32>,
    pub port: Option<u16>,
    pub model_installed: bool,
    pub model_name: String,
    pub model_size_bytes: u64,
    pub model_size_formatted: String,
    pub download_in_progress: bool,
    pub local_ai_supported: bool,  // false on Intel Macs
    /// Bytes per token for KV cache, used by frontend to estimate memory
    pub kv_bytes_per_token: u64,
    /// Base memory overhead in bytes (model weights + compute buffers)
    pub base_overhead_bytes: u64,
}
```

### Renaming `use_real_ai()` → `is_local_ai_supported()`

The current `use_real_ai()` function checks two things: Apple Silicon hardware AND release/dev-mode gate. It is used as
a blanket gate on ALL AI commands.

Changes:
1. Rename to `is_local_ai_supported()`. Keep the same logic (aarch64 check + dev-mode gate).
2. Remove it from these commands (they work with any provider, on any hardware):
   - `get_ai_status` → check `ai.provider` instead
   - `get_ai_runtime_status` → always works (returns `local_ai_supported: false` on Intel)
   - `get_folder_suggestions` → routes based on `ai.provider`; only checks `is_local_ai_supported()` for the `local` path
   - `get_ai_model_info` → always works (informational)
   - `configure_ai` → always works
3. Keep it on these commands (local-only operations):
   - `start_ai_server` → return error if not supported
   - `start_ai_download` → return error if not supported
4. The frontend uses `AiRuntimeStatus.local_ai_supported` to disable the "Local LLM" toggle on Intel Macs.

### Backend changes to `client.rs`

Add an `AiBackend` enum and route requests accordingly:

```rust
pub enum AiBackend {
    Local { port: u16 },
    OpenAi { api_key: String, base_url: String, model: String },
}
```

The `chat_completion` function gains a `backend: &AiBackend` parameter. For `OpenAi`, it sends the request to
`{base_url}/chat/completions` with `Authorization: Bearer {api_key}` header and uses the configured model name.
For `Local`, it behaves as today (localhost:{port}, no auth).

### Backend changes to `suggestions.rs`

`get_folder_suggestions` checks provider (stored in manager state, pushed by `configure_ai`):
- `off` → return empty vec
- `local` → check `is_local_ai_supported()`, then use local llama-server (current behavior)
- `openai-compatible` → build `AiBackend::OpenAi` from manager state, call `chat_completion`

No hardware gate at the command entry point — the provider routing handles it.

### Backend changes to `manager.rs`

- `init()`: only sets up manager state (directory, stale PID cleanup). Does NOT start the server. Does NOT read settings.
- New `configure_ai(provider, context_size)` command: stores provider + context size in `ManagerState`. If provider is
  `local` AND model is installed AND `is_local_ai_supported()`, calls `start_server_inner(ctx_size)`. Called once by the
  frontend after settings load.
- Also store `openai_api_key`, `openai_base_url`, `openai_model` in `ManagerState` — the frontend pushes these too
  (via `configure_ai` or a separate `update_ai_openai_config` command) so `get_folder_suggestions` can read them from
  the manager without reading the settings file.
- New `stop_ai_server` command: calls `stop_process`, clears `child_pid`, but keeps model installed.
- New `start_ai_server(ctx_size)` command: calls `start_server_inner` if model installed. Takes context size as param.
- Provider change handling: frontend calls `stop_ai_server` when leaving `local`, calls
  `start_ai_server(ctx_size)` when entering `local`.
- The `opted_out` field in `AiState` / `ai-state.json` becomes legacy. `ai.provider` in the frontend settings store is
  the source of truth. The implementing agent can leave `opted_out` in the struct but stop checking it.
- Deprecate these existing commands (leave wired but stop using from new code): `dismiss_ai_offer`, `opt_out_ai`,
  `opt_in_ai`, `is_ai_opted_out`. They're superseded by the `ai.provider` setting. The onboarding toast still
  references them — they'll be cleaned up in the onboarding revamp that immediately follows this work.

### Backend changes to `process.rs`

`spawn_llama_server` gains a `ctx_size: u32` parameter, adds `-c {ctx_size}` to the args.

### Backend changes to `mod.rs`

Add memory estimation constants to `ModelInfo`:

```rust
pub struct ModelInfo {
    // ... existing fields ...
    /// Bytes per token for KV cache (used for memory estimation)
    pub kv_bytes_per_token: u64,
    /// Base memory overhead in bytes (model weights + compute buffers)
    pub base_overhead_bytes: u64,
}
```

For Ministral 3B: `kv_bytes_per_token = 106_496` (~0.1016 MiB), `base_overhead_bytes = 3_500_000_000` (~3.5 GB).

### Startup flow (replaces old `init()` auto-start)

```
Tauri setup()
  └─ ai::manager::init()          ← sets up dirs, cleans stale PIDs. Does NOT start server.

Frontend loads
  └─ initializeSettings()         ← loads settings from tauri-plugin-store
  └─ configureAi({                ← new: pushes AI config to backend
       provider: getSetting('ai.provider'),
       contextSize: getSetting('ai.localContextSize'),
       openaiApiKey: getSetting('ai.openaiApiKey'),
       openaiBaseUrl: getSetting('ai.openaiBaseUrl'),
       openaiModel: getSetting('ai.openaiModel'),
     })
       └─ backend: if provider === 'local' && model installed && local AI supported
            └─ start_server_inner(ctx_size)
            └─ emit 'ai-server-ready' when healthy
```

This is clean: Rust never reads settings files, the frontend is the single source of truth, and there's no timing
ambiguity. The ~500ms delay from frontend load time is negligible — users don't create new folders in the first
half-second.

### Context size change: live restart flow

When `ai.localContextSize` changes in the frontend:

1. Frontend saves the new value to settings store (immediate, with 500ms debounced disk write).
2. Frontend debounces 2 seconds before triggering a restart.
3. After debounce, frontend calls `stop_ai_server` then `start_ai_server(newCtxSize)`.
4. During restart, the status card shows "Restarting..." with a spinner.
5. If the user changes the value again during the debounce window, the timer resets.
6. If the user changes the value during an in-progress restart, `stop_ai_server` is called (idempotent — if already
   stopping, it's a no-op), then a new debounced restart is scheduled with the latest value.
7. The server being stopped mid-startup is fine — `stop_process` sends SIGTERM then SIGKILL after 5s.

### Provider switch: auto-stop/start flow

When `ai.provider` changes in the frontend:

- **To `off` or `openai-compatible`**: if server is running or starting, call `stop_ai_server`. Immediate, no debounce.
- **To `local`**: if model is installed, call `start_ai_server(ctx_size)`. If not installed, show "not installed" state.
- The toast download flow should write `ai.provider = 'local'` to settings when the user accepts the download offer.
- Also push updated OpenAI config to backend when those fields change (API key, base URL, model).

### Toast onboarding interaction

The toast in `initAiState()` must respect the provider setting:
- Read `ai.provider` from settings store
- Only show the toast if provider is `local` AND model is not installed AND not dismissed
- If provider is `off` or `openai-compatible`, never show the toast

### Delete model confirmation

Uses the app's existing confirmation dialog pattern (per style guide):
- Title: "Delete AI model?"
- Body: "This frees up 2.0 GB of disk space. You'll need to re-download it to use local AI again."
- Buttons: **Cancel** · **Delete**
- On confirm: calls `uninstall_ai`, updates status card to "not installed"
- On cancel: nothing happens

### Frontend: `AiSection.svelte`

Hybrid section like LicenseSection:
- `onMount`: calls `get_ai_runtime_status()` for dynamic state (including `local_ai_supported` for Intel check)
- Listens for `ai-server-ready`, `ai-download-progress`, `ai-install-complete`, `ai-starting` events to update live
- Uses registry settings for provider, API key, base URL, model, context size
- Conditionally renders provider-specific content based on `ai.provider` value
- Disables "Local LLM" toggle option when `!local_ai_supported`, with Intel-specific tooltip
- Computes live memory estimate from `kv_bytes_per_token × ctx_size + base_overhead_bytes`
- Watches `ai.provider` changes to auto-stop/start server
- Watches `ai.localContextSize` changes with 2s debounce to restart server
- Watches OpenAI config changes to push updates to backend

### Search integration

Add AI-related keywords to `settings-search.ts` special section matching (same pattern as License):
`'ai artificial intelligence llm model openai api key local llama server provider context memory'`

### Settings sidebar

Add `{ name: 'AI', path: ['AI'] }` to `specialSections` in `SettingsSidebar.svelte`, between License and Advanced.

## What this plan does NOT cover (future work)

- Keychain storage for API key
- Model selection UI (switching between Ministral 3B and Falcon H1R 7B)
- Onboarding flow rework (making OpenAI-compatible the default recommended path)
- Streaming responses

## Task list

### Milestone 1: `SettingPasswordInput` component

Build a high-quality, reusable password input component before the AI section needs it.

- [x] Create `SettingPasswordInput.svelte` in `src/lib/settings/components/`
- [x] Implement: `type="password"` input with eye toggle to reveal/hide
- [x] When hidden, mask all but last 4 chars (like `••••••••sk-1234`)
- [x] Placeholder text support (for "Example: sk-abc123...")
- [x] Same visual style as existing setting inputs (borders, focus ring, sizing)
- [x] Accessibility: `aria-label`, toggle button tooltip ("Show API key" / "Hide API key")
- [x] Wire to settings store: reads/writes via `getSetting`/`setSetting` like other components
- [x] Test: paste works, no character restrictions, empty state looks clean

### Milestone 2: Backend foundation

- [x] Rename `use_real_ai()` to `is_local_ai_supported()` in `mod.rs`; update all call sites
- [x] Remove `is_local_ai_supported()` gate from provider-agnostic commands (`get_ai_status`, `get_folder_suggestions`,
  `get_ai_model_info`, `get_ai_runtime_status`)
- [x] Keep `is_local_ai_supported()` gate on local-only commands (`start_ai_server`, `start_ai_download`)
- [x] Add `kv_bytes_per_token` and `base_overhead_bytes` fields to `ModelInfo` in `mod.rs`, populate for both models
- [x] Add `ctx_size: u32` param to `spawn_llama_server` in `process.rs`, wire `-c` arg
- [x] Add `ai.provider`, `ai.openaiApiKey`, `ai.openaiBaseUrl`, `ai.openaiModel`, `ai.localContextSize` to settings
  registry in `settings-registry.ts`
- [x] Add settings types to `SettingsValues` in `types.ts`
- [x] Add `configure_ai` Tauri command: stores provider + context size + OpenAI config in `ManagerState`, triggers
  server start if provider is `local` + model installed + hardware supported
- [x] Rework `init()` in `manager.rs`: only set up directories and clean stale PIDs, do NOT start server
- [x] Add `AiBackend` enum and refactor `chat_completion` in `client.rs` to accept it
- [x] Update `get_folder_suggestions` in `suggestions.rs` to route based on provider stored in manager state
- [x] Add `get_ai_runtime_status` (with `local_ai_supported` and `server_starting` fields), `stop_ai_server`,
  `start_ai_server(ctx_size)` Tauri commands
- [x] Register new commands in `lib.rs`
- [x] Add frontend startup call: `configureAi(...)` after `initializeSettings()` completes (in layout or app init)
- [x] Add Rust tests for new commands and backend routing

### Milestone 3: Frontend AI section

- [x] Create `AiSection.svelte` as a special section (hybrid: dynamic state + registry settings)
- [x] Add to `SettingsSidebar.svelte` special sections list (between License and Advanced)
- [x] Add to `SettingsContent.svelte` section routing
- [x] Add AI search keywords to `settings-search.ts`
- [x] Wire provider toggle (off / OpenAI-compatible / local LLM) with rich tooltips on each option
- [x] Disable "Local LLM" toggle on Intel Macs (greyed out + tooltip: "Local AI requires Apple Silicon (M1 or later)")
- [x] Implement provider switch auto-stop/start (stop server when leaving `local`, start when entering `local`)
- [x] Push OpenAI config changes to backend when API key, base URL, or model changes
- [x] Implement OpenAI-compatible config inputs (API key via `SettingPasswordInput`, base URL, model)
- [x] Implement local LLM status card (model name + size, server status with PID/port)
- [x] Handle all local LLM states: not installed, downloading, starting, running, stopped, restarting
- [x] Implement local LLM actions: start/stop server, download model, cancel download
- [x] Implement "Delete model" button with confirmation dialog
- [x] Wire download progress bar using existing `ai-download-progress` events
- [x] Implement context window size select with live memory estimate below it
- [x] Implement 2-second debounced server restart on context size change
- [x] Update toast logic: don't show AI toast when `ai.provider === 'off'`
- [x] Update toast download flow: write `ai.provider = 'local'` to settings when user accepts download

### Milestone 4: Polish and test

- [ ] Test provider switching: off → openai-compatible → local → off (verify server stops/starts appropriately)
- [ ] Test OpenAI-compatible flow: enter key, verify suggestions work in New Folder dialog (F7)
- [ ] Test local flow: download, start, stop, delete (with confirmation), re-download
- [ ] Test context size change: change value, verify 2s debounce, verify server restarts, verify new `-c` in logs
- [ ] Test rapid context size changes (change 3 times in 4 seconds — only one restart should happen)
- [ ] Test memory estimate updates live when changing context size
- [ ] Test search finds AI section with relevant queries (ai, openai, api key, llm, model, etc.)
- [ ] Test fresh install: no stored settings → `configure_ai` sends `local` but model not installed → no server start
- [ ] Test toast: doesn't appear when provider is `off`, does appear when provider is `local` + model not installed
- [ ] Test password input: paste works, show/hide toggle, last-4-chars reveal, empty state
- [ ] Test Intel Mac simulation: "Local LLM" toggle disabled, OpenAI path works, `is_local_ai_supported()` returns false
- [x] Update `CLAUDE.md` in `src-tauri/src/ai/` to document: `configure_ai` startup flow, `is_local_ai_supported()`
  replacing `use_real_ai()`, provider routing in suggestions, `opted_out` is legacy
- [x] Update `CLAUDE.md` in `src/lib/settings/` to add AiSection and SettingPasswordInput to the component/section lists
- [x] Run check suite: `./scripts/check.sh --check clippy --check rustfmt --check svelte-check --check desktop-svelte-eslint --check desktop-svelte-prettier --check rust-tests --check svelte-tests`

### Milestone 5: Cloud / API UX improvements

- [x] Create `cloud-providers.ts` with 15 provider presets (OpenAI, Anthropic, Google Gemini, Groq, Together AI,
  Fireworks AI, Mistral, OpenRouter, DeepSeek, xAI, Perplexity, Azure OpenAI, Ollama, LM Studio, Custom)
- [x] Rename toggle label from "OpenAI-compatible" to "Cloud / API"
- [x] Add `ai.cloudProvider` (enum) + `ai.cloudProviderConfigs` (JSON blob) settings with per-provider storage
- [x] Add migration from old flat `ai.openaiApiKey`/`ai.openaiBaseUrl`/`ai.openaiModel` to new schema
- [x] Build provider preset dropdown with read-only endpoint display (editable for Custom/Azure)
- [x] Add `check_ai_connection` Tauri command (validates endpoint + fetches model list)
- [x] Implement two-step connection check: auto-triggers on key/URL change (1s debounce), shows inline status
- [x] Build model combobox: dropdown from `/v1/models` response with type-ahead, text input fallback
- [x] Add `get_system_memory_info` Tauri command (macOS: sysctl + host_statistics64, Linux: /proc/meminfo)
- [x] Build RAM gauge: 4-segment stacked bar (other / cmdr current / projected / free) relative to system total
- [x] Add warning icons at >70% (orange) and >90% (red) projected RAM usage with tooltips
- [x] Replace auto-debounce with explicit "Apply" button for context size changes
- [x] Update CLAUDE.md files for new commands, components, and architectural changes
- [x] All checks pass: clippy, rustfmt, svelte-check, eslint, prettier, rust-tests, svelte-tests
