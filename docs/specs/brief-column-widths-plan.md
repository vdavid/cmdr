# Backend-driven exact column widths for Brief mode

## Why

The cursor in Brief mode gets visually clipped after horizontal scrolling, the scrollbar slightly overestimates total
content width, and window/pane resizes can leave the cursor off-screen. All three are symptoms of one root cause:

The shrink-wrap feature (commit `c336dbba`, "Brief mode: Variable column widths that fit filenames") made each column
size to its widest visible filename, but the virtual-scroll math still assumes uniform "cap" column widths. So:

- `scrollToIndex` computes `column.left = N * cap` and asks "is it in view?", but the column's real left edge is
  `virtualWindow.offset + sum(measured_widths[startIndex..N-1])` (usually less than `N * cap`). The math can answer "in
  view" while the column is actually clipped.
- `virtualSpacer.width = totalColumns * cap` overestimates real total width when columns shrink, so the scrollbar thumb
  is slightly too small and the user can scroll a few pixels past the last column.
- On resize, the height-change `$effect` retriggers `scrollToIndex`, but it operates on the cap math AND the per-column
  widths re-measure asynchronously afterwards, so the cursor column drifts out of view post-settle.
- Width resize (drag-pane-resizer, window resize) currently has no `scrollToIndex` retrigger at all.

The shrink-wrap feature is the right UX win: short filenames let users scan more columns at once, long ones get their
full width. The fix is to make the virtual-scroll math agree with the layout that's actually being rendered.

## What we're building

The Rust backend already has every filename in the listing AND the cached font metrics for the current size. It is the
natural place to compute the exact text width of the widest filename in every column for the current `itemsPerColumn`

- sort + filename set + font scale. With those widths shipped to the frontend as a `Vec<f32>`, the FE adds CSS chrome
  (icon + gaps + padding), clamps to `[MIN_COLUMN_WIDTH, capPx]`, and replaces uniform-cap virtual-scroll math with
  prefix-sum-based exact math. One source of truth for text measurement, scroll math agrees with layout, cursor
  visibility becomes trivial to enforce on resize.

Specifically:

1. New Rust function + IPC command:
   `get_brief_column_text_widths(listingId, itemsPerColumn, hasParent, fontId, includeHidden) → Vec<f32>`. Returns
   per-column _text-only_ widths; the FE owns chrome/clamp.
2. New frontend wiring in `BriefList.svelte` that calls this on layout/listing change, applies chrome+clamp, and stores
   `columnWidths: number[]`.
3. New horizontal virtual-scroll math that uses prefix sums of `columnWidths` for `totalSize`, visible window, and
   `scrollToIndex`.
4. A single `$effect` that keeps the cursor in view: re-runs on `cursorIndex`, `containerWidth`, `containerHeight`,
   `columnWidths` mutations.
5. Fix the hardcoded `font_id = "system-400-12"` in `get_max_filename_width` and `list_directory_start_with_volume`
   (both currently in `operations.rs` and `streaming.rs`). Pass the live font ID from the FE instead, so all
   measurements work at any text scale.
6. Delete the per-column FE pretext measurer (`measure-brief-column-widths.ts` + its test). The cap-based FE measurer is
   now redundant; the backend is authoritative for text widths.

## What we're NOT building

**Sub-pixel-precise widths for filenames with code points outside the measured set.** The frontend canvas measurer
(`apps/desktop/src/lib/font-metrics/measure.ts`) iterates explicit Unicode ranges covering Latin, BMP-printable
characters, and common emoji (U+1F300–U+1FAFF). Filenames using CJK, Arabic, complex scripts, or rare symbols outside
those ranges fall back to the cached `average_width` on the Rust side. Emoji and Latin filenames are measured correctly.
We accept a slight visual discrepancy for the unmeasured-range case in this change. Document this as a known limitation
in `views/CLAUDE.md` and `font_metrics/CLAUDE.md`. Expanding the measured character set is a separate, optional
follow-up.

**Removing the backend `max_filename_width` field from `ListingStartResult` / `listing-complete`.** It's still used by
the `FilePane` wiring for both Brief and (transitively) by `BriefList` as a cap fallback. We could simplify later by
deleting it once the new pipeline proves itself, but keeping it as a vestigial signal during this change keeps the diff
small. A follow-up task can prune.

**Incremental diff path for column widths.** When the file watcher diffs a single file in/out of the listing, we
recompute all column widths (one IPC call, debounced behind the same diff handler). Diffs are already debounced at ~200
ms in the watcher, recomputation cost is a few ms for 50k files, so the simpler "recompute on diff" path wins on
clarity. Incremental column-width patching ("only the affected column changed") is a future optimization if profiling
shows it matters.

**Pixel-perfect alignment with the CSS `transition: width 300ms ease` animation.** The animation still runs on column
elements when widths change. While the transition is running, the visual width is between old and new, but the
virtual-scroll math uses the new (final) widths. This was already true in the FE-measured world; not regressing.

## Design decisions

### Stateless IPC command, not a stateful subscription

**Why:** `get_brief_column_text_widths` takes the inputs it needs (`listingId`, `itemsPerColumn`, `hasParent`, `fontId`,
`includeHidden`) and returns `Vec<f32>`. No stored layout state in the backend, no `set_brief_layout` setter, no
`brief-widths-changed` event to wire up. The frontend calls it whenever any input changes. This matches the existing
pattern of `get_file_range`, `get_listing_stats`, `refresh_listing_index_sizes`: listing-scoped reads keyed by
`listingId`.

