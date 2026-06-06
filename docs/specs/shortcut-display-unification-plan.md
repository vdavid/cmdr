# Shortcut display unification plan

Status: implemented, 2026-06-06. All milestones plus the two approved scope additions done, full check suite incl. slow
lane green, manually verified live in the running app via MCP (palette live rebind, deep-link arrival into a shortcut
row with the flash, F-key bar live update on rebind).

## Goal and intent

Cmdr teaches users its keyboard shortcuts by displaying them all over the UI (design principle: "When shortcuts are
available for a feature, always display the shortcut in a tooltip or somewhere less prominent than the main UI"). Today
that display is fragmented: ~20 distinct UI contexts render shortcuts in five different styles (raw `<kbd>`, assorted
`.shortcut-hint` spans, tooltip `shortcut:` field, plain prose, the Settings editor pills), and most sites hardcode the
combo string, so a user who rebinds a shortcut sees stale, untruthful hints. The command palette is the worst offender:
it renders registry **defaults**, so a rebound command shows a combo that doesn't work.

The product idea this plan serves: **every displayed shortcut is truthful, uniform, and teaches the user it's
customizable.** Concretely:

1. **One shared component** renders every in-UI shortcut, so the default style is uniform and new call sites can't
   hand-roll a divergent look.
2. **Dynamic display**: sites showing a customizable command's binding read the live effective shortcut reactively
   (rebinding in Settings updates visible UI immediately). The infrastructure for this
   (`reactive-shortcuts.svelte.ts::getFirstShortcutReactive`) already exists from the column-header tooltip work.
3. **First shortcut only** (matching the native menus), with one exception: the command palette shows up to **three**,
   because power users use the palette to discover alternates like `⌘3` / `⌘F3`. (Decision by David.)
4. **Click to customize**: where a displayed shortcut is free-standing (not nested inside another interactive control),
   clicking it opens Settings > Keyboard shortcuts, scrolled to that command's row with a brief flash. (Decision by
   David: option (a) — no click affordance where the chip sits inside an action control, like the F-key bar buttons;
   nested interactive elements are an a11y trap.)

Non-goals:

- Native menu accelerators (Rust-side) — already synced via `updateMenuAccelerator`; no display change.
- Tooltips stay non-clickable by nature; the tooltip action's `shortcut:` field remains the rendering path for tooltips.
  We only make tooltip _values_ reactive where they aren't yet.
