# Settings system details

Pull-tier docs for `apps/desktop/src/lib/settings/`: architecture, flows, and decision rationale. Must-know invariants
and gotchas live in [CLAUDE.md](CLAUDE.md).

## Live-apply rule

**Every setting MUST apply immediately without restart.** The frontend side of this contract lives in
`settings-applier.ts`: every setting that the backend (or a global JS module) reads must have a matching `case` in the
`handleSettingChange` switch that calls the right Tauri command or module helper. When adding a new setting that changes
backend behavior, you MUST add: (a) a Tauri command on the Rust side (see the mirrored rule in
`src-tauri/src/settings/CLAUDE.md`), (b) a typed wrapper in `$lib/tauri-commands/settings.ts`, and (c) an
`onSettingChange` case in `settings-applier.ts` that invokes it. Restart-required is a bug, not a design choice. If the
setting looks "structural" (like re-opening a TCP connection, rebinding a port, swapping a thread pool), still
live-apply. Reconnect, rebind, restart the worker, whatever it takes. **MUST.** No exceptions.

## Purpose

The settings system provides user-configurable options for Cmdr through a registry-based architecture. All settings are
defined once in `settings-registry.ts` and accessed uniformly by both UI and MCP tools.

## Architecture

### Registry (`settings-registry.ts`)

Single source of truth for all settings. Each entry is authored as a `SettingDefinitionSource` (in `types.ts`) carrying:

- `id`: Unique key (e.g., `appearance.uiDensity`)
- `section`: Path in settings tree (e.g., `['Appearance', 'Colors and formats']`) — stays English (see i18n below)
- `labelKey` / `descriptionKey`: i18n message KEYS, not English (see i18n below)
- `type`: boolean, number, string, enum, duration
- `default`: Default value
- `constraints`: Type-specific validation (min/max, enum options, etc.); enum options carry `labelKey` (or a literal
  `label` for non-copy values like brand names and numerals)
- `component`: UI hint (switch, select, slider, etc.)

### i18n: the registry stores message KEYS, resolved through `t()` (M2 decision)

**Decision / why.** Settings copy is translation-ready: `settings-registry.ts` stores message KEYS, and the rendered
text lives in `messages/en/settings.json` (the i18n catalog), not inline. `resolveDefinition` turns each authored
`SettingDefinitionSource` into a `SettingDefinition` whose `label`, `description`, and enum-option `label`s are
**getter-backed**: reading `getSettingDefinition(id).label` calls `tString(labelKey)` at read time. This was the
cleanest shape because it leaves the entire `getSettingDefinition(...).label` / `.description` consumer surface (the
section components, the search index in `settings-search.ts`, the MCP YAML bridge in `mcp-main-bridge.ts`,
`AdvancedSection`) unchanged — zero call-site churn — while moving the copy into the catalog. Getters also give the
right reactivity semantics for free: a `t()` read in markup re-renders on a locale change, and a read in plain `.ts` is
a snapshot (the same semantics the transfer pilot uses). An empty description (no `descriptionKey`) resolves to `''`.

**Why `section` stays English.** The `section: string[]` array is a STRUCTURAL identity used for routing
(`SettingsContent`), the section tree (`buildSectionTree`), section matching (`getSettingsInSection`), and the search
haystack — not a render path. Translating it would couple identity to copy. The rendered section TITLES are separate:
the section components and `SectionSummary` resolve them through `sectionTitle(name)` in `section-i18n.ts` (the single
English-name → title-key map), and `SettingsSidebar` does the same for the nav list.

