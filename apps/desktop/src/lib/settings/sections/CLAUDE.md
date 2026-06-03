# Settings sections

One Svelte component per UI section in the settings window. The registry decides which setting exists and what UI hint
it carries; these files decide where and how it renders. Section ↔ sidebar entry mapping is driven by
`getSettingDefinition(id).section`, which `SettingsContent.svelte` routes to the right component here.

Parents: [`../CLAUDE.md`](../CLAUDE.md) (registry, store, applier, search) and
[`../components/CLAUDE.md`](../components/CLAUDE.md) (the row primitives these sections compose).

## File map

| File                               | Responsibility                                                                                                                                                                                                                                                                                                                                                                                                                              |
| ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `AppearanceSection.svelte`         | `Appearance > Colors and formats`: theme mode, app color, size/date palettes, date/time format, striped rows                                                                                                                                                                                                                                                                                                                                |
| `AppearanceZoomSection.svelte`     | `Appearance > Zoom and density`: text size slider and UI density                                                                                                                                                                                                                                                                                                                                                                            |
| `AppearanceSizesSection.svelte`    | `Appearance > File and folder sizes`: size display, size unit (binary/SI drives `kB`↔`KB` label override), file size format, size mismatch warning                                                                                                                                                                                                                                                                                          |
| `ListingSection.svelte`            | `Appearance > Listing`: document icons, directory sort, brief column width                                                                                                                                                                                                                                                                                                                                                                  |
| `FileOperationsSection.svelte`     | `Behavior > File operations`: extension-change confirms (`maxConflictsToShow` / `progressUpdateInterval` live in Advanced)                                                                                                                                                                                                                                                                                                                  |
| `FileSystemWatchingSection.svelte` | `Behavior > File system watching`: three `SectionCard` sub-groups — Drive indexing (toggle + clear-index), Downloads notifications (4-option ToggleGroup, anchor `settings-downloads-notifications`), Go to latest download (a single on/off `Switch` whose description references the live global binding; the combo is edited under Keyboard shortcuts). FDA-closed greys sub-groups 2-3 with one shared hint linking to System Settings. |
| `SearchSection.svelte`             | `Behavior > Search`: auto-apply switch plus mirrored `recentSearches.maxCount` / `recentSelections.maxCount` rows from Advanced                                                                                                                                                                                                                                                                                                             |
| `AiSection.svelte`                 | `AI` wrapper: provider toggle (Off / Cloud / Local), auto-stops local server on switch-away, dispatches to one of the two sub-sections below                                                                                                                                                                                                                                                                                                |
| `AiCloudSection.svelte`            | Cloud provider config: preset dropdown, per-provider endpoint/model in `ai.cloudProviderConfigs`, API key in OS secret store, two-step connection check                                                                                                                                                                                                                                                                                     |
| `AiLocalSection.svelte`            | Local llama-server lifecycle, model install with multi-step tracking, context window "Apply" (server restart), RAM gauge, delete confirmation                                                                                                                                                                                                                                                                                               |
| `NetworkSection.svelte`            | `File systems > SMB/Network shares`: `network.enabled` master switch and the Local Network access info card                                                                                                                                                                                                                                                                                                                                 |
| `MtpSection.svelte`                | `File systems > MTP (Android/Kindle/cameras)`                                                                                                                                                                                                                                                                                                                                                                                               |
| `GitSection.svelte`                | `File systems > Git`                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `ViewerSection.svelte`             | `Viewer`                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `KeyboardShortcutsSection.svelte`  | `Keyboard shortcuts`: special (non-registry) section, renders the shortcut table from `shortcuts.json`, plus a bespoke `Global` group hosting `lib/downloads/GlobalShortcutRow.svelte` (the go-to-latest hotkey, marked `(global)`, binding stored in `settings.json` not `shortcuts.json`)                                                                                                                                                 |
| `McpServerSection.svelte`          | `Developer > MCP server`                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `LoggingSection.svelte`            | `Developer > Logging`                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `UpdatesSection.svelte`            | `Updates`: includes the `updates.errorReports` opt-in for Flow B (auto-send); Flow A consent-on-click is always available                                                                                                                                                                                                                                                                                                                   |
| `LicenseSection.svelte`            | `License`: special (non-registry), reads `getLicenseInfo` / `getLicenseStatus`                                                                                                                                                                                                                                                                                                                                                              |
| `AdvancedSection.svelte`           | `Advanced`: auto-generated rows for every registry entry with `showInAdvanced: true`. No custom UI per row                                                                                                                                                                                                                                                                                                                                  |
| `ai-secret-error.ts`               | Pure mapper from OS secret-store error variants to user-facing strings. Used by `AiCloudSection`                                                                                                                                                                                                                                                                                                                                            |
| `license-section-utils.ts`         | Pure label/status formatters extracted from `LicenseSection` for testability                                                                                                                                                                                                                                                                                                                                                                |
| `ram-gauge-utils.ts`               | Pure stacked-bar segment math for `AiLocalSection`'s memory gauge (used → projected → free, plus warning thresholds)                                                                                                                                                                                                                                                                                                                        |

Each section ships with an `*.a11y.test.ts` (axe-core tier-3). `McpServerSection`, `UpdatesSection`, `SearchSection`,
and `FileSystemWatchingSection` also have functional `*.test.ts` / `*.svelte.test.ts` files; the three pure-helper `.ts`
files have unit tests next to them. `FileSystemWatchingSection.svelte.test.ts` combines tier-3 axe with the functional
render contract since both share the same heavy IPC mock setup.