- Converting hardcoded interaction keys (Enter, Esc, the search dialog's internal mode keys) into registry commands.
  They're not customizable; they only get the uniform _look_, never a settings link. See "Class B" below.
- The Rust-built context menus (for example the breadcrumb context menu) keep their snapshot-at-open shortcut: a native
  menu can't re-render live, and menus are short-lived.

## Decisions already made (with David)

- **(a) on nested interactivity**: no settings-link when the chip sits inside another interactive control (F-key bar
  buttons, palette rows). Only free-standing chips (toasts, prose hints, empty states) are clickable. Precise rationale:
  in those containers the whole row/button already owns the click (the F-bar buttons are `tabindex={-1}`, the palette
  keeps DOM focus on its input via `aria-activedescendant`), so a nested clickable chip would create a competing click
  target and double-activation, not a focus-nesting problem. Don't "fix" this with focus management — the fix is simply
  a non-clickable chip there.
- **Visual direction**: delegated to the implementer. Direction chosen below (§ Component design); verify with
  screenshots and adjust to taste.
- **Palette**: up to three shortcuts, for power users.
- **Arrival UX in Settings**: scroll the row into view + flash. No search-box prefill.
- **Toasts stay snapshot**: a visible toast must not rewrite itself when the user rebinds mid-display (documented
  decision in `lib/go-to-path/CLAUDE.md`); the _next_ toast picks up the new binding. Toasts pass the snapshot string to
  the chip's literal mode.

## Current state (inventory)

Surveyed 2026-06-06. Two classes:

### Class A — sites displaying a _registry command's_ binding (customizable → must become dynamic)

| Site                      | File                                                                  | Today                                                                                                                                                       | Command id(s)                                                                                                                                                                                                                   |
| ------------------------- | --------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Command palette rows      | `lib/command-palette/CommandPalette.svelte:250-252`                   | `match.command.shortcuts` = registry DEFAULTS (bug), `slice(0, 2)`, plain span                                                                              | every palette command                                                                                                                                                                                                           |
| F-key function bar        | `lib/file-explorer/pane/FunctionKeyBar.svelte:95-184`                 | hardcoded `<kbd>F5</kbd>` etc., two platform variants                                                                                                       | `file.rename`, `file.view`, `file.edit`, `file.newFile`, `file.copy`, `file.move`, `file.newFolder`, `file.delete`, `file.deletePermanently` (map per button at implementation time; the bar's aria-labels list the combos too) |
| Tab bar "+" tooltip       | `lib/file-explorer/tabs/TabBar.svelte:153`                            | hardcoded `shortcut: '⌘T'`                                                                                                                                  | `tab.new`                                                                                                                                                                                                                       |
| Quick Look hint toast     | `lib/file-explorer/quick-look/QuickLookHintToastContent.svelte:36-46` | hardcoded `<kbd>⇧Space</kbd>`                                                                                                                               | `file.quickLook` (the Space/Enter kbds in the same toast are Class B)                                                                                                                                                           |
| Downloads toast           | `lib/downloads/DownloadToastContent.svelte:119`                       | snapshot prop (already effective-first)                                                                                                                     | `downloads.goToLatest`                                                                                                                                                                                                          |
| Go-to-path ancestor toast | `lib/go-to-path/GoToPathAncestorToastContent.svelte:32`               | snapshot prop (already effective-first)                                                                                                                     | `nav.back`                                                                                                                                                                                                                      |
| Transfer error suggestion | `lib/file-operations/transfer/transfer-error-messages.ts:80`          | hardcoded `Shift+F8` in the `trash_not_supported` entry's `suggestion` field (not `message`)                                                                | `file.deletePermanently` (interpolate effective-first at message build; plain text is fine here)                                                                                                                                |
| Onboarding AI step        | `lib/onboarding/StepAi.svelte:327`                                    | hardcoded `<kbd>⌘+</kbd>` — **factually wrong today**: the Select files dialog opens on bare `+` (`selection.selectFiles`, registry shortcut `['+']`, no ⌘) | `selection.selectFiles`. The migration intentionally changes the displayed key from `⌘+` to `+` — that's a truthfulness fix, not a regression                                                                                   |
| Sort column headers       | `lib/file-explorer/selection/SortableHeader.svelte`                   | already reactive via `getFirstShortcutReactive` (shipped)                                                                                                   | `sort.by*`                                                                                                                                                                                                                      |
| Settings editor           | `lib/settings/sections/KeyboardShortcutsSection.svelte`               | it IS the editor; out of scope for chip migration                                                                                                           | all                                                                                                                                                                                                                             |

### Class B — fixed interaction keys (not commands; uniform look only, never dynamic, never clickable)

Search/selection dialog internals (`lib/query-ui/`: ModeChips `⌥A/⌥F/⌥R`, EmptyState `⌘N/⌘H/⌘Enter`, QueryBar `⏎`,
ScopeFilterPopover `⌥C/⌥V`, QueryDialog footer hints, RecentItemsFooter `⌘H`), viewer keys (`routes/viewer/`: `W`, `F`,
search toolbar `⌘⌥C/⌘⌥R/⇧Enter/Enter/Esc`, binary-warning `⇧Space`/`Enter` kbds), `LoadingIcon` "Press ESC",
`PtpcameradDialog` `Ctrl+C`, network browser "Press ⌘R" (the handler is component-local, NOT the `pane.refresh` command
— leave static; promoting it to a registry command is separate future work), settings registry description strings,
recent-items `↑↓` hint.

Caveat on the dialog hints (`⌘N`, `⌘H`, `⌥A`…): some of these LOOK like commands but are dialog-internal key handlers
with no registry entry, hence not customizable. Displaying a settings link for them would be a lie. If any of them is
ever promoted to a registry command, its display site switches to `commandId` mode in the same edit.

### Shared infrastructure that already exists

- `lib/shortcuts/reactive-shortcuts.svelte.ts` — `getFirstShortcutReactive(commandId)` ($state version bumped on
  `onShortcutChange`; init notifies listeners for loaded customizations).
- `lib/tooltip/tooltip.ts` — `shortcut:` field renders `<kbd class="cmdr-tooltip-kbd">` (accent chip on glass).
- `openSettingsWindow(section?, anchor?)` (`lib/settings/settings-window.ts:63`) — handles cold-open (URL params
  `?section=<JSON>&anchor=<id>`) AND already-open (`focus-self` + `navigate-to-section` events). The settings page
  scrolls `anchor` into view via `scrollAnchorIntoView` (`routes/settings/+page.svelte:117`).
- Precedent deep-link: downloads toast → `'settings-downloads-notifications'` anchor; Quick Look toast →
  `openSettingsWindow(['Keyboard shortcuts'])` (section only, no row targeting).

### Known gaps the plan closes

1. No shared display component (`lib/ui/` has none; five ad-hoc styles).
2. Palette shows defaults, not effective bindings; not reactive; caps at 2.
3. `KeyboardShortcutsSection` rows have no DOM ids → nothing to scroll to.
4. `scrollAnchorIntoView` scrolls the settings `contentElement`, but the shortcuts list is its own nested scroll
   container (`.commands-list { max-height: 400px; overflow-y: auto }`) — `contentElement.scrollTo` can't reach a row
   inside it.
5. No arrival flash.
6. Pre-existing bug: the main window's `open-settings` Tauri-event listener (`routes/(main)/+page.svelte:310`) ignores
   the event payload, dropping the `section` the Rust MCP executor sends (`src-tauri/src/mcp/executor/dialogs.rs`).

## Component design

### `ShortcutChip.svelte` (new, `lib/ui/`)

One component, two mutually exclusive modes:

```svelte
<!-- Dynamic mode: live effective first shortcut, clickable by default -->
<ShortcutChip commandId="downloads.goToLatest" />

<!-- Literal mode: fixed key, never clickable. Also used for toast snapshots. -->
<ShortcutChip key="⏎" />
```

Props:

| Prop        | Type            | Notes                                                                                                                                                                                                                                     |
| ----------- | --------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `commandId` | `CommandId?`    | Dynamic mode. Renders the first effective shortcut via `getFirstShortcutReactive`. Renders **nothing** when the command has no binding (callers embedding the chip in prose must conditionalize the sentence — check each migrated site). |
| `key`       | `string?`       | Literal mode. Exactly one of `commandId` / `key` must be set; a dev-time error otherwise.                                                                                                                                                 |
| `clickable` | `boolean?`      | Default `true` in `commandId` mode, forced `false`/ignored in literal mode. Set `false` when the chip sits inside another interactive control (palette rows; the F-key bar, if it adopts the chip).                                       |
| `size`      | `'sm' \| 'md'?` | `sm` for dense contexts (palette), `md` default. Add only if the first visual pass needs it; don't speculate.                                                                                                                             |

Rendering and behavior:

- Element: `<kbd>` in non-clickable mode; `<button type="button">` wrapping the kbd in clickable mode (real button =
  free keyboard a11y; `aria-label="Customize the {command name} shortcut"`).
- Clickable behavior: `onclick` → `openShortcutCustomization(commandId)` (see § Deep link). Also show a tooltip
  ("Customize this shortcut") via the existing tooltip action so the affordance is discoverable.
- **Import-graph requirement (viewer bundle)**: the chip must NOT statically import the deep-link helper.
  `openShortcutCustomization` → `openSettingsWindow` pulls in `@tauri-apps/api/webviewWindow`, `emitTo`, and
  window-positioning. Two distinct reasons, both real: (1) bundle hygiene — a literal-mode chip in the
  capability-restricted viewer window must stay importable with zero Tauri surface at module-eval time, regardless of
  whether anything is ever clicked; (2) runtime — the viewer's capability file has no window-creation or store
  permissions, so if the path ever ran there it would reject with the generic "not allowed" error (AGENTS.md: "Tauri
  APIs fail silently without permissions"). Gate the helper behind the clickable branch with a dynamic `import()` (or an
  injected callback). This is an M1 design constraint, not an M5 cleanup — by M5 the component API is frozen.
- **Why the intent matters**: the chip in `commandId` mode is a _claim about live app behavior_ ("pressing this does
  X"), so it must read the reactive store. The chip in literal mode is _typography_. Keeping the modes in one component
  is what guarantees the uniform look David asked for, while the prop split keeps the truthfulness rule mechanical:
  customizable → dynamic; fixed → static.

Visual direction (delegated decision, made here): model the chip on the Settings `.shortcut-pill` (neutral
`--color-bg-tertiary` background, 1px `--color-border`, `--radius-sm`, `--font-size-xs`), NOT on the tooltip's accent
chip — accent-on-glass is right inside the dark tooltip but too loud repeated across the main UI. Clickable chips add a
hover state (accent border + `--color-accent-text` text) to signal interactivity; cursor stays `default` per the
app-wide convention (only `LinkButton` opts into `cursor: pointer`). The tooltip path keeps its existing
`.cmdr-tooltip-kbd` accent look — different context, different contrast needs; do NOT try to unify those two. Verify the
chip in light + dark + the component catalog before migrating call sites; the a11y-contrast check
(`scripts/check-a11y-contrast`) and stylelint will hold the token usage honest.

Per the `lib/ui/` checklist (its `CLAUDE.md` § "Component catalog"): add the catalog section
(`routes/dev/components/sections/ShortcutChip.svelte` + debug sidebar entry), `ShortcutChip.test.ts`, and
`ShortcutChip.a11y.test.ts`.

### Reactive plural helper

Add `getEffectiveShortcutsReactive(commandId: CommandId): string[]` to `lib/shortcuts/reactive-shortcuts.svelte.ts`
(same version read; `getFirstShortcutReactive` becomes `[0]` of it). The palette needs the full list (up to three). Type
the new helper's param as `CommandId`, not the loose `string` the existing helpers use — same no-stringly-typed
discipline as the chip's prop.

## Deep link design

### `openShortcutCustomization(commandId)`

New helper (suggested home: `lib/settings/settings-window.ts`, next to `openSettingsWindow`):

```ts
openSettingsWindow(['Keyboard shortcuts'], `shortcut-${commandId}`)
```

The anchor id convention `shortcut-<commandId>` (for example `shortcut-downloads.goToLatest`) is shared knowledge
between this helper and the section; keep it as one exported constant/function so it can't drift.

### Settings side

1. **Row ids**: each `.command-row` in `KeyboardShortcutsSection.svelte` gets `id="shortcut-{command.id}"`. Put the id
   on the keyed element so it survives the `shortcutChangeCounter` re-keying. `downloads.goToLatest` IS a regular
   registry row (it's in `commands` and `menuCommands`), so the deep link targets that row; the bespoke
   `GlobalShortcutRow` (the `(global)` hotkey, binding in `settings.json`) is a separate concern and gets NO anchor in
   this plan. Watch one interaction: the global row's visibility (`showGlobalGoToLatestRow`) reacts to the same filters
   the deep link clears — clearing filters must not glitch it (it'll simply show; fine).
2. **Nested-scroll fix — conditional, not a blanket swap**: `scrollAnchorIntoView` currently does
   `contentElement.scrollTo(...)`, and that form is deliberate — `handleSectionSelect` carries an explicit comment about
   avoiding `scrollIntoView` so the outer settings layout / drag region doesn't shift. The function has TWO call sites
   (cold-open URL anchor at `+page.svelte:252` and the `navigate-to-section` event handler at `:282`), and the existing
   `settings-downloads-notifications` consumer relies on the contained `contentElement.scrollTo` behavior. So: keep
   `contentElement.scrollTo` as the default path, and add a branch for `shortcut-*` anchors that scrolls the nested
   `.commands-list` container to the row (compute the offset within that scroller, or use
   `scrollIntoView({ block: 'nearest' })` ONLY on the inner scroller's content — verify visually that the outer layout
   doesn't shift). Keep the silent no-op when the anchor is missing. Honor `prefers-reduced-motion` (`'auto'` instead of
   `'smooth'`). Re-verify BOTH existing consumers land right after the change.
3. **Flash**: when the anchor matches `shortcut-*`, the settings page passes the target command id into the section
   (suggested: a small module-level `$state` in a `lib/settings/pending-shortcut-highlight.svelte.ts`, set by
   `+page.svelte`'s anchor handling, read-and-cleared by `KeyboardShortcutsSection`; both ends must actually import it
   or knip fails the suite on unused exports). The section applies `class:flash={command.id === highlightId}` and clears
   after the animation ends (~1.5 s). CSS: two gentle background pulses using accent-subtle;
   `@media (prefers-reduced-motion: reduce)` swaps the pulse for a static highlight that fades. Don't apply the flash by
   mutating DOM classes directly: the rows re-key on `shortcutChangeCounter`
   (`{#each ... (\`${command.id}-${counter}\`)}`), so a direct DOM class would vanish on any re-render; state-driven
   classes survive.
4. **Filters can hide the target row**: if the user left the section filtered (`activeFilter: 'modified'`, a key filter,
   or a search query), the target row may not be rendered. On deep-link arrival, clear the local filters first (set
   `activeFilter = 'all'`, clear `localNameSearchQuery` / `keySearchQuery`) so the row is present.
5. **Timing — the exact sequence is load-bearing**: clearing filters mutates `$derived` state (`filteredCommands` →
   `groupedCommands`), so the target row does NOT exist in the DOM until Svelte flushes. The sequence is: clear filters
   → `await tick()` (row mounts) → `setTimeout(0)` (defer past the current handler) → scroll + set the flash state. The
   `tick()` is a REQUIRED step, not an optimization. And per the repo-wide gotcha (`docs/testing.md` § "rAF in unfocused
   windows"): E2E opens settings with `focus: false`, where WKWebView throttles rAF — the flash/scroll is
   E2E-observable, so while touching `scrollAnchorIntoView`, switch its existing double-rAF deferral to `setTimeout(0)`
   per the documented rule. Skipping either piece ships a version that flakes exactly the way the testing doc warns.

### Main window side

Fix the `open-settings` listener (`routes/(main)/+page.svelte:310`) to read the payload and forward the **section**:
`openSettingsWindow(payload.section ? [payload.section].flat() : undefined)`. Scope note: the Rust MCP executor
(`src-tauri/src/mcp/executor/dialogs.rs:80`) emits `json!({"section": section})` ONLY — there is no `anchor` in the MCP
`dialog` tool's params, so don't wire `payload.anchor` and believe MCP can deep-link to a row; it can't today. This fix
makes the existing section-level MCP deep-link work; extending the MCP `dialog` tool with an `anchor` param is
explicitly out of scope (note it as possible future work in the commit message, nothing more). The in-app click path
(`openShortcutCustomization` → `openSettingsWindow` directly) never touches this event and carries the anchor fine.
Implementation notes: the current listener takes no `event` param at all (add it), and `dialogs.rs` sends `section` as a
bare STRING (`&str`), not an array — hence the `[payload.section].flat()` wrap. Parse the `unknown` payload defensively
(whitelist-style, no `as` casts — same discipline as `mcp-listeners.ts`).

## Milestones

Sequential is fine throughout (per our planning conventions, no parallel execution is needed; the milestones are ordered
by dependency). Each milestone ends green: `./scripts/check.sh --fast` during work, full `./scripts/check.sh` before its
commit.

### M1 — `ShortcutChip` component + reactive plural helper

- `lib/shortcuts/reactive-shortcuts.svelte.ts`: add `getEffectiveShortcutsReactive` (plural); reimplement
  `getFirstShortcutReactive` on top. Preserve the underlying copy-on-read contract: `getEffectiveShortcuts` always
  returns a fresh array (`[...custom]` / freshly mapped defaults) so consumers can't mutate the store's data — don't
  "optimize" the reactive wrapper into returning a cached reference.
- `lib/ui/ShortcutChip.svelte` per § Component design. No call-site migration yet.
- Component catalog section + debug sidebar entry.
- Tests: `ShortcutChip.test.ts` (literal renders; commandId renders effective first shortcut; renders nothing when
  unbound; rebind updates the chip — reuse the mock pattern from `SortableHeader.svelte.test.ts`; clickable mode calls
  the deep-link helper — mock it), `ShortcutChip.a11y.test.ts` (clickable + non-clickable states).
- Docs: `lib/ui/CLAUDE.md` (Key files row + section), `lib/shortcuts/CLAUDE.md` (plural helper).
- Note: M1 can stub `openShortcutCustomization` as the M3 function signature so the component compiles; M3 fills it. Or
  land M3 first — implementer's choice; the stub is one line either way.

### M2 — Command palette fix

- `CommandPalette.svelte`: render shortcuts from `getEffectiveShortcutsReactive(command.id)` (NOT `command.shortcuts`),
  cap at **three** (`slice(0, 3)`), render each as a non-clickable `ShortcutChip` (`clickable` false: the row is a
  button; decision (a)).
- Visual check: palette rows are dense; if three chips crowd the row, prefer shrinking (size `sm`) over dropping the
  count — the three-shortcut cap is a product decision.
- Tests: extend the palette's existing tests (or add) to pin: effective-over-default (rebind → palette shows custom),
  three-cap, and reactivity (the palette stays open while Settings rebinds via MCP — the row must update; this is what
  "reactive" buys here).
- Regression note: `formatShortcuts` and its `' / '` join go away (verified: only used inside `CommandPalette.svelte`).
- Docs: `lib/command-palette/CLAUDE.md` documents `formatShortcuts` and the 2-cap in multiple places — update it in the
  same commit or it goes stale immediately.

### M3 — Deep link into Settings > Keyboard shortcuts

- `openShortcutCustomization(commandId)` helper + the `shortcut-<commandId>` anchor convention (one definition).
- Row ids on `.command-row`s; `GlobalShortcutRow` decision per § Deep link.
- `scrollAnchorIntoView` nested-scroll fix + reduced-motion + `setTimeout(0)` deferral.
- Filter-clearing on deep-link arrival.
- Flash via state-driven class + CSS animation (+ reduced-motion variant).
- Fix the main window's `open-settings` payload forwarding (pre-existing MCP gap; makes the Rust-side "open settings at
  section" event actually work).
- Migrate the Quick Look toast's existing `openSettingsWindow(['Keyboard shortcuts'])` call to
  `openShortcutCustomization('file.quickLook')` — first consumer, proves the path. Precision: that call sits behind the
  toast's `LinkButton` ("Settings > Keyboard shortcuts"), NOT behind the `⇧Space` kbd. This file is touched twice across
  milestones: M3 migrates the LinkButton's target; M4 migrates the kbds to chips. Don't double-handle or skip either.
- Tests: unit-test the anchor-id builder and the filter-clearing logic if extracted pure; E2E: one Playwright spec —
  trigger `openShortcutCustomization` (via the deep-link consumer or `webview_execute_js`), assert the settings window
  lands on Keyboard shortcuts with the target row visible (use `expect.poll`; never bare `pollUntil` — see
  `docs/testing.md`). Settings E2E precedent: `settings.spec.ts`.
- Docs: `lib/settings/CLAUDE.md` + `sections/CLAUDE.md` (anchor convention, flash), `lib/shortcuts/CLAUDE.md` (deep-link
  entry point).

### M4 — Migrate Class A sites

Per site: switch to `ShortcutChip` (or reactive value), pick clickable per decision (a), keep toast snapshots literal.

- **F-key bar**: replace both hardcoded platform variants with per-command dynamic first shortcuts (the platform fork
  collapses naturally — `getEffectiveShortcuts` is platform-formatted; the bar's existing `fnKeyToCommand` map already
  names the 9 command ids). Chips render non-clickable (inside buttons). Mind the bar's visual rhythm: if the chip's
  boxed look fights the bar, it's acceptable for the bar to keep its local `<kbd>` styling but read the _reactive value_
  — truthfulness is the must, the chip look is the want. Screenshot before/after. Watch for long custom bindings (a user
  can bind `⌘⇧⌥K`); the bar must not break layout — test with an absurd binding.
  - The bar's `aria-label`s ("Create new file (Shift+F4)") must interpolate the same dynamic combo.
  - **Shift-fork semantics (decided)**: the bar's `shiftHeld` fork (which reveals the ⇧F4/⇧F6/⇧F8 alternate buttons)
    stays presentational and hardcoded — WHICH buttons appear on Shift doesn't change. Each visible button's chip shows
    ITS command's effective first binding, whatever that is. If the user rebinds `file.deletePermanently` to `⌘⌫`, the
    Shift-revealed "Permanently" button shows `⌘⌫` — slightly odd next to its siblings, but truthful, and truthful wins
    (that's the premise of this whole plan). Don't try to make the fork follow bindings; that's a rabbit hole with no
    user value.
- **Tab bar tooltip**: `shortcut: getFirstShortcutReactive('tab.new')` — value change only; the tooltip action already
  live-updates. Same pattern as the sort headers. (Wrap in `$derived`.)
- **Quick Look toast**: `⇧Space` kbd → `ShortcutChip commandId="file.quickLook"`... NO — toasts snapshot. Pass
  `key={snapshot}` captured at toast creation like the downloads toast does. The toast's `Space`/`Enter` kbds → literal
  chips. Its "customize" affordance: clickable is fine here IF wired to the snapshot's command id — clicking opens
  Settings regardless of rebind races, so use a literal chip + a separate explicit `clickable-for` prop? Simpler: give
  the chip an optional `customizeCommandId` escape hatch ONLY if this case demands it; otherwise leave the toast's chips
  non-clickable for v1. Don't grow the API speculatively — decide at the site.
- **Downloads toast + go-to-path toast**: replace `<kbd>{snapshot}</kbd>` with literal chips. Same clickable
  consideration as above; same resolution.
- **Transfer error message**: interpolate `getEffectiveShortcuts('file.deletePermanently')[0]` at message build time
  (it's a transient error string; snapshot semantics are right, and it's plain text — no chip).
- **Onboarding StepAi `⌘+`**: it's `selection.selectFiles` (registry shortcut `['+']`, bare key). The current `⌘+` is
  wrong; the migration fixes the displayed key to `+`. The surrounding copy is "You press `<kbd>` and type something
  like `*.jpg,...`" — a bare `+` chip mid-prose can read as a separator, so reword to something like "You press the `+`
  key and type..." while keeping the chip.
- Tests: each migrated site that has tests gets its assertion updated; add coverage where the migration introduces logic
  (the F-key bar's command→button map deserves a small test pinning the 9 mappings).
- Docs: touched feature `CLAUDE.md`s per the docs-maintenance rule.

### M5 — Class B uniform look (mechanical sweep)

Replace raw `<kbd>` / hint spans in Class B sites with literal-mode chips for visual uniformity. Notes:

- This is churn-heavy and low-risk; do it last, in one commit, so a revert is cheap if the uniform chip reads badly in
  some dense context (the query-ui `.tg-hint` mode chips are the most likely casualty — they're deliberately
  whisper-quiet tertiary text; if the chip is visually heavier, KEEP the hint style there and note the exception in the
  component's CLAUDE.md rather than forcing it).
- The viewer window is capability-restricted but the chip in literal mode imports nothing Tauri-touching — verify the
  import graph stays clean for the viewer bundle (no `$lib/ipc` pull-in from the chip's clickable path at module level;
  lazy-import the deep-link helper if needed).
- Screenshot the main affected surfaces in light + dark.

This milestone is explicitly trimmable: if during execution the uniform chip turns out wrong for most Class B contexts,
stop, keep Class B as-is, and record the decision in `lib/ui/CLAUDE.md`. The hard goals of this plan (truthful +
dynamic + customizable-on-click) all live in M1–M4.

### M6 — Wrap-up

- CHANGELOG entry (user-facing: "Shortcuts shown in the app now reflect your custom bindings; click a shortcut chip to
  customize it").
- Sweep all touched `CLAUDE.md`s; update `docs/architecture.md` only if a new module appeared (it didn't — the chip
  lives in existing `lib/ui/`).
- Full `./scripts/check.sh --include-slow` (E2E lanes included — M3 added a Playwright spec).
- Manual verification in the running app via MCP (isolated `pnpm dev --worktree` instance + Tauri MCP bridge, same flow
  as the sort-header verification): palette shows custom binding after rebind; chip click lands on the flashed row;
  F-key bar updates live on rebind.

## Testing summary

| Layer            | What                                                                                                                                          |
| ---------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| Unit (TS)        | ShortcutChip modes/reactivity/unbound; reactive plural helper; palette three-cap + effective source; F-key bar command map; anchor-id builder |
| a11y (tier 3)    | ShortcutChip clickable + static; re-run affected sections' existing a11y tests                                                                |
| E2E (Playwright) | One deep-link spec (open → section → row visible); existing settings + palette specs stay green                                               |
| Visual           | Component catalog section; MCP-driven screenshots of palette, F-key bar, toasts, Settings flash                                               |

## Risks / gotchas for the implementer

- **Rows re-key on `shortcutChangeCounter`** in `KeyboardShortcutsSection` — flash must be state-driven (see § Deep
  link). Also the row ids must be on the keyed element so they survive re-keying.
- **rAF in unfocused windows** (`docs/testing.md`): anything E2E-observable in the settings window must not gate on
  `requestAnimationFrame`.
- **No string-matching rule**: the chip branches on `CommandId`s and typed props only; never on combo strings.
- **`cursor: pointer` is LinkButton-only** by stylelint; the clickable chip uses hover styling, not cursor.
- **a11y-coverage check** requires the new component to have an a11y test; **file-length** is warn-only — don't bump
  allowlists.
- **Palette perf**: `getEffectiveShortcuts` does TWO O(commands) scans per call on the common no-custom path
  (`commands.find` + `.map(toPlatformShortcut)`), and the palette re-derives ~77 rows per fuzzy-search keystroke plus on
  every `onShortcutChange` bump. Registry is ~109 entries, so this is likely still fine — but if typing in the palette
  shows jank, memoize per version-tick. Measure first, don't pre-optimize.
- **Settings window scopes array**: `KeyboardShortcutsSection`'s `scopes` list filters by exact scope string; commands
  with compound scopes may not render in any group. If a deep-link target turns out to be unrendered for this reason,
  surface it — that's a pre-existing display bug to report, not to silently extend this plan with.