The cost is one extra IPC round-trip after each diff or resize. Diffs are already debounced; resize is already debounced
by the frontend before the call. Stateless is simpler and easier to reason about.

### Backend returns text-only widths; FE owns chrome and clamp

**Why:** Column chrome (icon 16 px + gap 8 px + padding 8 px + padding 8 px + 2 px rounding buffer = 42 px at scale 1)
is a sum of CSS design tokens (`var(--spacing-sm)`, icon size). If the tokens change, the FE picks up the new values
through `app.css`; if chrome formulas grow conditionals (scrollbar gutter, restricted-folder icon, future visual
tweaks), they evolve on the FE alongside the markup that draws them. Keeping the chrome formula on the FE means "single
source for the formula": the backend just measures text.

Same for clamps: `MIN_COLUMN_WIDTH = 100 px` and `MAX_BRIEF_COLUMN_WIDTH = min(containerWidth, 300 px)` are layout
concerns that depend on FE-only state (container width, density setting). Backend has no business knowing them.

Trade-off: the FE has to apply the same chrome to every column (one add + clamp per column, O(columns)). Trivially fast.

### Pass `fontId` from FE on every call; backend must NOT hardcode `"system-400-12"`

**Why:** The current backend `get_max_filename_width` and `list_directory_start_with_volume` both hardcode
`font_id = "system-400-12"`. This is a latent bug: at any non-default text scale the font ID changes to
`"system-400-15"` etc., the hardcoded lookup misses the cache, and `calculate_max_width` returns `None`. For Brief mode
columns, the FE pretext measurer was masking this (it measured per-column independently of the backend cap). Once we
delete the FE measurer, the bug stops being silent.

This change must fix both call sites: each takes `font_id: &str` (or builds it from a passed `scale: f32`, but the FE
already exposes `getCurrentFontId()` returning `"family-weight-size"`, so just pass that). Add a unit test pinning
non-default-scale behavior to prevent regression.

### Handling `None` from `calculate_max_width`: explicit fallback, not a footgun

**Why:** Even with `fontId` plumbed correctly, there's a real race: the user changes scale → font ID flips → cache miss
→ `ensureFontMetricsLoaded` schedules a `requestIdleCallback` re-measure → ~100–300 ms gap before the new metrics are
stored. If `get_brief_column_text_widths` runs during that gap, every column gets `None`.

Resolution: backend returns `Err(IpcError { message: "font_metrics_not_ready", timedOut: false })` (or a dedicated
variant) when `calculate_max_width` returns `None`. FE catches that specific error and:

1. Logs a debug message.
2. Schedules a retry after `ensureFontMetricsLoaded` resolves (it already returns a Promise).
3. In the meantime, falls back to rendering every column at `MAX_BRIEF_COLUMN_WIDTH` (the previous behavior
   pre-shrink-wrap).

This is rare (only on first scale flip) and the retry path is bounded.

### Frontend keeps `itemsPerColumn` derivation

**Why:** `itemsPerColumn = floor(containerHeight / rowHeight)` is a layout-only calculation, no listing data needed. The
FE knows `containerHeight` and `rowHeight` (from density setting). Passing it to the backend is mechanical, not moving
the computation. This also means the FE can detect "itemsPerColumn unchanged" and skip the IPC call entirely. A height
change of a few pixels that doesn't change row-fit doesn't trigger a recompute.

### Prefix-sum array on the frontend, not "ask the backend per scroll event"

