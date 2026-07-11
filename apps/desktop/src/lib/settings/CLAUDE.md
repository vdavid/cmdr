# Settings system

Registry-based user settings for Cmdr: defined once in `settings-registry.ts`, accessed uniformly by UI and MCP.

## Module map

- `settings-registry.ts` (single source of truth), `settings-store.ts` (persistence + cache + cross-window sync),
  `settings-applier.ts` (applies side effects), `reactive-settings.svelte.ts` (`$state` for render-affecting settings).
- `sections/` (one component per UI section; see its CLAUDE.md), `components/` (row primitives; see its CLAUDE.md).
- Shortcuts are a separate subsystem (`shortcuts.json`); see `lib/shortcuts/CLAUDE.md`.

## Must-knows

- **The registry stores i18n message KEYS, not English** (`labelKey` / `descriptionKey`, enum options `labelKey`); copy
  lives in `messages/en/settings.json`. `section: string[]` stays English (routing/search identity); titles render via
  `sectionTitle()`. Don't hardcode copy — `cmdr/no-raw-user-facing-string` is enforced on `lib/settings/`.
  [DETAILS.md](DETAILS.md) § i18n.
- **A registry entry alone does NOT render.** Adding a setting takes two steps: the `settings-registry.ts` entry AND a
  `SettingRow` in its `sections/*Section.svelte` (only `AdvancedSection` auto-renders, for `section: ['Advanced']`).
  Miss step two and the setting persists and is searchable but invisible. Checklist:
  [adding a new setting](../../../../../docs/guides/adding-a-new-setting.md).
- **A setting's `section` is its ONE home** (no `showInAdvanced` flag): either hand-rendered on its feature page OR
  auto-rendered in Advanced (`section[0] === 'Advanced'` + a `cardKey`), never both. The separate canonical/mirror
  pattern for a setting on two FEATURE pages is unrelated; see `sections/DETAILS.md`.
- **Every setting MUST apply immediately without restart.** A setting that changes backend behavior needs all three: (a)
  a Tauri command on the Rust side, (b) a typed wrapper in `$lib/tauri-commands/settings.ts`, (c) an `onSettingChange`
  case in `settings-applier.ts`. Restart-required is a bug; even "structural" changes (reconnect, rebind) must
  live-apply.
- **Every `tauri-plugin-store` reader must go through `resolveStorePath(storeName)`** (`store-path.ts`). The plugin
  resolves a bare name against `app_data_dir()`, which ignores `CMDR_DATA_DIR`; in isolated instances (dev,
  per-worktree, E2E) that reads the real production store. Applies to `settings.json`, `shortcuts.json`, and
  `app-status.json`. The backend sanitizes the name and can never escape the data dir.
- **The viewer window has NO store capability by design** (it renders hostile file content; see
  [`capabilities/CLAUDE.md`](../../../src-tauri/capabilities/CLAUDE.md)). It runs
  `initializeSettings({ restrictedWindow: true })`: seed from `get_restricted_window_settings` (allowlist), writes
  through `persist_restricted_window_setting`. **Failures degrade to registry defaults with `log.warn`, never
  `log.error`** (error-level auto-reports an error on every viewer open). Extend the allowlist struct/enum; never grant
  the viewer store permissions.
- **Persistence is sparse: `settings.json` holds ONLY keys an actor explicitly set.** "Explicit" is structural (which
  mutator ran, tracked in `explicitlySet`), NEVER `value !== default`. Don't seed defaults into the store or gate saves
  on a value compare; either re-opens the leak that pinned `developer.mcpEnabled`. `resetSetting` unsets (deletes the
  key). [DETAILS.md](DETAILS.md) § Sparse persistence.
- **Increment `SCHEMA_VERSION` and add a `migrateSettings()` case** when changing the settings format, or old files may
  crash on load. Adding a new key is additive (no bump).
- **Card visibility is section-owned**, NEVER re-derived from the registry `card` field (reintroduces the empty-card
  bug); `cardKey` is search metadata only. [DETAILS.md](DETAILS.md) § Card groups.
- **Reactive settings must live in `reactive-settings.svelte.ts`** (`.svelte.ts`, not `.ts`): `$state()` needs the
  extension.
- **Date and locale formatting has one source of truth:** `formatDateForDisplay()` (pure) → `formattedDate()` (reactive)
  → `<DateLabel>` (render); coloring lives only in `age-tier-utils.ts`. Add date consumers through these, not a fresh
  formatter. The `'system'` date and file-size decimals/grouping read the locale from `$lib/intl`'s `getLocale()`
  (iso/short/custom modes stay locale-independent); don't hardcode a locale. DETAILS § Date display;
  [`$lib/intl/CLAUDE.md`](../intl/CLAUDE.md).
- **AI hot-apply is wired in `settings-applier.ts`**, routing `ai.provider` / `ai.cloudProvider` /
  `ai.cloudProviderConfigs` to `ai-config.ts::pushConfigToBackend()`, which re-reads every setting fresh — callers MUST
  NOT pass cached values (sections and the wizard just call `setSetting(...)`).
- **Cloud AI API keys live in the OS secret store, never `settings.json`** (via `saveAiApiKey` / `getAiApiKey`).
  `ai.cloudProviderConfigs` holds only non-secret `model` / `baseUrl`.
- **A self-closing webview must defer `close()` past the current event-loop tick** (`setTimeout(0)`, not `rAF`):
  synchronous `close()` stalls cross-webview IPC on Linux/webkit2gtk; `rAF` is throttled in unfocused windows on macOS.
  DETAILS § Gotchas; `docs/testing.md` § "rAF in unfocused windows".

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
