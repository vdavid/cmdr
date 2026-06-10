# Settings system

Registry-based user settings for Cmdr: defined once in `settings-registry.ts`, accessed uniformly by UI and MCP.

## Module map

- `settings-registry.ts` (single source of truth), `settings-store.ts` (persistence + cache + cross-window sync),
  `settings-applier.ts` (applies side effects), `reactive-settings.svelte.ts` (`$state` for render-affecting settings).
- `sections/` (one component per UI section; see its CLAUDE.md), `components/` (row primitives; see its CLAUDE.md).
- Shortcuts are a separate subsystem (`shortcuts.json`); see `lib/shortcuts/CLAUDE.md`.

## Must-knows

- **Every setting MUST apply immediately without restart.** Adding a setting that changes backend behavior requires all
  three: (a) a Tauri command on the Rust side, (b) a typed wrapper in `$lib/tauri-commands/settings.ts`, (c) an
  `onSettingChange` case in `settings-applier.ts` that invokes it. Restart-required is a bug, not a design choice; even
  "structural" changes (reconnect, rebind, restart the worker) must live-apply. No exceptions.
- **Every `tauri-plugin-store` reader must go through `resolveStorePath(storeName)`** (`store-path.ts`). The plugin
  resolves a bare name against Tauri's identifier-driven `app_data_dir()`, which ignores `CMDR_DATA_DIR`; in isolated
  instances (dev, per-worktree, E2E) that reads the real production store. Applies to `settings.json`, `shortcuts.json`,
  and `app-status.json`. The backend sanitizes the name and can never escape the data dir.
- **The viewer window has NO store capability by design** (renders hostile file content). It runs
  `initializeSettings({ restrictedWindow: true })`: reads seed from `get_restricted_window_settings` (allowlist), writes
  forward through `persist_restricted_window_setting`. **Failures degrade to registry defaults with `log.warn`, never
  `log.error`** (an error-level log fires an auto error report on every viewer open). Never grant the viewer store
  permissions; extend the allowlist struct/enum instead.
- **Increment `SCHEMA_VERSION` and add a `migrateSettings()` case** when changing the settings format, or old files may
  crash on load.
- **Reactive settings must live in `reactive-settings.svelte.ts`** (`.svelte.ts`, not `.ts`): `$state()` needs the
  extension.
- **Date display has one source of truth:** `formatDateForDisplay()` (pure) → `formattedDate()` (reactive) →
  `<DateLabel>` (render). Per-component coloring lives only in `age-tier-utils.ts`. Add new date consumers through these,
  not a fresh formatter. Text-size compounding lives only in `text-size.ts`'s `computeAndApply()`.
- **AI hot-apply is wired in `settings-applier.ts`**, which routes `ai.provider` / `ai.cloudProvider` /
  `ai.cloudProviderConfigs` to `ai-config.ts::pushConfigToBackend()`. That helper re-reads every setting fresh, so
  callers MUST NOT pass cached values. Sections and the wizard just call `setSetting(...)`.
- **Cloud AI API keys live in the OS secret store, never `settings.json`** (via `saveAiApiKey` / `getAiApiKey`).
  `ai.cloudProviderConfigs` holds only non-secret `model` / `baseUrl`.
- **A self-closing webview must defer `close()` past the current event-loop tick** (`setTimeout(0)`, not `rAF`).
  Synchronous `close()` from a keydown handler stalls cross-webview IPC on Linux/webkit2gtk; `rAF` is throttled in
  unfocused windows on macOS. See DETAILS § Gotchas and `docs/testing.md` § "rAF in unfocused windows".

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it in whole before structural changes here.