**Why:** Once widths are received, the FE has all the data it needs to answer "what column is at scroll X?" and "what's
the X of column N?" via prefix sums (O(log n) binary search and O(1) lookup). Calling Rust per scroll event would add
IPC latency to scroll handling, which must stay 60 fps. Same approach as
[`pretext-word-wrap-plan.md`'s height-map architecture](./pretext-word-wrap-plan.md#prefix-sum-array-for-o-log-n-lookups).

The prefix-sum array is `Float64Array(columnWidths.length + 1)`, `cumWidth[0] = 0`,
`cumWidth[i+1] = cumWidth[i] + columnWidths[i]`. Computed once when `columnWidths` is received.

### Generation counter, keyed by `(listingId, generation)`

**Why:** During a fast resize-drag the FE may fire several `get_brief_column_text_widths` calls. They can complete out
of order, especially if Rust is busy (or, on slow folders, waiting on an `LISTING_CACHE` read lock). A stale response
overwriting fresh widths produces a flash of wrong layout. Worse: rapid pane navigation switches `listingId`, so a
response for the _previous_ listing might arrive after the new one mounted.

Implementation: FE keeps `let widthsGeneration = 0`; bumps it before each call; captures `(listingId, generation)` into
the awaited closure; on response, ignores if `listingId !== currentListingId` OR `generation !== widthsGeneration`. Same
pattern as the listing's `loadGeneration`, but with the listing-id key added so listing changes are self-invalidating.

### One `$effect` keeps the cursor visible, replacing scattered triggers

**Why:** Currently there's a height-only effect at `BriefList.svelte:512–520`, plus implicit reliance on FilePane
calling `scrollToIndex` when `cursorIndex` changes. With the new exact math, we can consolidate: a single `$effect` that
depends on `cursorIndex`, `containerWidth`, `containerHeight`, and `columnWidths` (reading `.length` is enough to wire
the dependency (Svelte 5 `$state` arrays track by value when reassigned). It runs `scrollToIndex(cursorIndex)`.
Replaces both the height-resize effect and any new width-resize effect we'd otherwise need, and naturally fires after
widths settle.

We don't need an extra "tick" indirection: assigning `columnWidths = result` retriggers the effect by virtue of
state-write semantics in Svelte 5. The fetch path naturally coalesces redundant work because the 50 ms debounce on
`fetchColumnWidths` only fires once per layout settle.

### Replace `virtual-scroll.ts`'s horizontal path with a width-array variant

**Why:** The current `calculateVirtualWindow` and `getScrollToPosition` assume uniform `itemSize`. The vertical case
(FullList) is still uniform, so those callers shouldn't change. Two paths:

- **Option A:** Add new functions, e.g. `calculateVirtualWindowVariable(widths, ...)` and
  `getScrollToPositionVariable(widths, index, ...)`. Keep the uniform versions for FullList. Both live in
  `virtual-scroll.ts`.
- **Option B:** Make the existing functions accept either `itemSize: number` or `itemSizes: number[]` (discriminated by
  typeof). Same surface, less duplication.

**Going with A.** The two paths have meaningfully different math (binary search vs. division). Keeping them as separate
named functions with explicit signatures is clearer for readers and tests. Co-locate them in the same file so the
relationship stays visible.

### Re-trigger on text-size settle, not on every scale tick

**Why:** Text-size changes already settle on a 1 s debounce via `onDebouncedScaleChange` (see
`lib/text-size.svelte.ts`). The font ID flips, font metrics get re-measured (canvas → Rust IPC → cache). Only after that
settles can we ask the backend for new widths. The backend always uses the _currently cached_ font ID, so the FE doesn't
need to pass it in.

Wiring: `BriefList` subscribes to `onDebouncedScaleChange` (already does, for the deleted measurer) and calls
`fetchColumnWidths()` from the same callback.

### File-watcher diffs trigger a single recompute

**Why:** `directory-diff` events already arrive throttled (~200 ms in the watcher). Each diff means the filename set
changed, so widths may have changed too. Wire the FE diff handler in `FilePane.svelte` (or wherever it lives, likely
`apply-diff.ts` consumer) to call `briefListRef.refetchColumnWidths()` after the listing's `cachedEntries` are patched.

For large drops/moves that trigger a full re-read, the listing's `cacheGeneration` bumps and `BriefList`'s existing
"reset cache" effect fires, naturally re-fetching widths through the same path.

### Sort change refetches widths after `resort_listing`

**Why:** Re-sorting reshuffles which filenames land in which column → widths change. The existing FilePane sort-change
path calls `resort_listing` and bumps `cacheGeneration`. BriefList's reset-cache effect then re-fetches widths. Same
path as listing-change. No new wiring needed beyond what cacheGeneration already triggers.

### All constants (`MIN_COLUMN_WIDTH`, `MAX_BRIEF_COLUMN_WIDTH`, chrome formula) live on the FE

**Why:** Following from the "backend returns text-only widths" decision: the FE owns clamping. `MIN_COLUMN_WIDTH = 100`
and `MAX_BRIEF_COLUMN_WIDTH = 300` stay as FE constants. The cap passed to columns is
`min(containerWidth, MAX_BRIEF_COLUMN_WIDTH)`. Chrome is added per-column on the FE.

`handleKeyNavigation`'s `visibleColumns` calculation (used for PageUp/PageDown step distance), currently
`Math.ceil(containerWidth / maxFilenameWidth)` at `BriefList.svelte:451`, switches to reading the live virtual window:
`virtualWindow.endIndex - virtualWindow.startIndex - 2 * bufferColumns` (the buffer is virtual padding, not visible).
Variable column widths mean PageUp/Down step distance is content-dependent, which is the right UX: a page of skinny
columns moves more files than a page of wide columns.

## Implementation milestones

### Milestone 1: Backend: compute exact text widths per column

**Files:**

- `apps/desktop/src-tauri/src/file_system/listing/brief_columns.rs` _(new)_: pure-logic module with one fn:
  `compute_brief_column_text_widths(listing_id, items_per_column, has_parent, font_id, include_hidden) → Result<Vec<f32>, BriefColumnsError>`.

  `BriefColumnsError` is an internal Rust enum (no `specta::Type` derive, does NOT cross IPC):

  ```rust
  enum BriefColumnsError {
      FontMetricsNotReady,
      InvalidItemsPerColumn,
      ListingNotFound(String),  // listing_id
      Other(String),
  }
  ```

  The IPC command wrapper (below) maps each variant to an `IpcError { message, timed_out: false }`:
  - `FontMetricsNotReady` → `IpcError { message: "font_metrics_not_ready", ... }`
  - `InvalidItemsPerColumn` → `IpcError { message: "invalid_items_per_column", ... }`
  - `ListingNotFound(id)` → `IpcError { message: "listing_not_found:{id}", ... }`
  - `Other(s)` → `IpcError { message: s, ... }` The FE branches on `error.message === "font_metrics_not_ready"` for the
    retry path. This follows the existing `IpcError` pattern documented in `commands/CLAUDE.md` § "Timeout-aware return
    types": no new wire-type precedent. Returns the widest filename's text-only width per column (no chrome, no clamp).
    Column 0 covers `..` + the first `items_per_column - 1` real entries when `has_parent`, and `items_per_column`
    entries otherwise; subsequent columns shift by `items_per_column - 1` if `has_parent`, else by `items_per_column`.
    The `..` filename itself contributes to column 0's measurement (it's a real glyph displayed in that column). Returns
    `Err(BriefColumnsError::FontMetricsNotReady)` if `calculate_max_width` returns `None` for any column; callers retry
    after metrics resettle. Errors out with `BriefColumnsError::InvalidItemsPerColumn` if `items_per_column == 0`. All
    returned `f32` values are guaranteed finite (no NaN, no Infinity); assert this in tests so the FE's `Float64Array`
    prefix sums never poison downstream math.

- `apps/desktop/src-tauri/src/file_system/listing/mod.rs`: pub-use `compute_brief_column_text_widths`.
- `apps/desktop/src-tauri/src/file_system/listing/brief_columns_test.rs` _(new)_: unit tests:
  - Empty listing → empty array.
  - Single column, single short name → array of length 1.
  - Long name (way wider than any cap) → backend returns its actual measured width unclamped (FE clamps).
  - Two columns, second shorter than first → widths differ accordingly.
  - `items_per_column = 0` → `Err(InvalidItemsPerColumn)`.
  - `has_parent = true` with `items_per_column = 5`: column 0 contains `..` + entries `[0..4]`; column 1 contains
    entries `[4..9]`; etc. (verify a known-width filename in the right column.)
  - Hidden-files inclusion respected.
  - Non-default `font_id` (e.g., `"system-400-15"`): store metrics for it, verify widths differ from the default.
  - Font ID not in cache → `Err(FontMetricsNotReady)`.
  - All returned values are finite (no NaN/Inf).
- `apps/desktop/src-tauri/src/commands/file_system/listing.rs`: new `#[tauri::command] #[specta::specta]` wrapper
  `get_brief_column_text_widths(listing_id, items_per_column, has_parent, font_id, include_hidden) -> Result<Vec<f32>, IpcError>`.
  Wrap in `blocking_result_with_timeout` (2 s, read path).
- `apps/desktop/src-tauri/src/lib.rs`: add the new command to `tauri::generate_handler!` AND `collect_commands!`.
- **Defer the hardcoded-font-id fix:** `get_max_filename_width` and `list_directory_start*` still hardcode
  `"system-400-12"`. Since Milestone 3 deletes the `get_max_filename_width` command and the `max_filename_width` field
  on `ListingStartResult` entirely (FullList doesn't consume the value; the new path replaces it for Brief), there's no
  reason to plumb `font_id` through them. The fix becomes "delete the latent bug" rather than "rewire it." Keep the
  deletion in Milestone 3 where the FE side also drops the prop, so the change is atomic.

Why this file split: a new module keeps the brief-mode-specific column math out of `operations.rs`, which is already
densely populated. The pure-logic function is unit-testable without Tauri / app handle.

**Logging:** `log::debug!(target: "brief_columns", "Computed {n} widths for listing {id} in {μs}μs")` when slow (>5 ms)
to catch regressions. `log::warn!(target: "brief_columns", "Font metrics not ready for {font_id}")` on the `None`
path.

**Bindings regen:** `cd apps/desktop && pnpm bindings:regen` after adding/changing commands. Update
`apps/desktop/src/lib/tauri-commands` consumers of the changed `get_max_filename_width` / `list_directory_start*`
signatures (now requiring `fontId`).

### Milestone 2: Frontend: receive text widths, apply chrome+clamp, replace per-column FE measurement

**Files:**

- `apps/desktop/src/lib/file-explorer/views/BriefList.svelte`:
  - Remove `columnWidthsMap`, `prevItemsPerColumn`, the scale-settle clear effect (lines 265–290 + lines 292–314), and
    the import of `measureWidestFilename`.
  - Add `MAX_BRIEF_COLUMN_WIDTH = 300` (new constant). The current code at `BriefList.svelte:139` uses `200` as a
    fallback-only floor (`Math.min(200, ...)`) when `backendMaxWidth` is undefined, which is not a "cap" in the new
    sense. Picking 300 px gives slightly more breathing room for long filenames; flag this as a deliberate behavior
    change in the commit message. `MIN_COLUMN_WIDTH = 100` stays; `COLUMN_PADDING` stays.
  - Add `let columnWidths = $state<number[]>([])`. Each entry is
    `clamp(backendTextWidth + COLUMN_PADDING, MIN_COLUMN_WIDTH, capPx)` where
    `capPx = min(containerWidth, MAX_BRIEF_COLUMN_WIDTH)`.
  - Derive `prefixSums` (`$derived`): running cumulative sum as `number[]` of length `columnWidths.length + 1`. A few
    hundred elements at most; plain array is fine. (Use `Float64Array` only if a future profiling pass points at this.
    Default is clarity.)
  - Add `async function fetchColumnWidths()`: generation-counter pattern keyed by `(listingId, generation)`. The
    generation bump and the `(listingId, generation)` capture both happen INSIDE the debounced callback (the actual-fire
    closure), not at every call to `fetchColumnWidths`. Otherwise a burst of 5 calls in 50 ms bumps generation 5 times
    for only 1 actual IPC, complicating reasoning. Inside the debounce: bump generation, capture
    `(currentListingId, currentGeneration)`, await
    `commands.getBriefColumnTextWidths(listingId, itemsPerColumn, hasParent, getCurrentFontId(), includeHidden)`; on
    response, check both `listingId === capturedListingId` AND `generation === capturedGeneration` before assigning. On
    success, applies chrome+clamp and stores `columnWidths = clampedArray`. On
    `IpcError { message: "font_metrics_not_ready" }`: log debug, `await ensureFontMetricsLoaded()` (exported from
    `$lib/font-metrics`), retry once. Bail after one retry, leaving `columnWidths = []` (fallback path uses
    `MAX_BRIEF_COLUMN_WIDTH`).
  - Wire `fetchColumnWidths` into:
    - The cache-reset effect (line 485–506): after `cachedEntries = []` + `cachedRange = ...`, kick off
      `fetchColumnWidths()`. The cache-reset path ALSO bumps `widthsGeneration` first, so in-flight responses for the
      previous listing are dropped.
    - A new `$effect` that re-runs when `itemsPerColumn` changes (track via local `prevItemsPerColumn`).
    - A new debounced `$effect` watching `containerWidth`, only refiring when `capPx = min(containerWidth, MAX)`
      actually changes, since the cap is the only width-derived input to fetched widths. Cheap `prevCapPx` guard. Add a
      4 px hysteresis on the cap-change check (`Math.abs(newCap - prevCap) >= 4`) so a transient scrollbar-gutter
      flicker (vertical scrollbar appearing/disappearing as content height crosses container height) doesn't trigger a
      refetch on a narrow pane where `capPx === containerWidth`. The cap drives the clamp only; sub-4-px cap changes
      don't visibly affect rendered columns.
    - The `onDebouncedScaleChange` callback (already subscribed for the deleted measurer).
    - The diff-handler path (cross-cutting; see Milestone 3 for the FilePane-side wiring).
  - `getColumnWidth(colIndex)` becomes `columnWidths[colIndex] ?? Math.min(containerWidth, MAX_BRIEF_COLUMN_WIDTH)`
    (fallback while `fetchColumnWidths` is in flight or recovered from `FontMetricsNotReady`).
  - Replace `virtualWindow = calculateVirtualWindow(...)` with
    `calculateVirtualWindowVariable(prefixSums, bufferColumns, containerWidth, scrollLeft, totalColumns)`, see
    Milestone 4.
  - Replace `scrollToIndex` body with the prefix-sum version (Milestone 4).
  - Replace the height-only effect (line 510–520) with a single "keep cursor in view" effect depending on `cursorIndex`,
    `containerWidth`, `containerHeight`, `columnWidths.length`.
  - `handleKeyNavigation` (line 451): replace `const visibleColumns = Math.ceil(containerWidth / maxFilenameWidth)`
    with an exact count derived from `prefixSums`. Implementation: count columns `c` for which
    `prefixSums[c] < scrollLeft + containerWidth && prefixSums[c + 1] > scrollLeft`. Two binary searches on `prefixSums`
    give the range in O(log n). Don't derive from `virtualWindow.endIndex - startIndex - 2 * bufferColumns`, as that
    formula underestimates at list edges where `endIndex` is clamped by `totalItems`, and the `Math.max(1, ...)` floor
    masks the bug rather than fixing it. Variable column widths mean PageUp/Down step distance is content-dependent,
    which is the right UX: a "page" of skinny columns moves more files than a "page" of wide columns.
- `apps/desktop/src/lib/file-explorer/views/measure-brief-column-widths.ts`: **delete**.
- `apps/desktop/src/lib/file-explorer/views/measure-brief-column-widths.test.ts`: **delete**.
- `apps/desktop/src/lib/file-explorer/views/CLAUDE.md`: update the "Decision: Brief columns shrink-wrap to widest
  visible filename" entry to reflect the new architecture. Add the CJK precision-loss limitation.

**Debouncing strategy:** `containerWidth` binds via `bind:clientWidth`, which fires per layout. Wrap the IPC call in a
tiny ~50 ms debounce (raw `setTimeout` + cleanup) to coalesce drag-resize bursts. Don't use a generic util; inline is
clearer here. `itemsPerColumn` is derived and changes discretely (when row count crossing happens), so no debounce
needed for that (it natural-throttles). After a debounced scale settle, all three triggers (scale callback,
itemsPerColumn possibly unchanged, capPx possibly unchanged) may fire, and the `prevItemsPerColumn` / `prevCapPx` guards
plus the 50 ms debounce coalesce these into a single IPC call.

### Milestone 3: Frontend: trigger refetch on file-watcher diffs; drop the now-vestigial `getMaxFilenameWidth` call

**Files:**

- `apps/desktop/src/lib/file-explorer/pane/FilePane.svelte`:
  - **Line ~1705–1714** currently calls `getMaxFilenameWidth(listingId, includeHidden)` after a diff and stores the
    result in `maxFilenameWidth`, which is then passed only to `BriefList` (line 2113). FullList does NOT consume
    `maxFilenameWidth` (grep confirms it). Replace the entire post-diff `getMaxFilenameWidth` call with
    `listRef?.refetchColumnWidths?.()` when the current view is Brief; do nothing when the current view is Full. Delete
    the `maxFilenameWidth = newMaxWidth` line and the `cacheGeneration++` that pairs with it (the cache generation
    should bump only when the listing actually changes, which the surrounding code already handles).
  - Export `refetchColumnWidths` from `BriefList.svelte` as part of the imperative API alongside `scrollToIndex` and
    `getEntryAt`.
  - `BriefList` no longer needs the `maxFilenameWidth` prop. Remove:
    - In `FilePane.svelte`: the `getMaxFilenameWidth` import (line 27), the `maxFilenameWidth` prop type (line 56), the
      `let maxFilenameWidth = $state<number | undefined>(undefined)` (line 172), all writes (lines 606, 623, 1091,
      1713), and the prop wiring at line 2113.
    - In `apps/desktop/src/lib/file-explorer/pane/types.ts` (line 8): `maxFilenameWidth: number | undefined` from
      `SwapState`.
    - In `apps/desktop/src/lib/file-explorer/types.ts` (line 95): `maxFilenameWidth?: number` from
      `ListingCompleteEvent`.
    - In `apps/desktop/src/lib/file-explorer/views/BriefList.svelte`: the `maxFilenameWidth` prop declaration and
      `backendMaxWidth` alias (lines 53, 88), plus the `calculatedColumnWidth` derivation that consumed it (lines
      138–144).
    - In `apps/desktop/src/lib/tauri-commands/file-listing.ts` and `apps/desktop/src/lib/tauri-commands/index.ts`: the
      `getMaxFilenameWidth` wrapper and re-export. This wraps up the "out of scope but vestigial" cleanup into this
      commit (leaving it in the codebase as dead state was the wrong call). See the updated "out of scope" section
      below.
- Debouncing: the watcher already debounces (~200 ms) at the Rust side, so most cases land as one batched diff event.
  Still wrap the FE `refetchColumnWidths` call in the same ~50 ms debounce as the resize trigger, so a quick succession
  of distinct events (diff + scale change in the same frame) collapses to one IPC call.

**Note on `get_max_filename_width` backend cleanup:** since FullList doesn't consume the value either, the backend
`get_max_filename_width` IPC command becomes unused once Milestone 3 lands. We can either delete it in this commit
(cleanest) or leave it for a follow-up. Plan recommends deletion: it's a dozen lines, only one consumer, no behavioral
risk. Same for the `max_filename_width` field on `ListingStartResult` and the `listing-complete` event payload: the FE
only ever stored the value in `FilePane.maxFilenameWidth` (now gone), so deleting the field is safe. Concrete Rust
deletions:

- `apps/desktop/src-tauri/src/ipc.rs:74, 522`: handler registration.
- `apps/desktop/src-tauri/src/file_system/mod.rs:34`: re-export.
- `apps/desktop/src-tauri/src/file_system/listing/mod.rs:15`: pub-use.
- `apps/desktop/src-tauri/src/file_system/listing/operations.rs:41, 115, 125, 181`: field on `ListingStartResult`, the
  calculation in `list_directory_start_with_volume`, and the standalone `get_max_filename_width` fn.
- `apps/desktop/src-tauri/src/file_system/listing/streaming.rs:58, 118, 171, 180, 258, 479, 549`: field on the
  `ListingCompleteEvent` payload, `emit_complete` trait signature, production `TauriListingEventSink::emit_complete`,
  test `CollectorListingEventSink::emit_complete` (the `_max_filename_width` underscored param goes too), the
  calculation site, and the emit call.
- `apps/desktop/src-tauri/src/commands/file_system/listing.rs:9, 222–223`: command wrapper.

And the docs ripple: `apps/desktop/src-tauri/src/file_system/listing/CLAUDE.md` line ~38 contains an ASCII data-flow
diagram with `maxFilenameWidth,` in the `listing-complete` payload. Trim it. (Added to the Milestone 5 doc-updates list
for visibility.)

**Updated Milestone 1 ripple:** the `font_id` plumbing for `get_max_filename_width` and `list_directory_start*` becomes
unnecessary if the command itself is deleted. Keep the `list_directory_start*` `font_id` plumbing iff we keep the
`max_filename_width` return field (for backwards-compat or future use). Recommendation: delete both; if a later feature
needs a directory-wide max, it can be re-added cleanly. **This simplifies Milestone 1: no signature change to existing
commands, only the new `get_brief_column_text_widths` command.**

### Milestone 4: Frontend: rewrite the horizontal virtual-scroll math

**Files:**

- `apps/desktop/src/lib/file-explorer/views/virtual-scroll.ts`:
  - Add
    `calculateVirtualWindowVariable(prefixSums, bufferColumns, containerWidth, scrollLeft, totalColumns) → VirtualWindow`.
    Binary-search `prefixSums` to find `firstVisibleIndex`, then walk forward to compute `endIndex` where
    `prefixSums[endIndex] > scrollLeft + containerWidth`. Apply buffer. Returns `totalSize = prefixSums[totalColumns]`
    and `offset = prefixSums[startIndex]`.
  - Add `getScrollToPositionVariable(prefixSums, index, scrollOffset, containerSize) → number | undefined`.
    `itemLeft = prefixSums[index]`, `itemRight = prefixSums[index + 1]`. Logic identical to the uniform version but
    using these.
  - Leave the existing uniform `calculateVirtualWindow` / `getScrollToPosition` intact for FullList.
- `apps/desktop/src/lib/file-explorer/views/virtual-scroll.test.ts` _(new or extended)_:
  - Empty widths → totalSize=0, startIndex=0, endIndex=0.
  - Single column wider than container.
  - Many small columns, scrolled to middle: startIndex/endIndex correct, offset matches `prefixSums[startIndex]`.
  - `getScrollToPositionVariable` returns `undefined` when fully visible; returns left-edge X when off-left; returns
    `right − containerSize` when off-right.
  - Buffer respected on both edges.

### Milestone 5: Tests, docs, and checks

**Vitest unit tests:**

- New ones above. Run: `cd apps/desktop && pnpm vitest run -t "virtual-scroll"`, `... -t "brief_columns"` etc.
- Update any FilePane / BriefList tests that mock the deleted measurer. Likely just removal of imports/setup.
- **`getCurrentFontId()` mocking:** the new `fetchColumnWidths` calls `getCurrentFontId()`, which reads `--font-scale`
  from `document.documentElement` (DOM). In jsdom, this returns `''` → fallback `1`. Tests that exercise non-default
  scale behavior must either stub `getCurrentFontId` directly or set the CSS variable on `document.documentElement`
  before mount. Document this in the test setup helper if you add one.

**Rust nextest:** `cd apps/desktop/src-tauri && cargo nextest run brief_columns`.

**E2E Playwright** (`apps/desktop/test/e2e-playwright/`):

- New test: generate a temp directory in the test setup containing ~200 files with mixed name lengths (short like
  `a.txt`, medium like the existing fixtures, long like `cmdr-e2e-playwright-mtp-1778621587.log`). Do NOT depend on
  `/tmp` content (it's host- and CI-non-deterministic). Use `playwright-e2e` feature flags or fs helpers per
  `apps/desktop/test/e2e-playwright/CLAUDE.md`'s conventions. Switch to Brief mode. Press Arrow Right ~50 times. After
  each press, assert `cursor.boundingBox()` is fully inside `briefList.boundingBox()` (left ≥ container.left, right ≤
  container.right, accounting for the header row offset). Same for Arrow Down then Arrow Right to traverse columns. Same
  for Home/End/PageUp/PageDown.
- Variant: resize the window mid-traversal. Drag the window narrower and verify cursor stays in view.
- **Disable CSS transitions** for the test via Playwright's `await page.emulateMedia({ reducedMotion: 'reduce' })`
  (Brief column widths transition over 300 ms; `boundingBox()` reads mid-animation values otherwise, producing
  intermittent failures). The `prefers-reduced-motion: reduce` media query already cuts column transitions to zero
  (`views/BriefList.svelte:790–794`).

**Documentation updates:**

- `apps/desktop/src/lib/file-explorer/views/CLAUDE.md`: rewrite the shrink-wrap decision entry; note the new IPC
  command in the data flow; add a "Gotcha: CJK / complex-script filenames may be slightly mis-measured because the
  backend uses cached average widths for code points outside the measured Latin + BMP-printable + emoji ranges. Emoji is
  fine (measured). Latin is fine (measured)." entry. Also note: PageUp/PageDown step is now content-dependent (derived
  from `virtualWindow`), not container-width-derived.
- `apps/desktop/src/lib/file-explorer/CLAUDE.md`: add a brief pointer to the new IPC command under "Architecture" if
  appropriate.
- `apps/desktop/src-tauri/src/font_metrics/CLAUDE.md`: note that `calculate_max_width` is now also the basis for
  per-column widths, not just the per-listing cap. List the CJK approximation. Update the "Gotcha" line that says "The
  frontend handles the `None` by falling back to its own width estimation." That's no longer accurate; FE now surfaces
  `FontMetricsNotReady` and retries after `ensureFontMetricsLoaded()`.
- `apps/desktop/src-tauri/src/file_system/listing/CLAUDE.md`: add `brief_columns.rs` to the Module structure table.
  Also trim `maxFilenameWidth,` from the `listing-complete` payload in the ASCII data-flow diagram (~line 38).
- `apps/desktop/src-tauri/src/commands/CLAUDE.md`: add the new command to the file map row for
  `file_system/listing.rs`. Note the `font_id` parameter addition to `get_max_filename_width` and
  `list_directory_start*`.

**Checks before declaring done:**

- `./scripts/check.sh --rust`: Rust compile, clippy, fmt, tests.
- `./scripts/check.sh --svelte`: TS, Svelte, vitest, eslint.
- `./scripts/check.sh`: full suite, including `oxfmt` and bindings-fresh. **Required before commit.**
- Manual UI check (by the user; sub-agent can't drive the dev server): navigate Brief mode in a large directory, press
  arrow keys, watch cursor stay visible. Resize window. Drag pane resizer. Toggle sort. Add a file via Cmd+N. All of
  these should keep the cursor visible and the scrollbar accurate.

## Risk areas

1. **Bindings-fresh check** can fail subtly if the new command's argument or return types are non-trivial. Regen and
   commit the updated `bindings.ts` in the same commit as the Rust command. `Vec<f32>` serializes to `number[]` via
   specta, but f32 `NaN`/`Infinity` JSON-encodes as `null`, which would poison the FE prefix sums. Backend MUST
   guarantee finite values: clamp inputs, assert outputs in tests.
2. **Stale `columnWidths` while in flight on first paint.** First mount: `cachedEntries` is empty, `fetchColumnWidths`
   hasn't returned yet. `getColumnWidth` falls back to `MAX_BRIEF_COLUMN_WIDTH` (or the container-derived cap). Columns
   render at the fallback width for one frame. To avoid a visible "snap" when widths arrive, use the same
   `skipTransition` 2-rAF trick that already exists for cache reset, applied when transitioning from "no widths" to
   "widths".
3. **`itemsPerColumn === 0`.** During a brief layout transition (window minimized, container height = 0), itemsPerColumn
   would be 0 (clamped to 1 in the existing code via `Math.max(1, ...)`). Keep that clamp. Backend rejects
   `items_per_column == 0` with `BriefColumnsError::InvalidItemsPerColumn`; FE never sends 0.
4. **Race between `directory-diff` and `cacheGeneration` reset.** A full re-read bumps `cacheGeneration`, which clears
   the listing cache on the FE and triggers width refetch. But mid-flight, a diff event for the _old_ listing might
   arrive and also trigger a refetch. The `(listingId, generation)` guard handles this, but ensure the FE generation is
   bumped on the cache-reset path too, not just on the IPC-call path. Test with rapid navigation between two
   directories.
5. **Backend can't access `LISTING_CACHE` write-locked.** The new fn takes a read lock on `LISTING_CACHE`. If another
   operation is holding the write lock (re-sort, watcher diff write), we wait. Acceptable for a function called once per
   layout change, but worth noting: under heavy churn, the call could block briefly. The `blocking_result_with_timeout`
   2 s wrapper covers the pathological case; on timeout, surface as `IpcError { timedOut: true }` and the FE retains
   existing `columnWidths`.
6. **`..` parent-entry offset interaction with `itemsPerColumn`.** When `hasParent`, FE indices are 1-based (`..` at FE
   index 0). Column 0 displays `..` + the first `itemsPerColumn - 1` real entries; column 1 displays the next
   `itemsPerColumn` real entries; etc., i.e., column 0 has one fewer real entry than other columns.

   **Backend resolution:** with `has_parent = true`, the backend computes column 0's width from entries
   `[0..items_per_column - 1)` of `visible_entries` AND from the literal `".."` string; column 1 from entries
   `[items_per_column - 1 .. 2 * items_per_column - 1)`; in general, column `c ≥ 1` covers
   `[c * items_per_column - 1 .. (c + 1) * items_per_column - 1)`. Last column may have fewer entries. With
   `has_parent = false`, column `c` covers `[c * items_per_column .. (c + 1) * items_per_column)` straight. Add a unit
   test pinning each branch.

7. **Vec\<f32\> payload size.** Worst case (50k files, `items_per_column = 1` from extreme narrow window): 50k columns ×
   ~10 bytes per JSON float (`"123.456,"`) ≈ 500 KB. Realistic (50k files, `items_per_column = 30`): ~14 KB. Acceptable;
   no streaming needed. Worth noting to head off "why not stream?" review questions.
8. **Font ID race with `ensureFontMetricsLoaded`.** First navigation after a scale change can land in the ~100–300 ms
   window where the new font ID isn't cached yet. The `FontMetricsNotReady` retry path handles it. Cover with a unit
   test that simulates the miss → load → retry sequence.
9. **Index-size updates → re-sort path.** `refresh_listing_index_sizes` re-enriches entries with `recursive_size`, which
   can change the sort order (and therefore which files land in which column). The existing `cacheGeneration` bump on
   re-sort propagates into BriefList's reset-cache effect, which triggers `fetchColumnWidths`; no explicit wiring
   needed. Worth a sentence in `views/CLAUDE.md` so a future agent doesn't try to add a separate trigger.

## Suggested commit boundaries

1. **`Brief mode: Backend computes exact per-column text widths`**: Milestone 1: new Rust module, command, tests,
   bindings regen. Fixes hardcoded `font_id` in existing `get_max_filename_width` / `list_directory_start*`. The new
   command is not wired in yet (body should say "wired up in commit 3"). Pure addition; scrolling still uses old math.
2. **`Brief mode: Variable virtual-scroll math`**: Milestone 4: new variable-width functions in `virtual-scroll.ts`
   - tests. Still not wired in. Pure addition.
3. **`Brief mode: Use backend widths for scrolling and rendering`**: Milestones 2 + 3: wire in the new path, delete the
   FE measurer, replace the cap-based math with prefix-sum math, consolidate the cursor-visibility effect, hook up
   diff/resize/scale triggers. Big commit; this is where behavior actually changes.
4. **`Brief mode: E2E test for cursor visibility under navigation and resize`**: Milestone 5 E2E test.
5. **`Docs: Backend-driven Brief column widths`**: Milestone 5 docs updates. Can fold into commit 3 if reviewers
   prefer; splitting keeps commit 3 reviewable.

## Parallelizable notes

Most of this is sequential by dependency. Two safe parallels (no worktrees needed, just call-batching in one agent):

- **Within Milestone 1:** the Rust module + its unit tests can be written together. The IPC command wrapper requires the
  module to exist first, but tests for the module don't.
- **Within Milestone 5:** the doc updates touch independent files (`views/CLAUDE.md`, `font_metrics/CLAUDE.md`,
  `listing/CLAUDE.md`, `commands/CLAUDE.md`). Batch all writes in one go.

Don't try to parallelize Milestone 2 against Milestone 3: they touch the same files (`BriefList.svelte`,
`FilePane.svelte`).

## Design principles invoked

- **Elegance above all** (Top 5 #2 in `AGENTS.md`): the fix is to make the two coordinate systems agree. Backend
  measures, frontend renders to those measurements, instead of patching one side to chase the other.
- **Smart backend, thin frontend** (Technicals #3): per-column width is logic, not display. Move it where the data is.
- **Subscribe, don't poll** (Technicals #6): we already get `directory-diff` events for content changes and
  `onDebouncedScaleChange` for size changes. The new IPC call is event-driven, not polled.
- **Invest in testability** (Technicals #7): pure Rust module + pure FE math module + E2E test for the visible bug.
- **The app should feel rock solid** (Top 5 #3): the cursor stays where the user expects it, even during fast
  resize/scroll bursts. The generation counter is specifically the "assume the hostile case" guard.

## Out of scope but worth noting for later

- Expanding the measured Unicode range (CJK, complex scripts) in `apps/desktop/src/lib/font-metrics/measure.ts` to close
  the precision gap for non-Latin filenames. One-time per-user cost (~few hundred ms re-measure at app start), unbounded
  benefit. Emoji is already measured; the gap is mostly East Asian and Arabic scripts.
- A one-time review of the `average_width` fallback strategy in `font_metrics/mod.rs`, e.g., a separate average per
  script range, once the font-metrics subsystem becomes more central. Refinement, not a blocker.

(The previously-listed "remove vestigial `max_filename_width`" follow-up was rolled into Milestone 3, leaving dead
state in the codebase wasn't the right call.)
