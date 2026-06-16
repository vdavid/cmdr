# AI features (frontend)

UI and state for AI features (local LLM or OpenAI-compatible cloud). Backend: `src-tauri/src/ai/` (model download,
llama-server process, inference client with provider routing).

## Module map

- **`ai-state.svelte.ts`**: reactive AI status, download progress, Tauri event listeners. Exports handlers.
- **`ai-toast-sync.svelte.ts`**: a `$effect` that watches `aiState.notificationState` and syncs the toast (no manual
  notify after state mutations).
- **`AiToastContent.svelte`**: install-flow UI (offer → downloading → installing → ready). Imports `getAiState` +
  handlers directly from `ai-state.svelte.ts`.
- **`translate-error-toast.ts`**: pure `aiTranslateErrorToast(kind)` + `isAiTranslateError` guard +
  `showAiTranslateErrorToast(err)` (the one impure wrapper).

No circular dependency: `ai-state.svelte.ts` never imports from the sync or content modules. Both are wired in
`(main)/+layout.svelte`: `initAiToastSync()` synchronously in `onMount`, `initAiState()` inside the async IIFE.

## Must-knows

- **Copy lives in the `ai.*` catalog**, resolved via `t()`/`tString()`; don't hardcode user-facing strings
  (`cmdr/no-raw-user-facing-string` is enforced on `lib/ai/` AND the cloud/local AI settings sections). See
  [DETAILS.md](DETAILS.md) § i18n.
- **Call `initAiToastSync()` synchronously in `onMount`**, before any `await`: it uses `$effect()`, which needs Svelte's
  reactive context. Calling it after an `await` throws `effect_orphan`. The initial `$effect` fires with `hidden` state
  (no-op), then re-fires when `initAiState()` updates the notification state.
- **The onboarding wizard owns first-launch AI consent**, not this module: `OnboardingWizard.svelte` step 2
  (`$lib/onboarding/`) is the only path that flips `ai.provider` from `'off'` (the default) to `'cloud'` / `'local'` on
  a fresh install. There is no `offer` toast. `ai-state.svelte.ts` tracks runtime states only (`downloading`,
  `installing`, `ready`, `starting`).
- **Keep `translate-error-toast.ts`'s switch in lockstep with the Rust `AiTranslateErrorKind`**
  (`src-tauri/src/ai/translate_error.rs`). Both Search and Selection translate through the backend; `QueryDialog`'s
  shared catch calls `showAiTranslateErrorToast` so the user learns why (quota, key rejected, timeout, empty answer).
  Branch on `kind`, never the message (`no-error-string-match`).
- **The downloading toast remembers user dismissal**: clicking X sets `aiState.downloadToastUserDismissed`, and while
  true the sync won't re-add the toast. X only HIDES it; the Rust download loop keeps running (nothing sets
  `cancel_requested`). Only the inline "Cancel" button calls `cancelAiDownload()` and aborts the real download. The flag
  resets when a new download run starts. Other transitions (installing, ready, starting) ignore the flag.
- **Status is frontend-derived**: the backend emits `ai-download-progress` / `ai-install-complete`; the frontend
  interprets them into `aiStatus`. The backend has no "status" concept, just `AiState` (installed/port/pid).
- **`resetForTesting()` must clear every `$state` field**: when adding a field to `ai-state.svelte.ts`, update
  `resetForTesting()` too. Tests use it instead of `vi.resetModules()` (avoids ~8s module re-parse per test).
- **`opted_out` in `AiState` is dead**: superseded by the `ai.provider` setting, which is the source of truth. It
  remains in the struct but is no longer checked.

## Behavior notes

- **Local LLM needs Apple Silicon**: Intel Macs use the cloud provider; the "Local LLM" toggle is disabled on Intel with
  a tooltip (gated on `AiRuntimeStatus.localAiSupported`).
- **llama-server is not auto-restarted**: a crash leaves AI down until app restart ("AI unavailable").
- **Switching the selected model needs download + app restart** (no hot-swap).
- **Folder suggestions degrade gracefully**: `getFolderSuggestions()` returns `[]` after a 10s timeout; the UI hides the
  section with no error.

Full details (settings registry + legacy-key migration, wizard reuse of the cloud pipeline, model registry, download
resumption, dev commands): [DETAILS.md](DETAILS.md).
