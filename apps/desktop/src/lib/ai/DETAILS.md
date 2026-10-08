# AI features (frontend) details

Depth for the frontend AI module. `CLAUDE.md` holds the must-knows; this file holds the configuration wiring, wizard
reuse, model registry, and dev commands.

## Cloud AI consent

The backend is the enforcer: every cloud call resolves through `ai::manager::resolve_backend`, which refuses without a
current consent record (`src-tauri/src/ai/DETAILS.md` § Cloud AI consent). This module mirrors the record so the UI can
render the switch and say why a feature is quiet.

- **State** (`cloud-consent.svelte.ts`): `cloudConsentState.accepted` is `null` / `false` / `true`, read from
  `cloud_ai_consent_status`; an unreadable status fails closed. The first `refreshCloudConsent()` in a window subscribes
  to `CloudAiConsentChanged`, so a flip in Settings reaches the main window's gates at once.
- **Accept / decline answer `done` / `notSaved`, never throw.** Accept is `done` only when the store then reads
  accepted, and lets go of a held "no" first. Decline (the switch's off, onboarding's "no AI") revokes, retries once,
  then HOLDS the "no" in the hidden `ai.cloudConsentRevokePending` (saved at once, then
  `cloud_ai_consent_revoke_pending_changed`), which every Rust cloud gate reads. `settleHeldCloudConsentRevoke()`
  retries the store on every refresh and as a main-window startup step. Revoking also stops in-flight cloud calls
  (backend).
- **The switch** (`AiCloudConsentToggle.svelte`) renders on Cloud only, in Settings > AI > Provider (with the
  `settings-ai-cloud-consent` anchor every "Open AI settings" button deep-links to, `openCloudConsentSettings(surface)`)
  and at the top of onboarding's Cloud column. Its disclosure (a native `<details>`, so its expanded state is announced)
  lists what every feature sends. The fold follows the switch: open while off, because people read it while deciding;
  folded when the switch turns on, so the locked setup below it comes into view (onboarding's Cloud column is where it
  was pushed off-screen); the user can reopen it any time (`AiCloudConsentToggle.test.ts`). The copy is versioned: a
  material change to `ai.cloudConsent.*` needs a `CLOUD_AI_CONSENT_VERSION` bump in the backend.
- **Only the user's click grants consent.** `acceptCloudConsent` may be imported by `AiCloudConsentToggle.svelte` alone
  (the one switch Settings and onboarding share); `cloud-consent-call-sites.test.ts` scans `src/` and fails on any other
  importer. MCP can't reach the record at all (it lives in `main.db`, behind commands no MCP tool calls).
- **Locked setup.** While blocked, `AiCloudSection` and onboarding's `CloudProviderSetup` are `inert` and dimmed, and
  their `ProviderSetupController` isn't pointed at the provider (that alone can start a connection check); it's pointed
  when the lock lifts. A check that does reach the backend answers `cloudConsentMissing`, which the controller reads as
  idle.
- **Entry points stay quiet or say why**: New folder skips its suggestion stream (no copy), Search and Select show
  `queryUi.ai.cloudOff.body` plus a button in the AI mode's empty state (the chip stays), a refused translate raises
  `CloudAiOffToastContent`, and the Ask Cmdr rail shows its cloud gate (`lib/ask-cmdr/DETAILS.md`).

## Under the organization's policy

The backend refuses every LLM request, model download, and server start the MDM policy rules out
(`src-tauri/src/ai/DETAILS.md`); this module renders the typed refusal, and every sentence comes from
`managedAiRefusalMessage` (`$lib/managed-policy/ai-refusal.ts`).

- **Consent**: `cloudConsentState.managed` mirrors `CloudAiConsentStatus.managed`. While it's set, the switch is
  disabled and reads off, with the refusal as a visible note its `aria-describedby` points at.
- **Translate**: a `managed` `AiTranslateErrorKind` carries the rule in `err.managed`; the toast body names it, and
  falls back to `ai.translateError.managed.body` when absent.
- **Local AI** (`local-ai-error.ts`): `start_ai_server` and `start_ai_download` reject with the typed `LocalAiError`
  (`managed` / `unsupported` / `cancelled` / `failed`). They're invoked raw, so `toLocalAiError` restores the type from
  `unknown`, and `localAiErrorLogLevel` logs `managed` and `cancelled` (a policy change cancels a download too) at info:
  an error-level log can send an automatic error report. Callers: `AiLocalSection` and onboarding's `StepAi`.

## Settings registry and config push

`ai.provider`, `ai.cloudProvider`, `ai.cloudProviderConfigs`, and `ai.localContextSize` are defined in
`settings-registry.ts`. The main layout calls `configureAi(...)` after `initSettingsApplier()` to push config to the
backend (the API key is fetched separately from the OS secret store).

The settings-applier listens for `ai.provider` / `ai.cloudProvider` / `ai.cloudProviderConfigs` changes and pushes fresh
config to Rust via `lib/settings/ai-config.ts::pushConfigToBackend()`.

## Wizard step 2 reuse

`lib/onboarding/StepAi.svelte` + `CloudProviderSetup.svelte` reuse the `checkAiConnection` / `saveAiApiKey` /
`getAiApiKeyStatus` pipeline from `lib/settings/sections/AiCloudSection.svelte` verbatim (1 s debounce, `/models` fetch,
in-place model combobox). The pipeline is documented in `lib/settings/DETAILS.md` § "AiSection". Step 2 calls
`pushConfigToBackend()` explicitly on its "Start using Cmdr!" / "One more optional setup step" handlers so the backend
reconfigure is ordered ahead of the wizard's `onComplete()`. The wizard's step 2 doesn't need backend wiring beyond
`setSetting(...)`.

## Model registry and download

- `AVAILABLE_MODELS` in `src-tauri/src/ai/mod.rs` defines available models. Current default: Ministral 3B (~2.0 GB);
  Falcon H1R 7B stays in the registry as a fallback. Attribution (Ministral 3B by Mistral AI, Apache 2.0) is in the
  About window.
- Model download supports the HTTP Range header to resume after interruption. No SHA256 verification (HuggingFace
  provides no checksums); a file-size check only.

## Dev commands

- Run with mock license: `CMDR_MOCK_LICENSE=commercial pnpm tauri dev`.
- AI debug logging: `pnpm dev:ai-debug`.
- llama-server update: `apps/desktop/scripts/download-llama-server.go` (version + SHA256). Binaries are extracted and
  signed at build time, bundled as individual files in `resources/ai/`.

## i18n

AI copy lives in the `ai.*` catalog (`$lib/intl/messages/en/ai.json`), resolved via `tString()` / `t()`;
`cmdr/no-raw-user-facing-string` is enforced on `lib/ai/`. The cloud/local AI settings sections
(`settings/sections/Ai{Cloud,Local}Section.svelte`) own their section-specific copy in `ai.cloud.*` / `ai.local.*`, and
reuse the registry settings keys (`settings.ai.*`) for rows that ARE registry settings (Service, Context window). The
download progress line is one ICU message (`ai.toast.progress`) with a `select` on `eta` (`'none'` discriminator when no
estimate) and preformatted size/speed STRING params. `translate-error-toast.ts` maps each `kind` to a `title`/`body`
pair of catalog keys, kept in lockstep with the Rust enum. Runtime rules: [`$lib/intl/CLAUDE.md`](../intl/CLAUDE.md).
