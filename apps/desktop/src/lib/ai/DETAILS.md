# AI features (frontend) details

Depth for the frontend AI module. `CLAUDE.md` holds the must-knows; this file holds the configuration wiring, wizard
reuse, model registry, and dev commands.

## Settings registry and config push

`ai.provider`, `ai.cloudProvider`, `ai.cloudProviderConfigs`, and `ai.localContextSize` are defined in
`settings-registry.ts`. The main layout calls `configureAi(...)` after `initSettingsApplier()` to push config to the
backend (the API key is fetched separately from the OS secret store).

The flat legacy keys (`ai.openaiApiKey`, `ai.openaiBaseUrl`, `ai.openaiModel`) are gone from the registry.
`ai-config.ts::migrateLegacyOpenAiKeys` lifts any stranded plaintext `ai.openaiApiKey` into the secret store and deletes
all three on startup.

The settings-applier listens for `ai.provider` / `ai.cloudProvider` / `ai.cloudProviderConfigs` changes and pushes fresh
config to Rust via `lib/settings/ai-config.ts::pushConfigToBackend()`.

## Wizard step 2 reuse

`lib/onboarding/StepAi.svelte` + `CloudProviderSetup.svelte` reuse the `checkAiConnection` / `saveAiApiKey` /
`getAiApiKey` pipeline from `lib/settings/sections/AiCloudSection.svelte` verbatim (1 s debounce, `/models` fetch,
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