**Catalog key shape.** `settings.<id>.label` / `.description` (the setting id's dots preserved),
`settings.<id>.opt.<value>` / `.optDesc.<value>` for enum options, `settings.section.*` / `settings.summary.*` for
section titles and summary blurbs, `settings.control.*` for shared row microcopy (reset, restart-required,
decrease/increase aria-labels), and `settings.<feature>.*` for section-component-specific copy. The downloads FDA hint
reuses the shared `<Trans>` message `common.downloadsFdaHint` (an inline `<settingsLink>`), so don't duplicate it.
Apostrophes in catalog values are doubled (`''`, the ICU rule). Full i18n runtime design:
[`$lib/intl/DETAILS.md`](../intl/DETAILS.md).

**Scope of the M2 settings tranche.** The registry-driven settings core and the section chrome are migrated and the
`cmdr/no-raw-user-facing-string` lint is enforced on `lib/settings/`. Four files render copy owned by ADJACENT
subsystems (not the settings registry) and are excluded in that lint's ledger until their own tranches: `AiCloudSection`
/ `AiLocalSection` (AI provider config UIs), `KeyboardShortcutsSection` (command names from the command registry +
conflict-banner chrome), and `LicenseSection` (license-API display copy).

### Store (`settings-store.ts`)

- Uses `tauri-plugin-store` for persistence to `~/Library/Application Support/com.veszelovszki.cmdr/settings.json`
- Debounced saves: 500ms after last change, atomic write (temp file + rename)
- In-memory cache for synchronous reads via `getSetting()`
- Cross-window sync: emits `settings:changed` events when values change
- **Store path goes through `resolveStorePath(storeName)`** (`store-path.ts`). `tauri-plugin-store` resolves a bare name
  against Tauri's identifier-driven `app_data_dir()`, which ignores `CMDR_DATA_DIR`. In isolated instances (dev,
  per-worktree dev, E2E) that would read the real production store file; the helper asks the backend
  (`get_isolated_store_path`) for an absolute path under the resolved data dir so the frontend store agrees with the
  Rust side. Production returns the bare name, byte-identical. **Every `tauri-plugin-store` reader must go through this
  helper** — `settings.json` (this store, plus `lib/settings-store.ts`'s FDA/onboarding store and
  `lib/logging/logger.ts`'s verbose-logging probe), `shortcuts.json` (`lib/shortcuts/shortcuts-store.ts`), and
  `app-status.json` (`lib/app-status-store.ts`) all do. The backend command takes `store_name` from the frontend and
  sanitizes it (`sanitize_store_name` in `commands/settings.rs`): it rejects anything that isn't a plain filename (path
  separators, `..`, absolute paths) and returns `None`, which the helper treats like production, so a bad name can never
  escape the data dir.

### Restricted-window mode (the viewer)

The viewer window has no `tauri-plugin-store` capability by security design (it renders arbitrary, possibly-hostile file
content; see `src-tauri/capabilities/CLAUDE.md` § viewer). It calls `initializeSettings({ restrictedWindow: true })`,
which never touches the store plugin:

