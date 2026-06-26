# Settings sections details

Pull-tier docs for `apps/desktop/src/lib/settings/sections/`: architecture, flows, and decision rationale. Must-know
invariants and gotchas live in [CLAUDE.md](CLAUDE.md).

One Svelte component per UI section in the settings window. The registry decides which setting exists and what UI hint
it carries; these files decide where and how it renders. Section ↔ sidebar entry mapping is driven by
`getSettingDefinition(id).section`, which `SettingsContent.svelte` routes to the right component here.

Parents: [`../CLAUDE.md`](../CLAUDE.md) (registry, store, applier, search) and
[`../components/CLAUDE.md`](../components/CLAUDE.md) (the row primitives these sections compose).

## File map

- **`AppearanceSection.svelte`**: `Appearance > Colors and formats`: theme mode, app color, size/date palettes,
  date/time format, striped rows
- **`AppearanceZoomSection.svelte`**: `Appearance > Zoom and density`: text size slider and UI density
- **`AppearanceSizesSection.svelte`**: `Appearance > File and folder sizes`: size display, size unit (binary/SI drives
  `kB`↔`KB` label override), file size format, size mismatch warning
- **`ListingSection.svelte`**: `Appearance > Listing`: document icons, directory sort, brief column width
- **`NavigationAndFileOpsSection.svelte`**: `Behavior > Navigation & file ops`: two labeled `SectionCard`s — Navigation
  (the `behavior.doubleClickPaneNavigatesToParent` switch) then File operations (the extension-change confirmation row
  `allowFileExtensionChanges`). The conflict/progress settings live ONLY in Advanced (`maxConflictsToShow`,
  `progressUpdateInterval` → `section: ['Advanced']`), never mirrored here. The hidden
  `behavior.doubleClickOnPaneNotificationSeen` flag (one-time-hint tracker) is registered but renders no row. Each card
  frame gated via `anyVisible(shouldShow, ...)` (same pattern as FSW above).
