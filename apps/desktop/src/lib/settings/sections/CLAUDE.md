# Settings sections

One Svelte component per settings sidebar entry. The registry says which setting exists and its UI hint; these files
decide where and how it renders. Parents: `../CLAUDE.md` (registry, store, applier, search), `../components/CLAUDE.md`
(row primitives).

## Module map

- One `*Section.svelte` per sidebar entry, plus sibling `<Component>.rows.ts` search metadata and pure helpers.
- AI and Indexing are card-menu parents: `AiSection` / `AskCmdrSection` / `McpServerSection`, and `DriveIndexingSection`
  / `ImageIndexingSection` (which composes the `MediaIndex*` components).
- Full file-and-responsibility map: DETAILS § File map.

## Must-knows

- **Rendering a registry setting here means a `SettingRow` + control + `shouldShow(id)` guard**, hand-written; without
  it the setting is invisible. `AdvancedSection` is the one auto-renderer, so ❌ never hand-render a
  `section: ['Advanced']` setting on a feature page (`../CLAUDE.md`: a setting's `section` is its ONE home). Checklist:
  `docs/guides/adding-a-new-setting.md`.
- **A control whose value lives in the OS owns its own `SectionCard` and no registry entry** (`RevealHandlerCard`, the
  only one). It reads through on mount, renders whatever the write returns, and hides where the state can't exist, so it
  never appears in a dev build. Before adding a second one, read `../DETAILS.md` § OS-backed rows.
- **A row that isn't a setting declares a `SearchableRow` in the sibling `<Component>.rows.ts`.** Gate it on
  `shouldShow('row:…')` AND list it in its card's `anyVisible(...)`, or a hit filters every card away. It's search
  metadata; ❌ it never decides what renders. A page with NO control (`ServersSection`) reaches the sidebar via a row's
  `anchorsSection`. DETAILS § Searchable rows.
- **New section = a route in `../components/SettingsContent.svelte` (registry-driven, via
  `getSettingDefinition(id).section`, never a string match), an entry in `TOP_LEVEL_ORDER`
  (`../components/SettingsSidebar.svelte`), and a mirror in `test/e2e-playwright/settings.spec.ts`.**
- **A toggle that can't use `SettingSwitch` still uses `$lib/ui/Switch`**, never a hand-rolled Ark `Switch.Root` /
  `Control` (it ships without the primitive's ARIA) and never `:global(.switch-control)` from a section: those class
  names are the primitive's and leak app-wide.
- **`AiCloudSection`'s endpoint, key, and model controls are NOT ours: they live in `$lib/ai-provider-setup/`**, shared
  with the onboarding wizard, and this section keeps the service row, the recheck buttons, the toast, and the search
  gate. A per-provider copy or link change goes there, ❌ never here. Cloud AI keys still never touch registry
  primitives (the parent's `ai.*` bullet says why). DETAILS § "The setup steps are shared with onboarding".
- **`recheckAdbInstall` runs one call per CLICK** (`AdbSection`), ❌ never on mount or polled: it is the only path that
  retries `adb start-server`. Mount reads `getAdbInstallStatus`, which looks nothing up.
- **`KeyboardShortcutsSection` "+ add" is UI-only; never write a provisional `''` to the store** (a placeholder
  `addShortcut(id, '')` leaks framed `(none)` pills cross-window). macOS-native and `FIXED_KEY_COMMAND_IDS` rows render
  read-only, and their conflicts offer ONLY Cancel because the other options would lie; classify with the pure
  `classifyConflict`, never an inline string check. Groups come from `groupCommandsByScope`, one per `CommandScope`, or
  commands vanish.

Architecture, flows, conventions, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
