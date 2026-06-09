# Indexing status indicator plan

Status: implemented, 2026-06-06. All milestones done, full check suite incl. slow lane green, manually verified in the
running app via MCP (scan + aggregation phases, tooltip content, size column, status bar, keyboard access).

## Goal and intent

The "drive indexing status" overlay at the top-right of the file explorer scares some users: it's prominent, technical
("Scanning... 42,000 entries, 1,200 dirs", "Computing directory sizes..."), and implies something heavy is happening to
their machine. Same for the `Scanning...` text in the Size column of every unindexed directory.

We're keeping the information (radical transparency is a core design principle — the user must be able to understand
what's happening) but moving it one level down in prominence:

1. **Top-right overlay → small hourglass icon.** The dynamic status message, progress bar, and ETA move into a rich
   tooltip on the icon. Anyone curious gets the full story on hover or focus; everyone else sees an unobtrusive "the app
   is doing something" hint.
2. **Size-column `Scanning...` → `<dir>` + hourglass icon.** Tooltip: "Sizes are usually ready after 3 minutes"
   (hard-coded copy for now, by explicit product decision). The per-row text was the scariest part: a fresh install
   shows `Scanning...` on every directory row.
3. **One indicator, not three overlays.** Scan, aggregation, and replay are all "the drive index is updating" in the
   user's mental model, so the scan overlay and the replay overlay merge into a single `IndexingStatusIndicator`
   component. (Decision made explicitly with David: "It's logically the same, 'Drive indexing status indicator' in my
   mental model.")

Non-goals:

- No backend/Rust changes. All state already flows through `index-state.svelte.ts`.
- No change to the size-display state machine (`getDirSizeDisplayState` keeps its four states); only the rendering of
  the `scanning` state changes.
- No change to the `size-stale` hourglass treatment (it already matches the target design).

## Key design decisions

### D1: Rich tooltip via a generic `contentEl` param on the existing tooltip action

The tooltip needs a live progress bar and ticking counters. Options considered:

- ~~`html` string updates~~: `innerHTML` replacement recreates the bar element every progress tick, so its CSS width
  transition resets (bar steps instead of glides) and text could flicker.
- ~~A `progress` param on the tooltip action~~: rejected — couples a domain concept into a generic text primitive.
- ~~A self-made popover in the indicator component~~: rejected — the frosted-glass look could be reused via the
  `.cmdr-tooltip` class, but the _behavior_ (400 ms show delay, measure-then-position, viewport clamping/flip,
  detached-trigger guards, `aria-describedby`, Escape/focus/blur) would be a ~60–80-line fork of `tooltip.ts` that
  drifts over time.
- ✅ **`contentEl?: HTMLElement` on `TooltipParam`**: the caller renders rich content into a hidden host div with normal
  Svelte; the action reparents that element into the tooltip on show and returns it to the host on hide. Because the DOM
  nodes persist, Svelte keeps updating them in place: the `ProgressBar` width transition survives, counters tick without
  flicker, and ALL existing tooltip machinery (delay, positioning, glass styling, a11y) comes along untouched. This is a
  generic primitive ("tooltip with live rich content"), not a domain leak.

Contract details for the implementer:

- `TooltipParam` object variant gains `contentEl?: HTMLElement`. Precedence: `contentEl` > `html` > `text`/`shortcut`.
- `isEmptyParam` must treat a param with `contentEl` as non-empty.
- On show: record the element's current parent (its hidden host), then `appendChild` into the tooltip element. On hide
  (and in `destroy`, and when `update()` swaps to a different param): move it back to the recorded host. Don't assume
  the host still exists — if the owning component unmounted mid-show, just detach (guard with `isConnected`).
- **The singleton-steal case (critical):** `tooltipEl` is a single shared element for the whole app. If ANOTHER
  trigger's tooltip shows (or the live `update()` path re-renders) while an adopted `contentEl` is mounted,
  `setTooltipContent`'s `innerHTML`/`textContent` assignment would orphan the adopted node without returning it — the
  owner's hidden host ends up with a detached child and Svelte keeps updating a dead subtree. Therefore the
  return-to-host logic must run at the TOP of `setTooltipContent` itself (it has three call sites: show, the live
  `update()` path, and any future ones), not just in hide/destroy: "if the tooltip currently holds an adopted contentEl
  and the new param is a different one (or not a contentEl param), return the old element to its host first."
- `positionTooltip` runs after content is attached, so measuring keeps working. Live growth (label getting longer)
  already repositions via the `update()` path; with `contentEl` the action can't see content mutations, so give the
  tooltip a stable width via CSS on the content host (fixed `min-width`, like ProgressOverlay's 160px column) instead of
  adding a ResizeObserver. Document this limitation in the tooltip docs.
- `aria-describedby` points at the tooltip element and screen readers read its _text_ content, so the content must carry
  the label + ETA as real text (the bar is decorative, `aria-hidden` is fine since `ProgressBar` has its own
  `role="progressbar"` — actually pass `ariaLabel` to it and let it be, axe will arbitrate in the a11y test).
- Tests in `tooltip.test.ts`: adopt-on-show, return-on-hide, return-on-destroy, live mutation while visible (mutate a
  child's text, assert the tooltip shows it without re-show), **the singleton-steal case** (trigger A's rich tooltip
  shown → trigger B's plain tooltip shows → A's contentEl is back in A's host, undamaged), param swap via `update()`
  while visible, and the existing detached-trigger guards still pass.

### D2: One `IndexingStatusIndicator.svelte` replaces both overlays

New component in `$lib/indexing/`, mounted in `routes/(main)/+page.svelte` where `<ScanStatusOverlay />` and
`<ReplayStatusOverlay />` are today (both get deleted).

- **Visibility**: `isScanning() || isAggregating() || isReplaying()`. The replay overlay's 4-second grace delay dies: it
  existed so a short replay wouldn't flash a big intrusive overlay; a small icon is unobtrusive, and showing it
  immediately makes the indicator honest about any index activity. Same for the old "replay hides while
  scanning/aggregating" anti-stacking rule — it becomes message priority inside one component (scan/aggregation message
  wins over replay) instead of two components fighting for the corner.
- **Rendering**: a small Lucide hourglass (`~icons/lucide/hourglass`, same icon as the size-column stale indicator,
  ~14px) positioned exactly where the overlay sat (`position: absolute; top/right: var(--spacing-sm)`), tertiary/
  secondary text color so it reads as a quiet status glyph, with a slow, subtle animation so it communicates activity
  (design principle: "show some anim to communicate that the app is doing something") — for example, a gentle opacity
  pulse. Gate it behind `prefers-reduced-motion: reduce` (static icon then).
- **Interactivity**: the old overlay was `pointer-events: none` so it wouldn't steal clicks near the corner. The icon
  must be hoverable, so that goes away — acceptable because the hover target is tiny. Keyboard-first principle: make it
  focusable (`tabindex="0"`) so keyboard users can reach the tooltip (the action already shows on focus; Escape hides
  it). **ARIA decision**: `<span tabindex="0" role="img" aria-label="Drive indexing status">` — matching the size-column
  hourglasses, which use `role="img"`. Do NOT use `role="status"`: it's a live region (implicit `aria-live="polite"`)
  for auto-announced changes, semantically wrong for a focusable hover target, and axe won't flag the misuse
  (valid-but-wrong). The tooltip carries the live detail via `aria-describedby`, which is the right mechanism. The tab
  stop is intentional and indexing-only (the component renders nothing when idle, so no dead tab stop in the steady
  state); confirm during M2 manual verification that Tab reaches it and Escape dismisses the tooltip.
- **Tooltip content** (the hidden host div, rendered by the same component):
  - Scan phase: "Scanning... 42,000 entries, 1,200 dirs" (existing dynamic label, `formatNumber` formatting).
  - Aggregation phase: existing phase labels ("Saving entries...", "Loading directories...", "Sorting directories...",
    "Computing directory sizes...", "Saving directory sizes...") + `ProgressBar` + percent + ETA for the phases that
    have progress (`saving_entries`, `computing`, `writing`).
  - Replay: "Updating index..." + "N events processed" + `ProgressBar` + blended ETA (keep the existing 50-50
    total-rate/sliding-window blend — it exists because early extrapolation alone is wildly wrong).
  - Reuse `ProgressBar.svelte` directly (size `sm`).
- **ETA logic extraction**: the "Almost done" / "Ns left" / "Nm left" formatting thresholds exist twice — as a named
  `formatEta` in `ReplayStatusOverlay.svelte` and inlined in `ScanStatusOverlay.svelte`'s `aggEta` `$derived.by`;
  `blendEtas` / `computeWindowEta` exist only on the replay side. Extract the pure parts (`formatEta`, `blendEtas`,
  `computeWindowEta`, and a simple elapsed-extrapolation ETA helper) into a new pure module `$lib/indexing/eta.ts` with
  unit tests (`eta.test.ts`). The component keeps only the reactive glue (sliding-window snapshot collection stays in
  the component since it's stateful, but feed it through the pure functions). Intent: testability without mounting a
  component, and killing the threshold duplication while we're here anyway.
- **A11y test**: `IndexingStatusIndicator.a11y.test.ts`. The component reads module-level `$state` from
  `index-state.svelte.ts`; mirror the mock pattern in `ScanStatusOverlay.a11y.test.ts` (lines ~14–27) **exactly as it
  is**: a single hoisted `vi.mock('./index-state.svelte', ...)` whose factory closes over module-scoped mutable `let`
  variables, which each test reassigns BEFORE `mount`. It is not a per-test re-mock — `vi.doMock` / per-test factories
  won't work with Vitest's hoisting. States to cover: idle → renders nothing; scanning → icon present;
  aggregating-with-progress. Then delete the old test files.

### D3: Size column `scanning` state renders like `size-stale`

In `FullList.svelte`, the `{:else if dirSizeState === 'scanning'}` branch (currently
`<span class="size-scanning"> Scanning...</span>`) becomes: `<span class="size-dir">&lt;dir&gt;</span>` followed by an
hourglass `icon-indicator`, copying the adjacent `size-stale` markup (lines ~935–939) but with the new tooltip copy.

- **Tooltip copy**: `Sizes are usually ready after 3 minutes` (David's wording, hard-coded number, explicit decision —
  don't compute or generalize it now).
- **The cell-level tooltip**: the whole `.col-size` span already carries `use:tooltip={buildDirSizeTooltip(...)}`, whose
  no-data-while-active branch returns `'Scanning...'` today. Change that branch in `full-list-utils.ts` to return the
  same new copy, so hovering anywhere on the cell and hovering the icon say the same thing. Update the pinned test in
  `dir-size-display.test.ts`.
- **Column measurement**: `measure-column-widths.ts` mirrors the rendered text in `sizeTextForEntry` (~line 196,
  currently returns `'Scanning...'` for the scanning state → change to `'<dir>'`). **The icon-width suffix needs its own
  fix, not a copy**: `sizeIconSuffixForEntry` (~lines 230–233) adds `SIZE_ICON_WIDTH` (= 14, the measurement constant —
  the rendered icon is `width="12"` plus gap) only when `entry.recursiveSize != null` (the size-stale case). The
  scanning case has `recursiveSize == null`, so today no icon width is reserved there. Extend the condition so a
  scanning dir (`recursiveSize == null && (indexing || recursiveSizePending)`) also reserves `SIZE_ICON_WIDTH`,
  otherwise the new hourglass clips in a shrink-wrapped size column. Verify visually.
- **Keep the state machine**: `getDirSizeDisplayState` still returns `'scanning'` as a distinct state. The _rendering_
  converges with `size-stale`; the semantics ("no size yet" vs "size shown but may change") stay distinct and so does
  the tooltip copy.
- The now-unused `.size-scanning` CSS rule in `FullList.svelte` gets deleted (`css-unused` check would catch it anyway).

### D4: Status bar (`SelectionInfo.svelte`) matches

Line ~240: `{#if dirSizeState === 'scanning'}Scanning...{:else}DIR{/if}` becomes `DIR` + an hourglass. **Branch
gotcha**: the existing `size-stale` hourglass (`stale-indicator stale-icon` markup, ~lines 245–254) lives in the SIBLING
`{:else if sizeDisplay}` branch (size exists); the scanning case hits the `{#if sizeDisplay === 'DIR'}` branch where no
hourglass exists today. So this is a new icon span added inside the `'DIR'` branch — copy the markup shape from the
stale one (including `role="img"`) but with its own `aria-label` ("Size not ready yet") and the "Sizes are usually ready
after 3 minutes" tooltip. Intent: the status bar is documented (selection/CLAUDE.md) as intentionally matching Full's
size column — keep that contract.

Note the status bar shows `DIR` (no angle brackets) while the file list shows `<dir>`; that asymmetry is pre-existing
and stays.

### D5: Deletions

After D2 lands, `ProgressOverlay.svelte` has exactly one consumer left: the dev components gallery. Delete:

- `$lib/ui/ProgressOverlay.svelte` + `ProgressOverlay.a11y.test.ts`
- `$lib/indexing/ScanStatusOverlay.svelte` + `.a11y.test.ts`
- `$lib/indexing/ReplayStatusOverlay.svelte` + `.a11y.test.ts`
- The ProgressOverlay demo block in `routes/dev/components/sections/Progress.svelte` (keep the `ProgressBar` rows — that
  component lives on), including the now-dead `overlayVisible` state, timer, its `onDestroy`, the "Show ProgressOverlay"
  button, and `.overlay-host` CSS. **Careful**: `overlayPercent` is also interpolated into the two surviving
  `ProgressBar` row labels ("size sm, {overlayPercent}%"), so keep it (or rename to something like `staticPercent`) —
  don't delete it with the rest.
- The `ProgressOverlay` row + section in `$lib/ui/CLAUDE.md`.
- The `ScanStatusOverlay` / `ReplayStatusOverlay` imports in `routes/(main)/+page.svelte` (lines ~19–20), not just the
  component tags (`knip` flags leftovers).
- The three line-coverage allowlist entries in `apps/desktop/coverage-allowlist.json` for
  `indexing/ScanStatusOverlay.svelte`, `indexing/ReplayStatusOverlay.svelte`, and `ui/ProgressOverlay.svelte`. The
  coverage check doesn't flag dead entries (it iterates coverage data, not the allowlist), so nothing fails if we forget
  — but removing entries for deleted files is the always-OK "tightening" direction per the allowlist rules.

Intent: gallery-only components are dead weight; `ProgressBar` remains the reusable primitive.

## Milestones

Sequential is fine. M1 and M3+M4 are independent of each other (both safe to parallelize in-session, no worktrees
needed), but M2 depends on M1.

### M1: Tooltip `contentEl` support

1. Extend `tooltip.ts` per D1.
2. Add tests to `tooltip.test.ts` (adopt, return on hide/destroy, live mutation, param swap).
3. Update the Tooltip section in `$lib/ui/CLAUDE.md` (usage example + the fixed-width note + trusted-content note
   stays).
4. `pnpm check --fast`, then the Svelte tests for the tooltip file.

### M2: IndexingStatusIndicator

1. Extract `$lib/indexing/eta.ts` (pure) + `eta.test.ts`.
2. Build `IndexingStatusIndicator.svelte` per D2 (icon + hidden tooltip host + tooltip wiring).
3. Swap it into `routes/(main)/+page.svelte`; delete the two old overlay components + tests (D5 list, except the
   ProgressOverlay parts which go after the gallery edit in the same commit).
4. Edit the gallery section, delete `ProgressOverlay.svelte` + its test.
5. Add `IndexingStatusIndicator.a11y.test.ts`.
6. Update `$lib/indexing/CLAUDE.md` (files table, replace both overlay sections with one indicator section, keep the
   listen-first-then-query decision, replace the pointer-events decision with the new hoverable-icon decision) and
   `$lib/ui/CLAUDE.md` (remove ProgressOverlay, note ProgressBar consumers change).
7. `pnpm check --fast` + targeted tests.
8. **Manual verification with the running app** (MCP, not browser): trigger a rescan (debug window has indexing
   controls; or `pnpm dev` on a worktree instance does a fresh scan with its own data dir), screenshot the corner icon
   and the open tooltip via the tauri MCP bridge, check both themes if feasible.

### M3: Size column de-scarification

1. `FullList.svelte` scanning branch per D3; delete `.size-scanning` CSS.
2. `full-list-utils.ts::buildDirSizeTooltip` no-data branch copy change.
3. `measure-column-widths.ts` `'Scanning...'` → `'<dir>'` (+ icon-width parity with stale).
4. Update `dir-size-display.test.ts` pinned strings; check `measure-column-widths` tests if any pin the string.
5. Comment updates in `e2e-playwright/indexing.spec.ts` (lines ~112, ~160 mention "Scanning..." in comments; the
   assertions check for numeric sizes and should pass unchanged — verify, don't assume).

### M4: Status bar

1. `SelectionInfo.svelte` per D4.
2. Update `SelectionInfo.dir-size-state.test.ts` (~lines 108–113): it asserts `toMatch(/Scanning/i)` for an unindexed
   dir while indexing — change to assert `DIR` text + the hourglass indicator element, matching the shape of the sibling
   tests in that file.
3. Update `selection/CLAUDE.md` ("An unindexed dir shows `Scanning...` while indexing" sentence) and `views/CLAUDE.md` +
   `file-explorer/CLAUDE.md` where they narrate the scanning state.

### M5: Wrap-up

1. Full `pnpm check` (must include `oxfmt`).
2. Re-read the diff for style-guide compliance (sentence case, active voice in all new copy).
3. `pnpm check --include-slow` if e2e-adjacent risk feels real (indexing.spec.ts touches this area — run at least that
   spec locally per the "run only the affected test" rule, full slow lane before declaring done).
4. Update this spec's status, commit (no co-author line, repo conventions for message).

## Testing summary

| Layer            | What                                                                                                                                                                                             |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Unit (Vitest)    | `tooltip.test.ts` contentEl contract (incl. singleton-steal); `eta.test.ts` pure ETA helpers; `dir-size-display.test.ts` updated copy; `SelectionInfo.dir-size-state.test.ts` updated assertions |
| A11y (tier 3)    | New `IndexingStatusIndicator.a11y.test.ts`; deletions of the three obsolete a11y tests                                                                                                           |
| E2E (Playwright) | `indexing.spec.ts` should pass unchanged (asserts numeric sizes); run it explicitly                                                                                                              |
| Manual (MCP)     | Icon + tooltip during a real scan, both phases (scan, aggregation), reduced-motion spot check                                                                                                    |

## Copy inventory (all sentence case, active voice)

- Icon `aria-label`: "Drive indexing status"
- Size-column/status-bar tooltip: "Sizes are usually ready after 3 minutes"
- All existing dynamic labels stay verbatim ("Scanning... N entries, M dirs", "Computing directory sizes...", "Updating
  index...", "N events processed", "Almost done", "Ns left", "Nm left")

## Risks / open questions

- **Tooltip width jitter**: contentEl content changes size as counters tick; mitigate with a fixed `min-width` on the
  host content (D1). The tooltip renders at `font-size: var(--font-size-sm)` (larger than the old overlay's `xs`), and
  `.cmdr-tooltip` caps at `max-width: 320px` with `overflow-wrap: anywhere` — verify visually that the longest label
  ("Scanning... 1,200,000 entries, 340,000 dirs") doesn't wrap awkwardly.
- **Icon discoverability**: a bare icon is less discoverable than text. Accepted trade-off (the whole point is
  de-emphasis); the tooltip on hover/focus covers the curious.
- **`aria-describedby` timing**: the description only exists while the tooltip is shown; screen-reader users who focus
  the icon get the label + (after delay) the description. Acceptable, matches the rest of the app's tooltip usage.
- **Coverage checks**: the line-coverage gate is 70% per `.svelte` file (`desktop-svelte-tests.go`), and
  `IndexingStatusIndicator.svelte` is in scope. Extract everything reasonably pure into `eta.ts` (which clears its own
  coverage via unit tests) — but note all three predecessors were allowlisted in `coverage-allowlist.json` precisely
  because Tauri-event-driven overlay glue can't be meaningfully covered. If the thin remaining `.svelte` glue still
  falls under 70% after the extraction, an allowlist entry with a reason naming the untestable surface (module `$state`
  driven by Tauri events) is legitimate — adding it needs David's explicit OK per the allowlist rules, so surface it
  rather than over-extracting into unreadable indirection.
