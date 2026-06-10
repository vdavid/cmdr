# AI features (frontend)

UI and state for AI-powered features (local LLM or OpenAI-compatible). Backend: `src-tauri/src/ai/` (download,
llama-server process, inference client with provider routing).

## Architecture

- **State**: `ai-state.svelte.ts`: Reactive AI status, download progress, Tauri event listeners
- **Toast wiring**: `ai-toast-sync.svelte.ts`: Reactively syncs state to toast via `$effect`
- **Toast content**: `AiToastContent.svelte`: Install flow UI (offer → downloading → installing → ready)
- **Translate-error toast**: `translate-error-toast.ts`: maps a typed AI-translation failure to friendly, actionable
  toast copy. Pure `aiTranslateErrorToast(kind)` + `isAiTranslateError` guard + `showAiTranslateErrorToast(err)` (the
  one impure wrapper). Both Search and Selection translate through the backend; when the call throws, `QueryDialog`'s
  shared catch calls `showAiTranslateErrorToast` so the user learns WHY (quota, key rejected, timeout, empty answer, …)
  instead of a silent no-op. Branches on `kind`, never the message (the `no-error-string-match` rule). Keep its switch
  in lockstep with the Rust `AiTranslateErrorKind` (`src-tauri/src/ai/translate_error.rs`).
- **Backend**: See `src-tauri/src/ai/` for model download, llama-server lifecycle, inference client

`ai-state.svelte.ts` manages state and exports handlers. `ai-toast-sync.svelte.ts` uses a `$effect` to reactively watch
`aiState.notificationState` and sync the toast; no manual notification needed after state mutations.
`AiToastContent.svelte` imports `getAiState` and handlers directly from `ai-state.svelte.ts`. No circular dependency
because `ai-state.svelte.ts` never imports from `ai-toast-sync.svelte.ts` or `AiToastContent.svelte`. Both are called
from `(main)/+layout.svelte`: `initAiToastSync()` synchronously in `onMount` (before the async IIFE), and
`initAiState()` inside the async IIFE. The toast sync runs first with initial state (`hidden` → no-op), then re-fires
reactively when `initAiState()` changes the notification state.

## Key decisions

### Apple Silicon only (local LLM)

Local LLM requires Apple Silicon. Intel Macs can use OpenAI-compatible provider. The "Local LLM" toggle is disabled on
Intel Macs with an explanatory tooltip (controlled by `AiRuntimeStatus.localAiSupported`).

### AI settings in registry

Settings `ai.provider`, `ai.openaiApiKey`, `ai.openaiBaseUrl`, `ai.openaiModel`, `ai.localContextSize` are defined in
`settings-registry.ts`. The main layout calls `configureAi(...)` after `initSettingsApplier()` to push config to
backend.

### The wizard owns AI consent

First-launch AI consent moved out of this module: `OnboardingWizard.svelte` (in `$lib/onboarding/`) step 2 is the only
path that flips `ai.provider` from `'off'` (the new default) to `'cloud'` or `'local'` on a fresh install. There is no
`offer` toast anymore. `ai-state.svelte.ts` only tracks runtime states (`downloading`, `installing`, `ready`,
`starting`) — the toast surfaces those when the wizard's local-pick kicks off a model download or when the server boots
on a returning launch. `pendingOffer` / `onboarded` / `notifyAiOnboardingComplete` are gone with the offer.

The settings-applier listens for `ai.provider` / `ai.cloudProvider` / `ai.cloudProviderConfigs` changes and pushes the
fresh config to Rust via `lib/settings/ai-config.ts::pushConfigToBackend()`. Same helper, same read-fresh semantics —
this means the wizard's step 2 doesn't need to know about backend wiring beyond `setSetting(...)`.

The onboarding wizard's step 2 (`lib/onboarding/StepAi.svelte` + `CloudProviderSetup.svelte`) reuses the
`checkAiConnection` / `saveAiApiKey` / `getAiApiKey` pipeline from `lib/settings/sections/AiCloudSection.svelte`
verbatim (1 s debounce, /models fetch, in-place model combobox). The pipeline itself is documented in
`lib/settings/CLAUDE.md` § "AiSection". Step 2 also calls `pushConfigToBackend()` explicitly on the "Start using Cmdr!"
/ "One more optional setup step" handlers to order the backend reconfigure ahead of the wizard's `onComplete()`.

### Model registry is extensible

`AVAILABLE_MODELS` constant in backend (`src-tauri/src/ai/mod.rs`) defines available models. Current default: Ministral
3B (~2.0 GB). Falcon H1R 7B still in registry as fallback.

### Folder suggestions: 10s timeout, graceful failure

When opening "New folder" dialog, calls `getFolderSuggestions()`. If LLM doesn't respond in 10s, returns empty array. UI
hides suggestions section (no error shown). Feature degrades gracefully.

### Download resumption via HTTP Range

Model download supports HTTP Range header for resume after interruption. No SHA256 verification (HuggingFace doesn't
provide checksums); file size check only.

## Gotchas

- **`initAiToastSync()` must be called synchronously in `onMount`**: It uses `$effect()`, which requires Svelte's
  reactive context. Calling it after an `await` (inside an async callback) causes `effect_orphan` because the reactive
  context is gone. It runs before `initAiState()` completes; the initial `$effect` fires with `hidden` state (no-op),
  then re-fires reactively when `initAiState()` changes the notification state.
- **Downloading toast remembers user dismissal**: `aiState.downloadToastUserDismissed` is set when the user clicks X on
  the downloading toast. While true, `ai-toast-sync.svelte.ts` won't re-add the toast even if the effect re-runs. The X
  only hides the toast; the Rust download loop in `manager.rs` (`do_download`) keeps going because nothing sets
  `cancel_requested`. Only the inline "Cancel" button calls `cancelAiDownload()` and aborts the actual download. The
  flag resets when a new download run starts (the wizard's local-pick triggers `startAiDownload()` again). Other state
  transitions (installing, ready, starting) ignore the flag and always render; they're fresh signals.
- **Status transitions are frontend-driven**: Backend emits `ai-download-progress` and `ai-install-complete` events.
  Frontend interprets these to update `aiStatus`. Backend has no "status" concept; just `AiState` (installed/port/pid).
- **llama-server is NOT auto-restarted**: Health monitoring (periodic restart on crash) is deferred. If server crashes,
  it stays down until app restart. User sees "AI unavailable."
- **Model switch requires app restart**: Changing selected model in Settings requires download + restart. No hot-swap.
- **`opted_out` flag is legacy**: The `opted_out` field in `AiState` is superseded by `ai.provider` setting. It remains
  in the struct but is no longer checked. `ai.provider` in the frontend settings store is the source of truth.
- **`resetForTesting()` must stay in sync with module state**: When adding new `$state` fields to `ai-state.svelte.ts`,
  update `resetForTesting()` to clear them. Tests use this instead of `vi.resetModules()` to avoid ~8s module re-parse
  penalty per test.

## Development

**Run with mock license**:

```bash
CMDR_MOCK_LICENSE=commercial pnpm tauri dev
```

**Test with AI debug logging**:

```bash
pnpm dev:ai-debug
```

**llama-server update**: See `apps/desktop/scripts/download-llama-server.go` (version + SHA256). Binaries are extracted
and signed at build time, bundled as individual files in `resources/ai/`. **Model registry**: See
`apps/desktop/src-tauri/src/ai/mod.rs` (`AVAILABLE_MODELS`) **Attribution**: Ministral 3B by Mistral AI (Apache 2.0),
attribution in About window
