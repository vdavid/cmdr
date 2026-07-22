# Settings system

Registry-based user settings: defined once in `settings-registry.ts`, accessed uniformly by UI and MCP.

## Module map

- `settings-registry.ts` (logic; data in `definitions/*.ts`), `settings-store.ts` (persistence + cache + cross-window
  sync), `settings-applier.ts` (side effects), `reactive-settings.svelte.ts` (`$state` for render-affecting settings).
- `sections/` (one component per UI section) and `components/` (row primitives) carry their own CLAUDE.md.
- Shortcuts are a separate subsystem (`shortcuts.json`); see `lib/shortcuts/CLAUDE.md`.

## Must-knows

- **The registry stores i18n message KEYS, not English** (`labelKey` / `descriptionKey`, enum options too); copy lives
  in `messages/en/settings.json`. `section: string[]` stays English (routing/search identity; titles render via
  `sectionTitle()`). `cmdr/no-raw-user-facing-string` is enforced here. `DETAILS.md` § i18n.
- **A registry entry alone does NOT render**: it also needs a `SettingRow` in its `sections/*Section.svelte` (only
  `AdvancedSection` auto-renders `section: ['Advanced']`). Miss that and the setting persists and is searchable but
  invisible. Checklist: [adding a new setting](../../../../../docs/guides/adding-a-new-setting.md).
- **A setting's `section` is its ONE home**: hand-rendered on its feature page OR auto-rendered in Advanced
  (`section[0] === 'Advanced'` + a `cardKey`), never both. (The canonical/mirror pattern for two FEATURE pages is
  unrelated; `sections/DETAILS.md`.)
- **Every setting MUST apply immediately without restart.** A backend-affecting one needs a Tauri command, a
  `$lib/tauri-commands/settings.ts` wrapper, AND an `onSettingChange` case in `settings-applier.ts`. Restart-required is
  a bug, even for "structural" changes (reconnect, rebind).
- **Every `tauri-plugin-store` reader goes through `resolveStorePath(storeName)`** (`store-path.ts`): the plugin
  resolves bare names against `app_data_dir()`, which ignores `CMDR_DATA_DIR`, so isolated instances (dev, worktree,
  E2E) would read the real production store. Applies to `settings.json`, `shortcuts.json`, `app-status.json`.
- **The viewer window has NO store capability by design** (it renders hostile file content). It runs
  `initializeSettings({ restrictedWindow: true })`: seeds from `get_restricted_window_settings` (an allowlist), writes
  through `persist_restricted_window_setting`, and degrades failures to registry defaults with `log.warn`, never
  `log.error` (error-level auto-reports on every viewer open). Extend the allowlist; never grant the viewer store
  permissions.
- **Persistence is sparse: `settings.json` holds ONLY keys an actor explicitly set.** "Explicit" is structural (which
  mutator ran, tracked in `explicitlySet`), NEVER `value !== default`. Don't seed defaults into the store or gate saves
  on a value compare — either re-opens the leak that pinned `developer.mcpEnabled`. `resetSetting` deletes the key.
  `DETAILS.md` § Sparse persistence.
- **Increment `SCHEMA_VERSION` and add a `migrateSettings()` case** when changing the settings format (adding a key is
  additive, no bump). Migrations must be idempotent: they re-run each launch until the first real save stamps the
  version.
- **Card visibility is section-owned**, never re-derived from the registry `card` field (reintroduces the empty-card
  bug); `cardKey` is search metadata only. `DETAILS.md` § Card groups.
- **Reactive settings live in `reactive-settings.svelte.ts`** (`$state()` needs the `.svelte.ts` extension).
- **Date/locale formatting has one source of truth**: `formatDateForDisplay()` (pure) → `formattedDate()` (reactive) →
  `<DateLabel>` (render); coloring only in `age-tier-utils.ts`. The `'system'` date mode and file-size decimals/grouping
  read `$lib/intl`'s `getLocale()`; don't hardcode a locale or add a fresh formatter. DETAILS § Date display.
- **AI hot-apply** routes `ai.provider` / `ai.cloudProvider` / `ai.cloudProviderConfigs` through `settings-applier.ts`
  to `ai-config.ts::pushConfigToBackend()`, which re-reads every setting fresh — never pass cached values (sections and
  the wizard just call `setSetting(...)`). The same four entries (plus `askCmdr.interactiveModel`) also nudge the Ask
  Cmdr rail's `noteModelSettingChanged()`, so a model switch lands in the open thread as a timeline event; see
  `lib/ask-cmdr/DETAILS.md` § Model-change events.
- **Cloud AI API keys live in the OS secret store, never `settings.json`** (`saveAiApiKey` / `getAiApiKey`);
  `ai.cloudProviderConfigs` holds only non-secret `model` / `baseUrl`.
- **A self-closing webview defers `close()` past the current tick** (`setTimeout(0)`, not `rAF`): synchronous `close()`
  stalls cross-webview IPC on webkit2gtk; `rAF` throttles in unfocused macOS windows. DETAILS § Gotchas.

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
