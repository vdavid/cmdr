# Settings system

Registry-based user settings: defined once in `settings-registry.ts`, accessed uniformly by UI and MCP.

## Module map

- `settings-registry.ts` (logic; data in `definitions/*.ts`), `settings-store.ts` (persistence, cache, cross-window
  sync), `settings-applier.ts` (side effects), `reactive-settings.svelte.ts` (`$state`), `window-settings.ts`
  (per-window init).
- `sections/` (UI) and `components/` (row primitives) carry their own CLAUDE.md; shortcuts are separate
  (`lib/shortcuts/CLAUDE.md`).

## Must-knows

- **The registry stores i18n message KEYS, not English** (`labelKey` / `descriptionKey`, enum options too); copy lives
  in `messages/en/settings.json`. `section: string[]` stays English: routing and search identity, not a title.
- **A registry entry alone does NOT render, and its `section` is its ONE home**: hand-render a `SettingRow` on the
  feature page, OR auto-render in Advanced (`section[0] === 'Advanced'` + a `cardKey`), never both.
  [Checklist](../../../../../docs/guides/adding-a-new-setting.md).
- **Every setting MUST apply immediately without restart.** A backend-affecting one also needs a Tauri command, a
  `$lib/tauri-commands/settings.ts` wrapper, and an `onSettingChange` case in `settings-applier.ts`.
- **Every `tauri-plugin-store` reader goes through `resolveStorePath(storeName)`** (`store-path.ts`): the plugin ignores
  `CMDR_DATA_DIR`, so a bare name makes an isolated instance read production.
- **The viewer and queue windows have NO store capability by design; never grant one.** Restricted mode seeds from a
  FIXED typed allowlist, so an unlisted setting reads as its default there. DETAILS § Restricted-window mode.
- **Persistence is sparse: `settings.json` holds ONLY keys an actor explicitly set.** "Explicit" is structural (which
  mutator ran), NEVER `value !== default` — seeding defaults or comparing values re-opens the `developer.mcpEnabled`
  leak. DETAILS § Sparse persistence.
- **Changing the settings FORMAT needs a `SCHEMA_VERSION` bump plus an idempotent `migrateSettings()` case** (a new key
  is additive, no bump). DETAILS § Schema version.
- **Card visibility is section-owned**, never re-derived from the registry `card` field (the empty-card bug); a row that
  isn't a setting is a `SearchableRow`, ❌ never a `hidden` setting. DETAILS §§ Card groups, Searchable rows.
- **A control whose value lives in the OS is an OS-backed row**: no registry entry, no store key, read through on every
  mount, and disabled with a reason (never hidden) where Cmdr may not change it. `RevealHandlerCard` is the only one.
  Reach for it ONLY when something outside Cmdr can change the value and nothing tells us. DETAILS § OS-backed rows.
- **Every window gets settings from `initWindowSettings()`** in the ROOT `routes/+layout.svelte`, ❌ never
  `initializeSettings()`: skip it and the window renders everything at its default. DETAILS § Per-window initialization.
- **Dates have one source of truth**: `formatDateForDisplay()` → `formattedDate()` → `<DateLabel>`. ❌ No second
  formatter, no hardcoded locale. DETAILS § Date display.
- **`ai.*` hot-applies via `ai-config.ts::pushConfigToBackend()`, which re-reads fresh**: callers `setSetting(...)`, ❌
  never pass cached values. A cloud API key lives in the OS secret store, never `settings.json` or a pre-filled field
  (`docs/security.md` § "AI API keys").
- **A self-closing webview defers `close()` via `deferWindowClose()`** (100 ms, ❌ never `0`/`rAF`): sync `close()`
  stalls webkit2gtk IPC, `0` segfaults macOS WebKit. DETAILS § Gotchas.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