- **`FileSystemWatchingSection.svelte`**: `Behavior > File system watching`: three `SectionCard` card groups — Drive
  indexing (toggle + clear-index action, the hidden `indexing.indexSize` search anchor), Downloads (BOTH
  Downloads-folder features in one card: the 4-option `downloadsNotifications` ToggleGroup, plus the on/off go-to-latest
  `Switch` whose description references the live global binding — the combo is edited under Keyboard shortcuts; anchor
  `settings-downloads-notifications`), Low disk space (3-option ToggleGroup + percent number input, anchor
  `settings-low-disk-space`, NOT FDA-gated — statfs needs no TCC permission). FDA-closed greys the Downloads card (via
  `SectionCard`'s `gated` prop) with one shared hint linking to System Settings (the shared `<Trans>`
  `common.downloadsFdaHint`); Drive indexing stays interactive (it operates on whatever paths it can read; the gate is
  for the downloads watcher). **Card-group pattern (the empty-card / blank-page fix):** each `SectionCard` frame is
  wrapped in `{#if anyVisible(shouldShow, ...memberIds)}` over the card's member setting ids, reading the SAME
  `shouldShow` (`createShouldShow(searchQuery)`) the rows use, so a card whose rows all filter out under search hides
  its frame too (no empty cards). No wrapper component; card visibility is section-owned, never re-derived from the
  registry `card` field. The anchor ids now sit on `SectionCard`'s own `<section id>`, so the `navigate-to-section`
  deep-links still land. The hidden `indexing.indexSize` anchor (its `section` equals this page's) makes "index size" a
  search hit, and the index-size action row is gated on `shouldShow('indexing.indexSize')`, so searching it keeps the
  section visible (no blank pane) and shows the Drive-indexing card. See `lib/settings/components/CLAUDE.md` § card
  groups.
- **`SearchSection.svelte`**: `Behavior > Search`: one unlabeled `SectionCard` wrapping only the auto-apply switch. The
  `recentSearches.maxCount` / `recentSelections.maxCount` caps live ONLY in Advanced now, never mirrored here. The card
  is gated via `anyVisible(shouldShow, ...)` so an all-filtered-out search leaves no empty frame.
- **`AiSection.svelte`**: `AI` wrapper: provider toggle (Off / Cloud / Local), auto-stops local server on switch-away,
  dispatches to one of the two sub-sections below. The provider toggle row sits in its own unlabeled `SectionCard` (the
  row already carries a "Provider" label, so an unlabeled card avoids a duplicate heading). Card boundaries are a
  deliberate "tasteful, not one-big-card" choice: only the registry-row clusters are card-framed; the AI status blocks,
  gauge, action buttons, and the delete modal stay full-bleed (they already read as distinct blocks and don't belong
  inside a card).
- **`AiCloudSection.svelte`**: Cloud provider config: preset dropdown, per-provider endpoint/model in
  `ai.cloudProviderConfigs`, API key in OS secret store, two-step connection check. Its whole row list plus the
  connection-status block live in one unlabeled `SectionCard` (no `anyVisible` gate: the section mounts only when
  `provider === 'cloud'` and its rows aren't search-gated as a group).
- **`AiLocalSection.svelte`**: Local llama-server lifecycle, model install with multi-step tracking, context window
  "Apply" (server restart), RAM gauge, delete confirmation. Only the context-window registry-row cluster (`SettingRow`
  - the RAM gauge) is wrapped in an unlabeled `SectionCard`, and that wrapper sits INSIDE the
    `{#if modelInstalled && shouldShow('ai.localContextSize')}` guard, so no empty card renders before the model is
    installed. The `.status-card`, install/`.actions` buttons, and the body-level delete `ModalDialog` stay OUTSIDE any
    card on purpose (already visually distinct full-bleed blocks).
- **`NetworkSection.svelte`**: `File systems > SMB/Network shares`: two `SectionCard` card groups — Connection
  (`network.enabled` master switch + the inline Local Network access info block + `network.directSmbConnection`) and
  Performance and timeouts (`shareCacheDuration` select, `timeoutMode` radio with its inline custom-timeout number).
  `network.smbConcurrency` lives ONLY in Advanced now (`section: ['Advanced']`), not on this page. The info block's own
  heading is `<h4>` (the card's `<h3>` is the group heading). Card frames are gated via `anyVisible(shouldShow, ...)`
  (same pattern as FSW above).
- **`MtpSection.svelte`**: `File systems > MTP (Android/Kindle/cameras)`: one unlabeled `SectionCard`, gated via
  `anyVisible(shouldShow, ...)`
- **`GitSection.svelte`**: `File systems > Git`: one unlabeled `SectionCard`, gated via `anyVisible(shouldShow, ...)`
- **`ViewerSection.svelte`**: `Viewer`: one unlabeled `SectionCard`, gated via `anyVisible(shouldShow, ...)`
- **`KeyboardShortcutsSection.svelte`**: `Keyboard shortcuts`: special (non-registry) section, renders the shortcut
  table from `shortcuts.json` as one `SectionCard` per `CommandScope` (labelled with the scope title), plus a bespoke
  `Global` `SectionCard` hosting `lib/downloads/GlobalShortcutRow.svelte` (the go-to-latest hotkey, marked `(global)`,
  binding stored in `settings.json` not `shortcuts.json`). Empty-card hiding is FREE here: `groupCommandsByScope`
  already emits only non-empty groups (both the in-section filter and the global search flow through it), so no
  `anyVisible`/`shouldShow` plumbing is needed; the `Global` card's `{#if controller.showGlobalGoToLatestRow}` gate sits
  OUTSIDE its `SectionCard`. The cards stay INSIDE the `.commands-list` scroller (the deep-link does
  `row.closest('.commands-list')`, and the scrollbar-gutter logic lives on that element). Thin: markup + scoped styles +
  the capture-phase `document` keydown listener (and its `onMount` cleanup) + the deep-link highlight wiring; all
  business logic lives in its `*.controller.svelte.ts` (see below)
- **`KeyboardShortcutsSection.controller.svelte.ts`**: the section's business logic behind a
  `createKeyboardShortcutsController(getSearchQuery)` factory (per-mount `$state`/`$derived`, exposed via
  getters/setters so the markup's `bind:`s stay live). Holds the keyboard-capture + conflict engine
  (`handleKeyDown`/`saveShortcut`/`handleRemoveFromOther`/`handleKeepBoth`/`cancelEdit`, `editingShortcut`/`pendingKey`/
  `conflictWarning`/`confirmTimeout`, the `isAddingNewShortcut` derived), shortcut CRUD wrappers, the search/filter
  derivations (`filteredCommands`, `conflictingIds`/`conflictCount`, `showGlobalGoToLatestRow`, `groupedCommands`,
  filter `$state` + `resetFilters`), and the key-filter field helpers (`splitCombo`/`keyFilterMatches`/
  `formatModifiers`/`handleKeyFilterKeyDown`/`handleKeyFilterKeyUp`). It imports the pure `keyboard-shortcuts-grouping`
  and `keyboard-shortcuts-banner` helpers rather than duplicating them. The `getSearchQuery` arg is an accessor (not a
  snapshot) so the name-search derivation stays reactive to the parent-driven global search prop
- **`McpServerSection.svelte`**: `Developer > MCP server`: one unlabeled `SectionCard` wrapping the enable switch, port
  row, and the live port-status block, gated via `anyVisible(shouldShow, 'developer.mcpEnabled', 'developer.mcpPort')`
- **`LoggingSection.svelte`**: `Developer > Logging`: one unlabeled `SectionCard` wrapping the verbose-logging switch
  and the open-log/copy-diagnostics action buttons, gated via `anyVisible(shouldShow, ...)`
- **`UpdatesSection.svelte`**: `Updates & privacy`: two `SectionCard` card groups — Updates (the "Check for updates"
  action + status, `updates.autoCheck`, `whatsNew.showOnUpdate`) and Privacy and data sharing (the beta analytics
  opt-out `analytics.enabled` default-on, the `analytics.email` contact field with its "never sent with your usage data"
  note, and the `updates.crashReports` / `updates.errorReports` opt-ins — Flow B auto-send; Flow A consent-on-click is
  always available). The report opt-in logic and the beta-signup email flow are unchanged; the cards are presentation
  only. Frames are gated via `anyVisible(shouldShow, ...)` (same pattern as FSW above). The email field persists to
  settings here; the beta-signup network call is wired separately
- **`LicenseSection.svelte`**: `License`: special (non-registry), reads `getLicenseInfo` / `getLicenseStatus`. The info
  block + action buttons live in one unlabeled `SectionCard`; the personal / commercial / expired / loading states are
  presentational variants of that one block, all inside the one card (no `anyVisible` gate — it's not registry
  search-driven). `.license-info` dropped its own background / border / radius so the wrapping card is the only frame
  (no card-in-card)
- **`AdvancedSection.svelte`**: `Advanced`: auto-generated rows for every `section: ['Advanced']` registry entry (via
  `getAdvancedSettings()`), grouped into `SectionCard`s by `cardKey` via the pure `advanced-grouping.ts`
  (`groupAdvancedByCard`). No custom UI per row. Row visibility rides the SAME global `shouldShow`/`anyVisible` pipeline
  as every other section (Advanced settings are in the global search index), so an advanced term lights the Advanced
  sidebar entry and shows its row in its card. Advanced is a normal registry section now (in `buildSectionTree`), not a
  hardcoded special section
- **`advanced-grouping.ts`**: Pure card-grouping logic for `AdvancedSection` (group `section: ['Advanced']` settings by
  resolved `card` title, registry order; trailing untitled "Other" bucket for any with no `cardKey`). Tested by a
  set-equality regression guard (union of grouped === all Advanced settings)
- **`ai-secret-error.ts`**: Pure mapper from OS secret-store error variants to user-facing strings. Used by
  `AiCloudSection`
- **`license-section-utils.ts`**: Pure label/status formatters extracted from `LicenseSection` for testability
- **`ram-gauge-utils.ts`**: Pure stacked-bar segment math for `AiLocalSection`'s memory gauge (used → projected → free,
  plus warning thresholds)
- **`keyboard-shortcuts-grouping.ts`**: Pure scope→group logic for `KeyboardShortcutsSection` (one titled group per
  `CommandScope`, fixed order). Tested by the set-equality regression guard
- **`keyboard-shortcuts-banner.ts`**: Pure conflict-banner classification for `KeyboardShortcutsSection`
  (`classifyConflict` → native vs fixed vs normal, `classifySystemShortcut` → soft macOS-system warning with "Use
  anyway"; `reservedByMacOsMessage` / `fixedKeyMessage` / `systemShortcutMessage`). Native > fixed > normal in mixed
  sets; system checked only when no in-app conflict. Unit-tested

Each section ships with an `*.a11y.test.ts` (axe-core tier-3). `McpServerSection`, `UpdatesSection`, `SearchSection`,
`FileSystemWatchingSection`, and `KeyboardShortcutsSection` also have functional `*.test.ts` / `*.svelte.test.ts` files;
the pure-helper `.ts` files have unit tests next to them. `FileSystemWatchingSection.svelte.test.ts` combines tier-3 axe
with the functional render contract since both share the same heavy IPC mock setup.
`KeyboardShortcutsSection.svelte.test.ts` runs the real `$lib/shortcuts` store against an in-memory disk (mocks only the
Tauri boundaries, like `shortcuts-store.test.ts`) and drives the add/edit/conflict flows through the DOM.
`KeyboardShortcutsSection.controller.svelte.test.ts` instantiates the controller factory directly via `$effect.root`
(store + registry mocked) to cover the units the DOM test doesn't reach: the key-filter field helpers (platform-aware
combo splitting and subset matching) and the search/filter derivations.

## Conventions

### Registry-driven section routing

Sections aren't picked by string match. `SettingsContent.svelte` maps the active sidebar entry to a component, and each
section renders its rows by explicit setting `id` through the primitives in `../components/`. If you add a new section,
add the route in `SettingsContent` and the top-level entry to `TOP_LEVEL_ORDER` in `SettingsSidebar.svelte` (also
mirrored in `apps/desktop/test/e2e-playwright/settings.spec.ts`).

### Mirroring a setting in multiple sections

A setting can appear on more than one FEATURE page without duplicating it in the registry. Each `*Section.svelte` lists
the ids it wants to show (`getSettingDefinition('foo.bar')` + `shouldShow('foo.bar')` + a primitive); adding the same id
to a second section just makes it render there too. This is ONLY for a setting that genuinely belongs on two feature
pages — never to surface an Advanced setting (`section: ['Advanced']`) on a feature page. A setting's `section` is its
one home; Advanced auto-renders its own.

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

Every `section: ['Advanced']` setting shows up in `AdvancedSection` with generated UI, grouped into `SectionCard`s by
its `cardKey` (every advanced setting carries one; `advanced-grouping.ts` is the pure grouper, set-equality-tested).
There is no `showInAdvanced` flag and no mirroring of an Advanced setting onto a feature page: `section` is the single
home. Don't add a custom row for an Advanced setting elsewhere. `advanced.maxLogStorageMb` (0 disables file logging
entirely, non-zero/zero swap or raising the cap needs a restart) and `fileExplorer.typeToJump.resetDelay` (live-applied
via `getTypeToJumpResetDelay()` on every keystroke) both live here.

Advanced settings are in the GLOBAL search index (there's one index, no Advanced-specific one), so they're findable from
the main settings search and the page rides the same `shouldShow` per-row + `anyVisible` per-card pipeline as every
other section. Advanced is a normal section in `buildSectionTree`, so its sidebar entry comes from the tree via
`TOP_LEVEL_ORDER` like any other. A new advanced setting MUST get a `cardKey`, or the set-equality guard in
`advanced-grouping.test.ts` flags it (it lands in a trailing untitled "Other" card). The Advanced-only settings each
render here in exactly one place: `network.smbConcurrency` under "Network and mounts";
`fileOperations.maxConflictsToShow` / `progressUpdateInterval` under "File operations"; `search.recentSearches.maxCount`
/ `selection.recentSelections.maxCount` under "History and limits".

### Cloud AI API keys never go through registry primitives

Cloud API keys live in the OS secret store via `saveAiApiKey` / `getAiApiKey`, not in `settings.json`.
`AiCloudSection.svelte` uses `SettingPasswordInput` in **controlled** mode (passes `value` + `onchange`) so the store
isn't touched. Error mapping flows through `ai-secret-error.ts`. See parent DETAILS.md § "Why store cloud AI API keys in
the OS secret store".

### Hot-apply for AI provider/key/model is wired in the applier, not here

Settings AI changes hot-apply because `settings-applier.ts` routes `ai.provider`, `ai.cloudProvider`, and
`ai.cloudProviderConfigs` to `ai-config.ts::pushConfigToBackend()`, which re-reads everything fresh. Sections just call
`setSetting(...)`; don't try to push the AI config from the section component.

### The model picker loads on open and caches across reopens

`AiCloudSection` renders the model field through the shared `$lib/ui/Combobox` (a text-field-with-suggestions, not a
value-bound select), fed by `availableModels`. On mount, after the saved key resolves from the secret store,
`populateModelsOnOpen()` runs: a warm hit from `$lib/settings/ai-model-cache.ts` populates the list instantly; a cold
miss schedules the same debounced connection check the key/URL editors use, and a successful check writes the result
back into the cache. The cache key is a SHA-256 digest of `providerId \0 baseUrl \0 apiKey` (collision-free across
equal-length keys, so a revoked-vs-new key can't serve a stale list); the raw key and the digest input are never stored
or logged.

Two things keep the field honest, both load-bearing:

- **`triggerConnectionCheck()` must NOT zero `availableModels` at the start of a refetch.** The field text is
  `inputValue`-driven (the saved/typed model), but flashing an empty suggestion list mid-check is the regression we
  forbid. A genuine config change (provider switch) still drops the list via `resetConnectionState()`.
- **The mount-trigger fires in dev and prod, suppressed only in automated E2E (`getAppMode() === 'e2e'`).** E2E has no
  real provider, and for no-key providers (`custom`/`ollama`/`lm-studio`) `hasCheckableConfig` is true with just the
  preset base URL, so an unguarded trigger would add network flakiness there. Warm cache hits still work everywhere,
  including E2E (no network). The mount-trigger also bails when a check is already scheduled or in flight, so it can't
  double-fire with `handleCloudProviderChange`'s `setTimeout(0)` check.

`CloudProviderSetup` (onboarding) uses the same `ui/Combobox` but gets **no** mount-trigger: it already loads on open
(`loadApiKeyForProvider` triggers a check when a stored key resolves), so a second trigger would double-fire. The
session cache is process-lifetime and shared by both consumers.

### Every command groups by scope (one group per `CommandScope`)

`KeyboardShortcutsSection` renders one `SectionCard` per `CommandScope` (labelled with the scope title), in a fixed
reading order, via the pure `groupCommandsByScope` (`keyboard-shortcuts-grouping.ts`). Compound scopes
(`'Main window/File list'`, `'Main window/Brief mode'`, `'Main window/Volume chooser'`, …) each become their own card
titled by the last segment ("File list", "Brief mode", …). So every registry command lands in exactly one card and is
rebindable here, including `file.quickLook` and the F-key commands. Don't reintroduce an ad-hoc title list matched
against scopes: the group set must stay the scope union, or whole groups of commands silently vanish from the rebinding
UI. The `keyboard-shortcuts-grouping.test.ts` set-equality test (union of grouped commands === all registry commands) is
the guard; it also fails if a new `CommandScope` is added without a `scopeOrder` entry.

Empty cards never render under filtering: `groupCommandsByScope` drops empty groups, and BOTH the in-section name/key
filter and the global settings search flow through it (the filtered command list feeds the grouping). So a scope's card
shows iff it has ≥1 visible row, automatically, under both — no `anyVisible`/`shouldShow` plumbing is needed here
(commands aren't registry settings). The `SectionCard`s sit inside the `.commands-list` scroller so the deep-link's
`row.closest('.commands-list')` and the scrollbar-gutter still resolve; the row id (`shortcutAnchorId`) stays on the
`.command-row`, not the card.

The bespoke `Global` card (`lib/downloads/GlobalShortcutRow.svelte`) is a `SectionCard` too, gated by
`{#if controller.showGlobalGoToLatestRow}` OUTSIDE the card so it never renders empty. `GlobalShortcutRow` renders only
its row (no heading of its own): the `SectionCard`'s `<h3>` is the sole "Global" heading, so adding a heading back would
duplicate the label and break heading order.

Deep links to compound-scope rows now land + flash like any other (`shortcut-file.quickLook` from the Quick Look toast,
F-key chips from the F-bar).

## The add slot is UI-only (never write a provisional `''`)

`KeyboardShortcutsSection`'s "+ add" flow is pure UI state, NOT a store mutation. Clicking `+` only sets
`editingShortcut = { commandId, index: getEffectiveShortcuts(id).length }` — one past the end — and the template renders
a synthetic editing pill at that slot. Nothing reaches `shortcuts-store` until a key is captured and confirmed
(`saveShortcut` calls `addShortcut(id, combo)` for the add slot, `setShortcut(id, index, combo)` for an existing pill).

Why it MUST stay UI-only: every store mutator saves to disk AND broadcasts cross-window (see `lib/shortcuts/CLAUDE.md`).
Calling `addShortcut(id, '')` the instant `+` is clicked means any exit that isn't Escape/Backspace (clicking another
pill, clicking `+` on another row, clicking away) leaks a real `['']` entry to disk and to other windows — the user sees
framed `(none)` pills accumulate (one per leak). With the add slot UI-only, every cancel/exit path just resets
`editingShortcut`; there's nothing in the store to clean up. `isAddingNewShortcut` derives from
`index === getEffectiveShortcuts(id).length` (no `''` sentinel). The duplicate-on-same-action path simply cancels.

If you ever need to persist a placeholder mid-edit, don't — the store has no concept of an empty shortcut, and
`initializeShortcuts` actively heals leaked `''` entries on load (the matrix lives in `lib/shortcuts/CLAUDE.md`).

Known accepted edge: with the add slot open at `index === length`, a cross-window broadcast that ADDS a shortcut to the
same command bumps the length, so the stale `editingShortcut` now points at the remotely added pill — a key captured
after that lands as an overwrite of that pill instead of an append. It needs a precise two-window race on the same
command, the result is visible immediately, and any fix (re-deriving the slot on `shortcutChangeCounter` bumps) costs
more state than the race is worth. Revisit only if it shows up in practice.

## macOS-native and fixed-key rows are read-only

The four `nativeShortcut` commands (`app.quit`/`hide`/`hideOthers`/`showAll`) render read-only: their combos show as
plain `.shortcut-pill.static` spans (no click-to-edit), with no `+` add, no `×` remove, no reset button, and never the
add slot. Each native row also carries a small "macOS" badge (`.readonly-badge`) with a tooltip: "macOS handles this
shortcut. Cmdr can't change it." (`Show all` has no default binding, so it renders its `(none)` unframed plus the
badge.) The branch is keyed off `isNativeShortcutCommand(command.id)` from `$lib/shortcuts`. This is honest: AppKit owns
both the behavior and the accelerator (see `lib/shortcuts/DETAILS.md` § "macOS-native commands are not customizable"),
so an editable control here would be a double illusion. The store also refuses these writes as defense in depth, so the
UI and the store agree.

The `FIXED_KEY_COMMAND_IDS` rows (nav arrows, palette navigation, modal Enter/Escape — `isFixedKeyCommand(command.id)`)
get the same read-only treatment with a "Fixed" badge ("This key is built into Cmdr and can't be changed.") and share
the `.readonly-badge` style with the macOS badge. Their keys are hardcoded in the owning component's keydown handler and
never consult the shortcuts store, so an editable row would be the same double illusion. The conflict banner treats a
captured combo that collides with a fixed key as non-resolvable (Cancel only, via `classifyConflict`'s `'fixed'` kind):
"Remove from other" would be refused by the store and "Keep both" would race a key that always fires.

## Conflict banner: native conflicts are honest (reserved by macOS), others are resolvable

`handleKeyCapture` classifies the captured combo's conflicts via the pure `classifyConflict`
(`keyboard-shortcuts-banner.ts`):

- If the conflict set includes a `nativeShortcut` command (even in a MIXED set with a normal command — the native wins,
  because the combo is unusable regardless), the banner reads
  `⌘H is reserved by macOS (Hide Cmdr) and won't reach Cmdr. Pick a different combo.` (`reservedByMacOsMessage`) and
  offers ONLY Cancel. No "Remove from other" (removing Cmdr's binding doesn't free the OS accelerator) and no "Keep
  both" (the user's binding would never fire) — both would be a lie.
- A purely non-native conflict keeps the resolvable banner (Remove-from-other / Keep-both / Cancel).

`conflictWarning` carries `{ shortcut, conflict: ConflictKind }`; the template branches on `conflict.kind`. The
classification logic is extracted to the pure `keyboard-shortcuts-banner.ts` (unit-tested) to keep the section component
lean.

## Conflict banner: the editing pill reads as a pending decision

When a captured combo conflicts, `handleKeyCapture` sets `conflictWarning` and returns without saving (the banner offers
Remove-from-other / Keep-both / Cancel). The editing pill keeps showing the proposed combo — honest, it IS the proposed
combo — but gains `class:pending-conflict` (warning-tinted) so it doesn't read as a saved binding sitting next to the
banner. Pressing more keys re-evaluates; choosing Cancel (or Escape) exits via `cancelEdit`; and clicking a different
pill or `+` on another row dismisses the banner (those handlers reset `conflictWarning`).

## Deep-link arrival into a shortcut row

A clickable `ShortcutChip` (and the Quick Look toast's "Settings > Keyboard shortcuts" link) deep-links to a command's
row via `openShortcutCustomization(commandId)` (`../settings-window.ts`), which opens
`openSettingsWindow(['Keyboard shortcuts'], shortcutAnchorId(commandId))`. The arrival behavior lives across three
files; the pieces are load-bearing in this order:

1. **Row id**: each `.command-row` carries `id={shortcutAnchorId(command.id)}` (i.e. `shortcut-<commandId>`) on the
   keyed element, so the id survives the `shortcutChangeCounter` re-keying. `GlobalShortcutRow` (the `(global)`
   go-to-latest hotkey) gets NO anchor — its binding lives in `settings.json`, not the registry, and it isn't a
   deep-link target.
2. **Filter clear**: a leftover filter (the `Modified`/`Conflicts` chip, the name search, the key search) can keep the
   target row out of the DOM. The section registers a resetter via `registerShortcutFilterReset`
   (`../pending-shortcut-highlight.svelte.ts`); the settings page calls `resetShortcutFilters()` BEFORE it scrolls.
3. **Scroll the nested list, not the outer content**: the rows live inside `.commands-list`, an `overflow-y: auto`
   scroller, so `routes/settings/+page.svelte`'s default `contentElement.scrollTo` can't reach them.
   `scrollAnchorIntoView` branches on `commandIdFromShortcutAnchor`: shortcut anchors scroll the inner `.commands-list`
   (via the live rect delta, not `offsetTop`), leaving the outer settings layout / drag region put; everything else
   keeps the old `contentElement.scrollTo` path. The sequence is clear filters → `await tick()` (the row mounts only
   after Svelte flushes the `$derived` filter state) → `setTimeout(0)` (defer past the handler, and stay off `rAF` for
   the unfocused-window throttle, see `docs/testing.md`) → scroll + flash.
4. **Flash**: the page calls `setPendingShortcutHighlight(commandId)`; the section reads it via a `$derived` and applies
   `class:flash` on the matching row, clearing the state after the ~1.5 s animation. Two gentle `--color-accent-subtle`
   pulses (a static fade under `prefers-reduced-motion`). State-driven, NOT a direct DOM class: the rows re-key on
   `shortcutChangeCounter`, so an imperative class would vanish on the next re-render. Both ends import
   `pending-shortcut-highlight.svelte.ts` (the page writes, the section reads + registers the resetter) so knip doesn't
   flag either export.

## Key decisions

### Why "File system watching" is one umbrella section

`File system watching` under `Behavior` covers both the file-system indexer and the downloads watcher because both are
file-system watchers and share the same FDA gate. One header, one shared FDA hint, three `SectionCard` card groups:
Drive indexing, Downloads, Low disk space. The indexer's own toggle still carries the label "Drive indexing" — that's
its per-toggle name, distinct from the umbrella section title.

The Downloads card carries `id={DOWNLOADS_NOTIFICATIONS_ANCHOR_ID}` (on `SectionCard`'s own `<section>`, value
`settings-downloads-notifications`) so the downloads-toast "Stop showing these" deep-link lands on the card instead of
the section top; the Low disk space card carries `LOW_DISK_SPACE_ANCHOR_ID` the same way for its warn-toast link.
`openSettingsWindow(section, anchor)` accepts an optional `anchor` arg that the settings page
(`routes/settings/+page.svelte`) reads from the URL on cold-open and from the `navigate-to-section` event on
already-open windows, then `scrollIntoView`s the matching element.

### Global go-to-latest hotkey: on/off in the Downloads card, combo edited in Keyboard shortcuts

The go-to-latest toggle is a plain on/off `Switch` inside the Downloads card (alongside the downloads-notifications
ToggleGroup). The combo is edited under `Keyboard shortcuts` (`lib/downloads/GlobalShortcutRow.svelte`, marked
`(global)`), because that's where users look to rebind keys. We don't fold it into the `commands` registry /
`shortcuts.json` machinery: the binding's persistent home must stay in `settings.json` so the Rust startup/focus refresh
can read it before any window loads, and a global Carbon hotkey has no in-app scope and doesn't travel through the
keydown dispatch, so the scope/conflict apparatus doesn't apply. The toggle's description references the live binding
(via `global-shortcut-description.ts`) and updates when the user rebinds. Both surfaces call the
`set_global_go_to_latest_shortcut` IPC on change for live-apply.