- **Reads**: the cache seeds from the typed `get_restricted_window_settings` backend command (allowlist:
  `viewer.wordWrap`, `fileViewer.suppressBinaryWarning`, `appearance.textSize`, `appearance.appColor`; the command reads
  `settings.json` fresh, so the snapshot lags the main window's cache by at most the 500 ms save debounce). Live updates
  after open arrive through the regular cross-window `settings:changed` event.
- **Writes**: `setSetting` skips the store save and forwards allowlisted ids through the typed
  `persist_restricted_window_setting` command (enum-validated on the Rust side), which emits to the main window;
  `restricted-settings-bridge.ts` (mounted in the main layout) re-checks the allowlist and persists via
  `persistSettingFromRestrictedWindow` — a deliberate `setSetting` bypass, because the viewer's own cross-window emit
  has usually already synced the main cache and the idempotency guard would otherwise skip the save. Non-allowlisted
  `setSetting` calls in restricted mode are session-only (debug log, no persistence).
- **Failures degrade to registry defaults with a `log.warn`**, never `log.error`: an error-level log here fires an auto
  error report on every viewer open, which is the regression that motivated this mode (the viewer used to call the
  store-backed init path and hit `plugin:store|load not allowed by ACL` every time).

When a new setting needs to be readable or persistable from the viewer, extend the `RestrictedWindowSettings` struct /
`RestrictedWindowPersistableSetting` enum in `src-tauri/src/commands/settings.rs` and the mirror maps in
`settings-store.ts` + `restricted-settings-bridge.ts`. Never grant the viewer store permissions instead.

### Text size (`appearance.textSize`)

`appearance.textSize` (slider 50–200%, default 100%) compounds with the macOS Accessibility > Display > Text Size value
to produce the effective scale that `lib/text-size.ts` writes to `--font-scale` on `:root`. **Compounding lives in
exactly one place**: `text-size.ts`'s `computeAndApply()`. The CSS
`html { font-size: calc(16px * var(--font-scale, 1)) }` plus rem-based `--font-size-*` tokens in `app.css` cover
typography; `applyDensity()` in `settings-applier.ts` multiplies row-height/icon-size/density-spacing by the same
`--font-scale` so layout grows with text. After each scale change, `text-size.ts` re-triggers
`ensureFontMetricsLoaded()` on a 1 s debounce so Rust gets fresh Brief-mode width data for the new font ID.

### Date display (one source of truth)

Every site that shows a modified date in the UI flows through one entry point:

- **`formatDateForDisplay(ts, format, customFormat, nowMs?)`** in `format-utils.ts`: pure. Returns a `FormattedDate`
  with the joined `text` and an ordered `segments` list of `DateSegment`s. Each segment carries a `text` and an optional
  `ageClass` covering one of four per-component tiers (year, month, day, time). Handles all four format modes:
  token-based (`iso`, `short`, `custom`, default) via `applyTokens`, and `system` via
  `Intl.DateTimeFormat#formatToParts` (component type comes from `part.type`, not from string-parsing locale output).
- Per-component coloring rules live in `age-tier-utils.ts`: `tierForYear` colors every year (current → `age-fresh`, last
  → `age-recent`, two back → `age-aging`, three or more back → `age-old`). `tierForMonth` only colors when the year
  matches now (same scale). `tierForDay` only colors when the year and month both match (today / yesterday / two days /
  three+ days). `tierForTime` only colors when the file's date equals today, distance in full hours. Future timestamps
  in any component clamp to the freshest tier. Segments outside their coloring window carry `ageClass: null`, and the
  renderer leaves them in default text color.
- **`formattedDate(ts)`** in `reactive-settings.svelte.ts`: reactive wrapper that reads the current setting values. This
  is the canonical entry point for the rest of the app.
- **`<DateLabel modifiedAt={ts} />`** in `$lib/ui/DateLabel.svelte`: the render-side equivalent. Use it anywhere a
  modified date appears in the UI and you don't have special layout needs (status bar, dialogs, search results, etc.).
  It walks `segments` and wraps each one with a non-null `ageClass` in `<span class={ageClass}>`.
- `FullList.svelte` is the one consumer that opts out of `<DateLabel>` because it renders the segments straight into its
  own virtual-scroll grid cell. It uses the same `formattedDate(...)` data; do the same if you add another consumer with
  bespoke layout.
- `buildDateTooltip` in `selection-info-utils.ts` mirrors the renderer for HTML tooltips: it walks segments and wraps
  the colored ones into `<span class="age-…">` directly.
- The plain-string shortcut `formatDateTime(ts)` is `formattedDate(ts).text`. Use it for tooltips, MCP responses,
  clipboard copies, anywhere you need a one-line label.

### Color palettes (size + date)

`appearance.sizeColors` (default `none`) and `appearance.dateColors` (default `none`) each pick a color palette applied
via `data-size-colors` / `data-date-colors` attributes on `<html>`. Settings applier wires both. CSS tokens
(`--color-size-*`, `--color-age-*`) live in `app.css`. Date coloring uses four tiers (`age-fresh`, `age-recent`,
`age-aging`, `age-old`) applied per-component (year, month, day, time) by the helpers in
`lib/settings/age-tier-utils.ts`. The setting value `app` (renamed from the older `accent`) refers to the user-facing
"app color" (internally the underlying CSS token is still `--color-accent`).

### Reactive state (`reactive-settings.svelte.ts`)

- Svelte 5 `$state` for settings that affect UI rendering (density, date format, file size format, directory sort mode)
- Subscribes to `onSettingChange()` and updates reactive values immediately
- Provides derived getters like `getRowHeight()` based on current density

### Sections (`sections/`)

Top-level sidebar order (declared in `SettingsSidebar.svelte`'s `TOP_LEVEL_ORDER`; keep in sync with the E2E test in
`settings.spec.ts`):

1. **Appearance** — `Colors and formats`, `Zoom and density`, `File and folder sizes`, `Listing`
2. **Behavior** — `File operations`, `File system watching`, `Search`
3. **AI** (no subsections)
4. **File systems** — `SMB/Network shares`, `MTP (Android/Kindle/cameras)`, `Git`
5. **Viewer** (no subsections)
6. **Keyboard shortcuts** (special, non-registry)
7. **Developer** — `MCP server`, `Logging`
8. **Updates & privacy** (no subsections): app-update checks, the crash/error report opt-ins, and the beta
   `analytics.enabled` opt-out + `analytics.email` contact field
9. **License** (special, non-registry)
10. **Advanced** (special, auto-generated from `showInAdvanced: true` entries)

Per-section component files (one `*Section.svelte` per sidebar entry), the section ↔ component map, the
mirror-in-multiple-sections pattern, and the AI hybrid-section split live in [`sections/CLAUDE.md`](sections/CLAUDE.md).

### Components (`components/`)

Reusable setting-row primitives (`SettingRow`, `SettingSwitch`, `SettingSelect`, `SettingSlider`, etc.) plus
`SettingsSidebar` / `SettingsContent` / `SectionSummary`. See [`components/CLAUDE.md`](components/CLAUDE.md) for the
file map, the 50-50 split-layout rule, and the `SettingPasswordInput` store-driven vs controlled modes.

### Other files

- **cloud-providers.ts**: Cloud provider preset definitions (OpenAI, Anthropic, Groq, etc.) and per-provider config
  helpers (`getProviderConfigs`, `setProviderConfig`, `resolveCloudConfig`). Used by `AiSection` and the startup flow in
  `+layout.svelte` to resolve the effective base URL and model. The API key is fetched separately from the OS secret
  store via `getAiApiKey(providerId)` before calling `configureAi`.
- **settings-search.ts**: Fuzzy search over setting definitions; returns ranked matches with highlight ranges
- **settings-applier.ts**: Listens for setting changes and applies side effects (CSS vars, backend config sync). The
  `passthroughBackendHandlers` table includes three entries for `ai.provider` / `ai.cloudProvider` /
  `ai.cloudProviderConfigs` that all call `ai-config.ts::pushConfigToBackend()`. The helper re-reads every relevant
  setting fresh at call time, so callers MUST NOT pass cached values — whichever provider/key/model is current at the
  IPC moment wins. This is the wiring that makes Settings AI-provider changes hot-apply without restart, and that lets
  the onboarding wizard's step 2 just call `setSetting(...)` and have the backend reconfigure automatically. The same
  table also wires `updates.autoCheck` to `updater.svelte::applyAutoCheckEnabled()`, which starts / stops the background
  poll loop in place. The onboarding wizard's "auto-update" toggle, the Settings UI switch, and any future MCP/IPC
  writer all go through this one handler.
- **ai-config.ts**: AI configuration plumbing shared by Settings, the onboarding wizard, and the applier listener.
  Exports `pushConfigToBackend()` (read-fresh push of the current AI config to Rust) and `migrateApiKeysFromSettings()`
  (one-time lift of pre-launch `apiKey` strings from `settings.json` into the OS secret store). Relocated here from
  `sections/ai-settings-utils.ts` so the function isn't tied to a UI subcomponent path.
- **network-settings.ts**: Network-specific setting helpers (proxy config, SMB auth defaults)
- **settings-window.ts**: Logic for opening/focusing/closing the settings window (Tauri window management). Accepts an
  optional `section` array (e.g. `['File systems', 'SMB/Network shares']`) to deep-link a specific section. Two delivery
  paths: (a) new-window: JSON-encoded array on the URL as `?section=...` (JSON because section names can contain `/`,
  e.g. "SMB/Network shares"); (b) already-open window: emits a `navigate-to-section` Tauri event the settings page
  listens for. The settings page also reads the URL param at mount, so reloads or fresh-opens land on the same section.
  Position: opens centered on the main window on first open of the session (via `lib/window-positioning.ts`). After
  that, the position+size persists in-memory (via the `get_child_window_rect` / `set_child_window_rect` Tauri commands)
  so reopening lands in the same spot. On app start the cache is empty again. Saved rects that no longer fit any monitor
  (display disconnected, etc.) are clamped to the nearest monitor. Also exports the keyboard-shortcut deep-link pair:
  `shortcutAnchorId(commandId)` / `commandIdFromShortcutAnchor(anchorId)` (the `shortcut-<commandId>` DOM-id convention,
  one definition so writer and readers can't drift) and `openShortcutCustomization(commandId)` (the in-app entry point
  clickable `ShortcutChip`s call to deep-link to a row). See § "Deep-link arrival into a shortcut row" below and
  [`sections/CLAUDE.md`](sections/CLAUDE.md).
- **pending-shortcut-highlight.svelte.ts**: shared module-level `$state` seam for the deep-link arrival flash. The
  settings page writes the target command id (`setPendingShortcutHighlight`) after scrolling its row into view; the
  `KeyboardShortcutsSection` reads it (`getPendingShortcutHighlight`) to apply a `class:flash`, then clears it
  (`clearPendingShortcutHighlight`) once the animation ends. The section also registers a filter resetter
  (`registerShortcutFilterReset`) the page calls before scrolling, so a leftover filter can't hide the target row. State
  (not a direct DOM class) because the rows re-key on `shortcutChangeCounter` — an imperative class would vanish on
  re-render. Both ends must import the module or knip flags the exports as unused.
- **format-utils.ts**: Shared formatters used in settings UI (e.g., duration, file-size display strings). Date/time is
  covered in detail under § "Date display" above. `formatDateForDisplay` is the canonical entry point. Every token
  format emits a fixed character count (`YYYY`=4, the rest zero-padded to 2), so the file-list date column lines up
  across rows under tabular figures with no split-cell trick; custom format default is `YYYY-MM-DD HH:mm`. The
  `'system'` formatter requests fixed-width components (2-digit month/day/hour/minute) so locale formats align too, and
  is memoized at module scope (constructing `Intl.DateTimeFormat` per call shows up in virtualized scroll profiles).
- **mcp-main-bridge.ts**: MCP bridge for settings; handles `mcp-get-all-settings` and `mcp-set-setting` round-trip
  events in the main window (always alive), enabling AI agents to query and modify settings without the settings window
  open

### Shortcuts (separate subsystem)

- Stored in separate `shortcuts.json` file
- Delta-only persistence: only customized shortcuts are saved
- Platform-specific display strings (macOS: `⌘⇧P`, Windows: `Ctrl+Shift+P`)
- Scope hierarchy determines conflict detection

## Key decisions

### Why hybrid declarative registry with custom UI?

JSON schema → generated UI was rejected (loses per-section UX polish). Manual pages + parser script was rejected (parser
is a second source of truth that drifts on refactors). Full architectural enforcement was overkill for a solo dev +
agents workflow. The hybrid approach gives: search that works perfectly (registry IS the search index), full UX freedom
per section, single source of truth for metadata, and CI catches missing UI or orphaned registry entries automatically.
UI components use Ark UI (see `../ui/CLAUDE.md`).

### Why separate shortcuts from settings?

Settings and shortcuts have different access patterns. Settings are mostly static config loaded at startup. Shortcuts
are dynamic, queried on every keypress, and frequently customized. Separate stores keep them isolated and optimized for
their use cases.

### Why registry-first?

Defining all settings in a registry enables:

1. Automatic MCP tool generation (agents can query/modify any setting)
2. Guaranteed validation (types and constraints enforced at runtime)
3. UI generation for Advanced section (technical settings don't need custom UI)
4. Schema migration (registry knows what's valid, can transform old data)

### Why store cloud AI API keys in the OS secret store, not `settings.json`?

API keys live in the OS-native secret store (macOS Keychain, Linux Secret Service, or an encrypted file fallback on
Linux without Secret Service) via `crate::secrets`. Access goes through the `saveAiApiKey` / `getAiApiKey` /
`deleteAiApiKey` / `hasAiApiKey` Tauri commands. `ai.cloudProviderConfigs` in `settings.json` only holds non-secret
fields (`model`, `baseUrl`). This keeps API keys out of Time Machine, cloud-sync backups, and any tool that mirrors
`~/Library/Application Support`. Same secret store backs SMB credentials. See `src-tauri/src/secrets/CLAUDE.md`.

### Why debounced saves?

Users often change multiple related settings in quick succession (e.g., tweaking slider values). Debouncing reduces disk
I/O from potentially hundreds of writes to one per change batch.

### Why cross-window sync?

The settings window and main window both need to react to setting changes. Without sync, changing UI density in settings
wouldn't update the main window until restart.

## Gotchas

### `$state` requires `.svelte.ts` extension

Reactive settings must live in `reactive-settings.svelte.ts` (not `.ts`). Svelte 5 `$state()` only works in `.svelte` or
`.svelte.ts` files.

### Schema version is mandatory

When modifying the settings format, increment `SCHEMA_VERSION` in `settings-store.ts` and add a migration case to
`migrateSettings()`. Otherwise old settings files may cause crashes.

### Settings cache is write-through

`setSetting()` updates both the in-memory cache and schedules a debounced save. If the app crashes between the cache
update and the save, the change is lost. This is acceptable for settings (worst case: revert to old value on restart).

### Shortcut menu sync is manual

When shortcuts are modified, menu accelerators don't auto-update. Call `invoke('update_menu_accelerator')` for commands
that have menu items (`view.fullMode`, `view.briefMode`). Most commands don't need this.

### Advanced section is auto-generated

Settings with `showInAdvanced: true` appear in the Advanced section with auto-generated UI. No custom component needed.
Just add to registry and it works.

### Hidden internal-state settings

`hidden: true` on a `SettingDefinition` excludes it from both the main section tree and the Advanced section, but the
value is still persisted via the same store and accessible via `getSetting`/`setSetting`. Use this for internal flags
the backend or business logic needs to track but the user shouldn't see (for example, `network.firstTriggerDone`, which
records whether we've ever performed a gated network action so subsequent launches can start mDNS eagerly without
re-prompting).

### Density mapping is internal

Users see "Compact/Comfortable/Spacious" but the code sees rowHeight/iconSize pixel values. The `densityMappings` object
in `types.ts` bridges this gap. Don't expose raw pixel values in the UI.

### Shortcuts conflict only when scopes overlap

`⌘N` in `Main window/File list` and `⌘N` in `About window` do NOT conflict because their scope chains don't overlap.
Only warn when the same key combo is used in overlapping scopes (for example, `Main window/File list` and
`Main window`). See `lib/shortcuts/DETAILS.md` § "Scope hierarchy" for the full chain model.

### No undo

Both settings and shortcuts save immediately (after debounce). There's no undo stack. Users must use "Reset to default"
to recover from mistakes.

### Escape closing the settings window must defer `close()` past the current event-loop iteration

`routes/settings/+page.svelte`'s `handleKeydown` wraps `getCurrentWindow().close()` in `setTimeout(() => …, 0)`
(mirroring `routes/viewer/+page.svelte`'s `closeWindow()`). Calling `close()` synchronously from inside the keydown
handler runs the destruction on the same GTK main-loop tick that handled the keydown. On Linux/webkit2gtk this stalls
**any IPC call queued behind the destruction from other webviews** (e.g. the main window) for an undefined time
(observed 60-65 % of test runs landing in the fast path, others timing out past 30 s). macOS uses WKWebView with
per-webview processes and doesn't exhibit the GTK-stall issue.

**`setTimeout(0)` instead of two `requestAnimationFrame`s** — the earlier rAF-based version flaked on macOS E2E because
WKWebView throttles `requestAnimationFrame` on windows that opened without focus (in E2E, `openSettingsWindow` passes
`focus: false` so the host machine stays usable while tests run). Throttled rAFs could push the deferred close past the
test's 3 s close-confirmation budget. `setTimeout(0)` isn't subject to the same throttling and still defers to the next
event-loop tick, which is all the Linux fix needs.

When adding a new self-closing webview (escape, close button, etc.), defer the `close()` call the same way. See commit
`46481b29` for the original bug-fix and the subsequent settings-escape-flake hunt for the macOS-throttle follow-up.

The rAF-throttling half of this gotcha is a repo-wide rule with its own recurrence history (three sightings: settings
close, viewer close, viewer readiness marker): `docs/testing.md` § "rAF in unfocused windows". Check there before gating
anything test-observable on `requestAnimationFrame`.