## Conventions

### Registry-driven section routing

Sections aren't picked by string match. `SettingsContent.svelte` maps the active sidebar entry to a component, and each
section renders its rows by explicit setting `id` through the primitives in `../components/`. If you add a new section,
add the route in `SettingsContent` and the top-level entry to `TOP_LEVEL_ORDER` in `SettingsSidebar.svelte` (also
mirrored in `apps/desktop/test/e2e-playwright/settings.spec.ts`).

### Mirroring a setting in multiple sections

A setting can appear in more than one section without duplicating it in the registry. Each `*Section.svelte` lists the
ids it wants to show (`getSettingDefinition('foo.bar')` + `shouldShow('foo.bar')` + a primitive); adding the same id to
a second section just makes it render there too.

What this buys you:

- Single source of truth for label, description, keywords, default, constraints, persistence.
- `searchSettings` walks the registry, not the UI tree, so each setting produces exactly one search hit (linking to its
  canonical section).
- `shouldShow(id)` returns `true` whenever the active query matches the id regardless of where it renders, so search
  filtering still works inside the mirror.

Edge case: the sidebar's "section contains a match" highlight reads only `setting.section`, so mirrors aren't
sidebar-highlighted. Mirror sections are discoverable by browsing. If a mirror genuinely needs a sidebar highlight, add
an optional `mirrorSections: SectionPath[]` to the registry and fold it into `getMatchingSectionsForSettings` — but ship
without it first; canonical-only highlight is usually right.

Live example: `appearance.sizeColors` is canonical under `Appearance > Colors and formats` (`AppearanceSection.svelte`)
and mirrored under `Appearance > File and folder sizes` (`AppearanceSizesSection.svelte`) because users hunt for it
under "file sizes" just as often.

### AI is a hybrid section (dynamic state + registry)

`AiSection.svelte` is a thin wrapper that loads `getAiRuntimeStatus()`, listens to backend events, and renders the
provider toggle, then conditionally mounts `AiCloudSection` or `AiLocalSection`. Cloud and local are independent feature
areas with their own state machines (`connectionStatus` for cloud, `installStep` for local); they share only the
`provider` toggle and `shouldShow` callback, passed in as props. `LicenseSection` follows the same pattern at smaller
scale.

## Gotchas

### Advanced section is auto-generated — don't hand-render

Anything tagged `showInAdvanced: true` shows up in `AdvancedSection` with generated UI. Don't add a custom row for it
elsewhere unless you're mirroring (see above) for discoverability. `advanced.maxLogStorageMb` (0 disables file logging
entirely, non-zero/zero swap or raising the cap needs a restart) and `fileExplorer.typeToJump.resetDelay` (live-applied
via `getTypeToJumpResetDelay()` on every keystroke) both live here.

### Cloud AI API keys never go through registry primitives

Cloud API keys live in the OS secret store via `saveAiApiKey` / `getAiApiKey`, not in `settings.json`.
`AiCloudSection.svelte` uses `SettingPasswordInput` in **controlled** mode (passes `value` + `onchange`) so the store
isn't touched. Error mapping flows through `ai-secret-error.ts`. See parent CLAUDE.md § "Why store cloud AI API keys in
the OS secret store".

### Hot-apply for AI provider/key/model is wired in the applier, not here

Settings AI changes hot-apply because `settings-applier.ts` routes `ai.provider`, `ai.cloudProvider`, and
`ai.cloudProviderConfigs` to `ai-config.ts::pushConfigToBackend()`, which re-reads everything fresh. Sections just call
`setSetting(...)`; don't try to push the AI config from the section component.

## Key decisions

### Section renamed from "Drive indexing" to "File system watching"

The umbrella section under `Behavior` was renamed because both the file-system indexer and the downloads watcher are
file-system watchers, and they share the same FDA gate. One header, one shared FDA hint, three `SectionCard` sub-groups
(Drive indexing, Downloads notifications, Go to latest download). The indexer setting still carries the label "Drive
indexing" — that's the per-toggle name and stays accurate; what changed is the umbrella section's name.

The sub-group sits inside a `<div id="settings-downloads-notifications">` so the downloads-toast "Stop showing these"
deep-link lands on the right row instead of the section top. `openSettingsWindow(section, anchor)` accepts an optional
`anchor` arg that the settings page (`routes/settings/+page.svelte`) reads from the URL on cold-open and from the
`navigate-to-section` event on already-open windows, then `scrollIntoView`s the matching element.

### Global go-to-latest hotkey: on/off here, combo edited in Keyboard shortcuts

The "Go to latest download" sub-group is a plain on/off `Switch`. The combo is edited under `Keyboard shortcuts`
(`lib/downloads/GlobalShortcutRow.svelte`, marked `(global)`), because that's where users look to rebind keys. We don't
fold it into the `commands` registry / `shortcuts.json` machinery: the binding's persistent home must stay in
`settings.json` so the Rust startup/focus refresh can read it before any window loads, and a global Carbon hotkey has no
in-app scope and doesn't travel through the keydown dispatch, so the scope/conflict apparatus doesn't apply. The
toggle's description references the live binding (via `global-shortcut-description.ts`) and updates when the user
rebinds. Both surfaces call the `set_global_go_to_latest_shortcut` IPC on change for live-apply.
