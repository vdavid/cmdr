# Settings sections

One Svelte component per settings sidebar entry. The registry says which setting exists and its UI hint; these files
decide where and how it renders. `SettingsContent.svelte` routes each entry via `getSettingDefinition(id).section`.

Parents: [`../CLAUDE.md`](../CLAUDE.md) (registry, store, applier, search),
[`../components/CLAUDE.md`](../components/CLAUDE.md) (row primitives).

## Module map

- One `*Section.svelte` per sidebar entry. AI is a card-menu parent: `AiSection` = `AI › Provider`, `AskCmdrSection` =
  `AI › Ask Cmdr`. Pure helpers: `ai-secret-error.ts`, `license-section-utils.ts`, `ram-gauge-utils.ts`,
  `keyboard-shortcuts-grouping.ts`, `keyboard-shortcuts-banner.ts`. Full file/responsibility table in DETAILS.md.

## Must-knows

- **A registry entry alone doesn't render.** Hand-render the row here (`SettingRow` + control + `shouldShow(id)` guard),
  or the setting is invisible. Only `AdvancedSection` auto-renders (`section: ['Advanced']`).
  [Checklist](../../../../../../docs/guides/adding-a-new-setting.md).
- **New section = route in `SettingsContent.svelte` + entry in `TOP_LEVEL_ORDER` (`SettingsSidebar.svelte`) + mirror in
  `settings.spec.ts`.** Routing is registry-driven, not string match.
- **Don't push AI config from a section.** Just `setSetting(...)`; `settings-applier.ts` →
  `ai-config.ts::pushConfigToBackend()` hot-applies (re-reads fresh).
- **Cloud AI keys never touch registry primitives.** `AiCloudSection` uses `SettingPasswordInput` in controlled mode;
  keys live in the OS secret store (`saveAiApiKey` / `getAiApiKey`).
- **AI model picker (`ui/Combobox`) loads on open.** Don't zero `availableModels` mid-refetch (empty-list flash); never
  store or log the raw key (SHA-256 cache key). DETAILS § model picker.
- **Don't hand-render a `section: ['Advanced']` setting on a feature page.** It auto-renders in `AdvancedSection`; a
  setting's `section` is its ONE home. (The mirror pattern is for two FEATURE pages only.)
- **`KeyboardShortcutsSection` "+ add" is UI-only; never write a provisional `''` to the store.** Nothing hits
  `shortcuts-store` until a key is confirmed; a placeholder `addShortcut(id, '')` leaks framed `(none)` pills
  cross-window.
- **macOS-native + `FIXED_KEY_COMMAND_IDS` rows render read-only** (badge, no edit/add/remove/reset). AppKit or
  hardcoded handlers own them, and the store refuses these writes.
- **Conflict-banner honesty:** native or fixed-key conflicts offer ONLY Cancel (no "Remove from other" / "Keep both" —
  both would lie). Classify via the pure `classifyConflict`, not inline string checks.
- **One group per `CommandScope` via the pure `groupCommandsByScope`.** Don't ad-hoc a title list; the group set must
  stay the scope union or commands vanish. `keyboard-shortcuts-grouping.test.ts` guards it.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
