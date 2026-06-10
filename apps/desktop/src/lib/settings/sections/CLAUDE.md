# Settings sections

One Svelte component per UI section in the settings window. The registry decides which setting exists and what UI hint
it carries; these files decide where and how it renders. `SettingsContent.svelte` routes each sidebar entry to its
component via `getSettingDefinition(id).section`.

Parents: [`../CLAUDE.md`](../CLAUDE.md) (registry, store, applier, search) and
[`../components/CLAUDE.md`](../components/CLAUDE.md) (the row primitives these sections compose).

## Module map

- One `*Section.svelte` per sidebar entry (Appearance, Behavior, AI cloud/local, File systems, Viewer, Developer,
  Updates, License, Advanced, Keyboard shortcuts). Pure helpers: `ai-secret-error.ts`, `license-section-utils.ts`,
  `ram-gauge-utils.ts`, `keyboard-shortcuts-grouping.ts`, `keyboard-shortcuts-banner.ts`.
- The full file/responsibility table, test layout, and conventions are in DETAILS.md.

## Must-knows

- **Adding a section: register the route in `SettingsContent.svelte` AND the top-level entry in `TOP_LEVEL_ORDER`
  (`SettingsSidebar.svelte`), and mirror it in `apps/desktop/test/e2e-playwright/settings.spec.ts`.** Sections are
  picked by registry-driven routing, not string match.
- **`KeyboardShortcutsSection`'s "+ add" flow is UI-only state; never write a provisional `''` to the store.** Clicking
  `+` only sets `editingShortcut` at `index === length`; nothing reaches `shortcuts-store` until a key is confirmed.
  Every store mutator saves to disk AND broadcasts cross-window, so a placeholder `addShortcut(id, '')` leaks a real
  `['']` entry (framed `(none)` pills accumulate). The store has no concept of an empty shortcut.
- **macOS-native (`app.quit`/`hide`/`hideOthers`/`showAll`) and `FIXED_KEY_COMMAND_IDS` rows render read-only** (badge,
  no edit/add/remove/reset), keyed off `isNativeShortcutCommand` / `isFixedKeyCommand`. AppKit (or a hardcoded keydown
  handler) owns both behavior and accelerator, so an editable control would be a double illusion; the store refuses
  these writes too.
- **Conflict-banner honesty:** a native conflict (even mixed with a normal command) offers ONLY Cancel
  (`reservedByMacOsMessage`), no "Remove from other" / "Keep both" (both would lie). A fixed-key collision is also
  non-resolvable (Cancel only). Classification is the pure `classifyConflict` in `keyboard-shortcuts-banner.ts`; keep it
  there, don't inline string checks in the component.
- **`KeyboardShortcutsSection` groups one titled group per `CommandScope` via the pure `groupCommandsByScope`.** Don't
  reintroduce an ad-hoc title list matched against scopes: the group set must stay the scope union, or whole groups of
  commands silently vanish from the rebinding UI. The set-equality test in `keyboard-shortcuts-grouping.test.ts` is the
  guard (it also fails when a new `CommandScope` lacks a `scopeOrder` entry).
- **Cloud AI API keys never go through registry primitives.** `AiCloudSection` uses `SettingPasswordInput` in
  **controlled** mode (so the store isn't touched); keys live in the OS secret store via `saveAiApiKey` / `getAiApiKey`.
- **Don't push the AI config from a section.** Sections just call `setSetting(...)`; hot-apply is wired in
  `settings-applier.ts` → `ai-config.ts::pushConfigToBackend()` (re-reads fresh). See parent CLAUDE.md.
- **Don't hand-render anything tagged `showInAdvanced: true`** (it auto-renders in `AdvancedSection`), unless you're
  deliberately mirroring a setting in a second section for discoverability.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it in whole before structural changes here.
