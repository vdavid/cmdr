# File explorer views details

Pull-tier docs for `apps/desktop/src/lib/file-explorer/views/`: architecture, flows, and decision rationale. Must-know
invariants and gotchas live in `CLAUDE.md`.

## Architecture

### Components

Where a symbol lives and who calls it: `codegraph_search` / `codegraph_explore`. The area's shape: `CLAUDE.md` § Module
map. Column shrink-wrapping, the sticky header, prefetch, cache invalidation, icon and tag-dot passes, the date column,
and `showExtensionInName` are all in § "Key decisions" below; the Git status column is in `../git/DETAILS.md`, and
`formatSizeForDisplay`'s bytes / dynamic / forced-unit modes in `../selection/DETAILS.md`. Three things live here
because nothing else carries them:

**The size column's content state (`full-list-utils.ts::getDirSizeDisplayState`).** One pure function of
`{recursiveSize, complete, stale, updating}` returns `'dir' | 'scanning' | 'lower-bound' | 'size' | 'size-stale'` (the
"honest sizes" model; see `$lib/indexing/DETAILS.md` § Honest size rendering). `FullList.svelte`'s size cell,
`SelectionInfo.svelte`'s Brief status bar, and `measure-column-widths.ts` ALL consume it, so rendered text and
pre-measured column width agree: don't re-inline the decision in any of them.

- An unknown size collapses to `'dir'` / `'scanning'` and shows the familiar `<dir>` placeholder rather than a
  settled-looking value. "Unknown" covers both not-yet-enriched and an incomplete subtree with nothing known below it
  (`complete === false` and `recursiveSize === 0`), kept distinct from a genuinely-empty `0 bytes` (`complete === true`,
  `recursiveSize === 0` → `'size'`).
- The lower-bound prefix is `LOWER_BOUND_GLYPH` (`≥`), a symbol rather than copy.
- **The in-flux hourglass is ORTHOGONAL to the content state**, not a sixth value: `isDirSizeUpdating` is the global
  `indexing` flag (a full scan or aggregation, every size in flux) OR the row's own `recursiveSizePending` (a live
  delete/copy for that dir with no scan running), so it rides on TOP of a size, a `≥` lower bound, or the placeholder.
  The `'scanning'` tooltip is "Sizes appear as the scan progresses", so a fresh install reads as quietly working rather
  than `Scanning...` on every row. Freshness-stale (`'size-stale'`) renders exactly like `'size'`: no glyph, no muting,
  with the staleness voiced by the per-drive freshness badge and the tooltip's stale line (see
  `$lib/indexing/DETAILS.md` § Honest size rendering). `measure-column-widths.ts` reserves `SIZE_ICON_WIDTH` whenever
  `isDirSizeUpdating`, or the shrink-wrapped column clips the glyph. The per-dir flag rides
  `DirStats.recursiveSizePending`, copied onto entries by `updateIndexSizesInPlace` / `createParentEntry` (backend:
  `indexing/read/pending_sizes.rs`).

**A file's size cell is dual-valued (logical vs physical on disk).** `full-list-utils.ts::getDisplaySize()` picks
between them per the `listing.sizeDisplay` setting (logical / physical / smart), `hasSizeMismatch()` decides whether the
two disagree enough to warn, and a `circle-alert` `<Icon>` marks the row when they do, gated by
`listing.sizeMismatchWarning`. `buildFileSizeTooltip()` / `buildDirSizeTooltip()` / `buildSelectionSizeTooltip()` spell
the pair out on hover. `measure-column-widths.ts` takes the SAME options, so the column shrink-wraps the text actually
rendered.

**`FullList`'s `staticEntries?: FileEntry[]` prop bypasses the backend-listing path entirely.** The array is mirrored
into the cache and the cache-fetch / soft-refresh / cache-generation paths short-circuit. The search-results virtual
volume is the user: it feeds full paths as the entries' `name` field, so the name cell mid-truncates via
`useShortenMiddle` (snapping to `/` when the name carries one, `.` otherwise). Unset, FullList renders identically: same
grid template, same fetch loop, same DOM.

### FullList's siblings

`FullList.svelte` keeps what needs the component (the props contract, the reactive readers, the `$effect`s, the DOM
refs, and the row template). Four siblings hold the rest, each with its own suite:

- **`full-list-cache.svelte.ts`** — the prefetch buffer plus the reset / soft-refresh / static-entries policy.
  `syncToProps(ready)` returns `'reset' | 'refresh' | 'none' | 'idle'`; the component reacts to `'reset'` only, by
  suppressing the width transition for one paint. **Each dep is its own getter, deliberately.** Collapsing them into one
  `props()` bag read whole makes every method subscribe to every prop, and the host's `$effect`s inherit that: the
  `..`-row stats would refetch on every `directory-diff` tick and the static-entries mirror would rewrite on any prop
  change at all.
- **`full-list-git-column.svelte.ts`** — the repo-relative status map and its watcher subscription. `watch()` returns a
  teardown, so the host's `$effect` cancels an in-flight load when the directory changes.
- **`full-list-mouse.ts`** — the pure mousedown plan (ignore / select / drag) and the drag payload, including the
  paths-by-value flavour a static-entries pane needs.
- **`FullListHeader.svelte`** — the sticky column header. It owns `.header-row` / `.header-icon` / `.header-name-ext` /
  `.header-git` (all self-contained: no rule reaches outside the header's own sub-tree), and reports its measured height
  back through `bind:height` because the virtual-scroll math subtracts it from the container.

### Where the row styles live

The row chrome that is IDENTICAL in both views lives in `src/app-file-list.css`: the `.is-striped` fill, the
`.is-selected` fill, the `--color-selection-fg` swap for a cursor-on-selected row, the hairline between consecutive
selected rows, the `.is-under-cursor` fill + outline + radius, and the `.restricted-indicator` icon's own chrome. One
copy, so the two views can't drift apart on how a selected or cursor row looks.

Every selector there is prefixed with `.full-list-container` / `.brief-list-container`. That's mandatory, not stylistic:
a lifted rule loses the class of specificity Svelte's scoping gave it, and a bare `.file-entry.is-selected` would tie
with `DualPaneExplorer`'s `:global(.file-entry.folder-drop-target)` and lose on source order. Full rationale and the
verification recipe: `src/DETAILS.md` § Global stylesheets.

Everything else stays per-view, deliberately:

- **FullList's column cascade** (~190 lines): `.col-name` / `.col-ext` / `.col-size` / `.col-date` / `.col-git`, the
  size-tier and date-age color rules, the rename-editor grid spans, the `.is-compact` padding, and the
  `--color-size-*-selected` collapse on a cursor-on-selected row. These are FullList's own cells; scoping is what keeps
  them from leaking, and it's what lets `css-unused` see the class as defined-and-used in one file.
- **BriefList's `.name`** rules and its `.header-row` layout, for the same reason.
- **The base `.file-entry` box.** Its `padding` / `gap` / `align-items` / `white-space` are the same in both views, but
  the rule also carries each view's `display` (grid vs flex), FullList's `transition` and `.is-compact` padding
  override, and BriefList's `overflow`. Splitting four declarations out would leave a husk in each component and put the
  base padding a file away from the override that adjusts it, so the rule stays whole in both.

Don't move the per-view cascade into a row component either: that's ~50 component instances per frame on the app's
hottest render path.

### Data flow

```
FilePane (parent)
  ├── listingId: string           (backend cache key)
  ├── totalCount: number           (for scrollbar sizing)
  ├── cursorIndex: number          (selection position)
  └── BriefList / FullList
        ├── cachedEntries: FileEntry[]   (prefetch buffer ~500 items)
        ├── cachedRange: {start, end}    (cached region)
        └── visibleFiles: FileEntry[]    ($derived from virtual window)
```

`BriefList` holds those three inline; `FullList` holds the same shape in `full-list-cache.svelte.ts` (`cache.entries` /
`cache.range` / `cache.windowRows()`).

**Key**: Data lives in Rust `LISTING_CACHE`. Frontend fetches visible ranges on-demand via
`getFileRange(listingId, start, count, includeHidden)`.

### Virtual scrolling

Uses a configurable row height via `getRowHeight()` from `reactive-settings.svelte.ts` (varies by density setting:
compact/comfortable/spacious). The virtual scroll uses an `itemSize` parameter from `VirtualScrollConfig`:

1. Calculate visible window: `startIndex = floor(scrollTop / itemSize)`
2. Add buffer above/below viewport (20 items default, configurable)
3. Render only `visibleFiles = entries.slice(startIndex, endIndex)`
4. Position via `transform: translateY(startIndex * itemSize)`
5. Spacer div maintains scrollbar accuracy: `height: totalCount * itemSize`

**Prefetch buffer**: ~500 items around current position, cached in `cachedEntries`. Reduces IPC calls during scroll.

Native drag auto-scroll uses the same scroll state and fetch path as keyboard/cursor scrolling, but one animation frame
at a time. `FullList.autoScrollDuringDrag(position, elapsedMs)` scrolls `scrollTop` vertically from top/bottom edge
bands; `BriefList.autoScrollDuringDrag(position, elapsedMs)` scrolls `scrollLeft` horizontally from left/right edge
bands. Both call `fetchVisibleRange()` when they move so newly revealed rows/columns can be hit-tested immediately by
the drag controller.

### Brief column widths and the font-metrics fill-in

`brief-column-widths.svelte.ts` owns the RAW backend text widths and the whole fetch policy; `BriefList.svelte` derives
what renders. `createBriefColumnWidths(deps)` returns `{ rawWidths, status, request, reset, destroy }`, and the pure
`clampColumnWidths(rawWidths, capPx)` / `provisionalColumnWidth(capPx)` turn raw numbers into rendered ones.
`get_brief_column_text_widths` answers with `{ widths, missingCodePoints }`; a non-empty `missingCodePoints` means some
characters were costed at the font's average width, so the store calls `fillMissingFontMetrics` (which measures them off
the main thread) and re-fetches once for exact widths. The full fill-in contract lives in `$lib/font-metrics/DETAILS.md`
§ On-demand fill-in.

**`WidthFetchAttempt` carries one flag per recovery step (`afterFontLoad`, `afterFill`, `retries`), and each bounds only
its own recursion.** That's load-bearing three ways. A fetch that already filled won't fill again, so a code point that
stays unmeasurable (it comes back in `missingCodePoints` regardless) can't drive an endless measure-and-refetch loop.
Because the flags are separate, a fetch that waited for `ensureFontMetricsLoaded` can still fill afterwards — collapsing
them into one `retry` boolean silently strands non-Latin names at the average width on exactly the path where the font
was measured fresh. And `retries` is its own counter, so waiting for the font doesn't consume a transient-failure retry.
❌ Don't merge them, and thread `{ ...attempt, … }` through recursive calls rather than a bare literal.

### Cursor visibility never waits on measurement

Three rules keep a Brief pane usable when the width IPC is slow, fails, or never answers. Each exists because its
absence shipped a bug in which the cursor was invisible and every column filled the pane, with nothing in the logs.

1. **The cursor highlight is unconditional.** `class:is-under-cursor={globalIndex === cursorIndex}`. ❌ Never gate it on
   widths having arrived: an async IPC then decides whether the user can see where they are.
2. **An unmeasured column renders at `provisionalColumnWidth(capPx)`**, roughly 260 px, never at `capPx`. `capPx` IS the
   pane width, so a `capPx` fallback makes one column swallow the whole view — which is what made a cursor stripe drawn
   at that width look wrong enough to suppress in the first place. The constant sits between the `MIN_COLUMN_WIDTH`
   floor (100) short names bottom out at and the 400 px `listing.briefColumnWidthMaxPx` ceiling users can opt into, so a
   provisional column reads as a column.
3. **Readiness is a state (`pending` / `ready` / `degraded`), never inferred from `rawWidths.length`.** An empty
   directory legitimately measures to `[]`, so length can't tell "no columns" from "no answer".
4. **Every failed attempt logs.** Retries log a `warn` naming the listing, the `BriefColumnsErrorKind`, and the attempt
   number; giving up logs one more. The original bug was undiagnosable from production logs precisely because both bail
   paths returned silently.

**Decision**: the pane width is NOT an input to the IPC, and there is no `capPx` effect. The backend measures TEXT,
which a resize can't change, so a resize re-clamps the widths already in hand: synchronous frontend math, no IPC, no new
chance to fail. **Why**: storing CLAMPED widths made `capPx` a fetch trigger, so every pane resize spent an IPC, and
each one could fail and (pre-fix) leave the pane with nothing. ❌ Don't reintroduce a `capPx`-change refetch.

**Decision**: a response is discarded only when the listing changed under it (an `epoch` bumped by `reset()`) or a NEWER
response already landed (a monotonic `requestId`). **Why**: the earlier guard bumped a generation on the way OUT of
every fetch, so merely ASKING again threw away an answer that was already in flight — and if the newer ask then failed,
the pane kept nothing. Asking is not answering.

Transient failures (`timeout`, `listingNotFound`, `other`, a thrown IPC) get two bounded retries at 150 ms and 400 ms,
cancelled on listing change and on unmount. `invalidItemsPerColumn` is a caller bug and is never retried.

All four rules are pinned end to end by `test/e2e-playwright/brief-cursor-visibility.spec.ts`, which arms the
`fail_next_brief_column_widths` E2E command (see `docs/testing.md` § E2E env-var hooks) and then asserts the cursor is
still drawn and every column is provisional-width. The failure has to be injected in the backend because a spec can't
make a healthy listing fail from JS: Tauri defines `window.__TAURI_INTERNALS__` and its `invoke` non-writable and
non-configurable, so wrapping them from a spec silently does nothing.

## Key decisions

**Decision**: `FullList`'s column header lives **inside** the scroll container as a `position: sticky; top: 0;` child,
not as a sibling above. **Why**: when the user has "Always show scrollbars" set (System Settings → Appearance),
non-overlay scrollbars steal a ~15 px gutter from the scroll container. A sibling header rendering at the wrapper's full
width then misaligned with the data rows below. Moving the header inside makes it share the row content width
automatically (and therefore the scrollbar gutter), so columns line up at every scrollbar mode without JS measurement.
Virtual-scroll math: the spacer follows the header in natural flow, so the spacer's content origin (row 0) sits
`headerHeight` pixels into the unscrolled document. The sticky header always covers the first `headerHeight` pixels of
the viewport once any scroll has happened, so the effective row area is `containerHeight - headerHeight`. Critically,
`scrollTop` and the spacer's scroll offset are the same number — no translation needed. `FullList` therefore derives
`spacerScrollTop = scrollTop` and `rowAreaHeight = containerHeight - headerHeight` and feeds those into
`calculateVirtualWindow` / `getScrollToPosition` / `firstVisibleGlobalIndex` / `lastVisibleGlobalIndex` /
`getVisibleItemsCountUtil`. `scrollToIndex` writes `getScrollToPosition`'s result straight to
`scrollContainer.scrollTop`. A11y: the listbox role moves off `.full-list` (now a generic scroll container) onto a
`.listbox-region` inner wrapper around `.virtual-spacer` so the sticky header isn't a direct child of the listbox (would
violate `aria-required-children`). The **"Empty folder" text is a sibling of that region, not a child**, for the same
rule: an empty listbox passes, a listbox holding a non-option child doesn't. `BriefList` keeps its overlay outside
`.brief-list` for the identical reason. ❌ Don't move either back inside to simplify the layout.

**`aria-activedescendant` names a row only when that row is RENDERED.** Both views derive the id from the rows in the
virtual window (`visibleColumns` / `visibleFiles`), not from `cursorIndex >= 0`: the cursor legitimately points at
nothing rendered in an empty folder, or when the user scrolls the cursor out of the window. Naming a missing id is a
critical axe violation (`aria-valid-attr-value`) and leaves a screen reader announcing a stale row. ❌ Don't "simplify"
it back to a `cursorIndex` bound like `cursorIndex < totalCount`, which still lies in the scrolled-away case. Pinned by
the empty-folder cases in `BriefList.a11y.test.ts` / `FullList.a11y.test.ts`.

**Don't reintroduce a `scrollTop - headerHeight` shift with a `Math.max(0, …)` clamp**: `scrollTop ∈ [0, headerHeight]`
then collapses to the same spacer state. PageDown × 2 → PageUp × 2 lands at `scrollTop === headerHeight`, hiding row 0
(including the `..` cursor) under the sticky header. The pinned regression is
`test/e2e-playwright/full-cursor-page-nav.spec.ts`.

**Decision**: Virtual scroll in frontend, data in backend **Why**: Sending 50k entries over IPC = 17.4MB, ~4s transfer.
Virtual scroll fetches only visible ~50 items on demand. Backend-driven caching eliminates serialization overhead.

**Decision**: Uniform row height per density setting (no variable height) **Why**: Variable height requires measuring
every row, defeating performance gains. Uniform height allows pure math: `scrollTop / itemSize = startIndex`.

**Decision**: Prefetch buffer (~500 items) **Why**: Smooth scrolling requires data ready before user sees blank space.
Buffer balances memory (small) vs. IPC latency (reduces fetches).

**Decision**: Cache invalidation via `cacheGeneration` prop **Why**: Changing sort, toggling hidden files, or resizing
window requires fresh data. Parent bumps `cacheGeneration`, triggering re-fetch. Uses `$effect()` to react.

**Decision**: Icon prefetching only for visible entries **Why**: With 50k files, prefetching all icons = 50k IPC calls.
Virtual scrolling renders only ~50 items, so prefetch only visible. Re-fetch on scroll. The same visible-range pass in
`fetchVisibleRange` also drives Tier-C custom-folder icons: it collects the visible directory rows' paths and calls
`prefetchCustomFolderIcons` (→ backend `get_custom_folder_icon_ids`), which runs the `kHasCustomIcon` `getxattr` only
for that bounded on-screen set and returns `path:{dir}` ids to fetch. The bulk listing never pays the per-entry syscall;
packages already arrive as `pkg:` ids from `get_icon_id`. `FilePane` evicts a directory's `path:*` / `pkg:*` icons via
`evictPerPathIconsForDir(loadedPath)` when its listing ends (navigation away / unmount), so a folder re-iconed while
away is re-detected next time it's shown.

**Decision**: Finder tag dots (`TagDots.svelte`) ride the same visible-range pass as custom-folder icons, and reserve
their cluster width in the column math **Why**: Tags (`com.apple.metadata:_kMDItemUserTags`) are a per-file `getxattr`,
too costly to read inline in the bulk listing (~6× an `lstat`), so the backend defers them. `fetchVisibleRange` calls
`commands.enrichTags(listingId, visiblePaths)` right beside `prefetchCustomFolderIcons` (gated by the `listing.showTags`
setting); the backend patches the cache and emits a coalesced `directory-diff`, which re-fetches the range and
re-renders the dots. `FilePane.handleListingComplete` additionally kicks off a low-priority **background sweep**
(`sweepTagsForListing`, 500-path chunks) so off-screen rows get tags too; it's cancelable — each chunk re-checks the
pane wasn't destroyed and the listing is still current (`loadGeneration` / `listingId`). The dots cluster at the right
edge of the Name cell: in **Full** mode the Name column is `1fr`, so flexbox gives the dynamic-space behavior for free
(name `flex: 1; min-width: 0` truncates, `TagDots` is `flex-shrink: 0`); in **Brief** mode columns are
width-constrained, so `brief_columns.rs` adds a per-row `tag_cluster_width` suffix (a pure function of the colored-tag
count, mirroring `tagClusterWidthPx` in `tag-dots-utils.ts` — keep the two in sync) before taking the per-column max, or
the dots would clip the next column. Tags arrive after first paint, so the column grows once when the tag batch lands:
one accepted "settle" per directory (D10). Only colored tags (index 1-7) draw a dot; colourless tags (index 0) are
dotless but still listed in the cluster's accessible label.

**Decision**: Brief columns shrink-wrap to the widest filename in each column, with the backend measuring widths and the
frontend rendering to those measurements **Why**: Long filenames deserve their full width while short ones let the user
scan more columns at once. The Rust backend owns the text data and the font metrics cache, so it computes the widest
filename's text width per column in one IPC call
(`get_brief_column_text_widths(listingId, itemsPerColumn, hasParent, fontId, includeHidden)`). The FE stores those raw
text widths and DERIVES `columnWidths` from them: add CSS chrome (icon + gaps + padding), clamp to
`[MIN_COLUMN_WIDTH, capPx]` where `capPx = min(usableWidth, the user's optional cap)`. A `prefixSums` array (`$derived`)
drives all virtual-scroll math: `totalSize` is the final prefix sum, `calculateVirtualWindowVariable` binary-searches
`prefixSums` for the visible range, and `getScrollToPositionVariable` looks up exact column edges. Scrollbar size and
cursor visibility now agree with what's actually rendered. `transition: width 300ms ease` still animates width changes
within a directory; nav resets snap via the `skipTransition` 2-rAF trick. Unmeasured columns render at
`provisionalColumnWidth(capPx)` and the cursor is drawn regardless; see § "Cursor visibility never waits on measurement"
below. The initial fetch after a dir change skips the 50 ms coalesce so that gap is as short as possible; re-fetches
during resize keep the coalesce.

**Decision**: A single `$effect` keeps the cursor in view, depending on `cursorIndex`, `containerWidth`,
`containerHeight`, and `columnWidths` **Why**: With exact prefix-sum math, every input that could move the cursor's
column out of view is a state read, so one consolidated effect replaces the old height-only effect plus the implicit
width-resize gap. It re-runs naturally when `columnWidths` changes (widths arriving, or a re-clamp after a resize), so a
fast resize-drag → fetch → widths-arrive sequence ends with `scrollToIndex(cursorIndex)` settling the view exactly once.
PageUp/PageDown step distance is content-dependent, derived from `prefixSums` directly (not from the container width),
so a "page" of skinny columns moves more files than a page of wide ones. Intentional UX: the step matches what's
visible.

**Decision**: Shrink-wrap Ext / Size / Modified columns from the rows **currently on screen**, not the prefetch buffer
or the full directory **Why**: The name column should keep every spare pixel, so columns track live content. Pretext's
canvas measurement is fast enough to recompute on every scroll row-crossing and window resize. The 300ms
`grid-template-columns ease` transition (on both `.header-row` and `.file-entry`) smooths the resulting width changes.
Dir switches snap instead of animating (see Gotcha below). The `..` row's (often huge) recursive size only contributes
when that row is actually on screen. Otherwise the size column would stay oversized after scrolling past it.
`SelectionInfo` keeps using `measureDateColumnWidth` (worst-case sampling) because it renders a single-entry snapshot
with no "visible set" to measure from.

**Decision**: The date column renders as one segment list with tabular figures, no split. **Why**: Earlier the column
split into a fixed-width date half plus a time half so the times lined up across rows despite proportional digits. With
`font-variant-numeric: tabular-nums` on `.col-date` every digit takes the same advance, and every token format (`YYYY`=4
digits, the rest zero-padded to 2) emits a fixed character count, so all dates are the same width and align on their
own. The contract: `formatDateForDisplay` (in `lib/settings/format-utils.ts`) returns a `FormattedDate` whose `segments`
is the ordered segment list; `computeFullListColumnWidths` measures the joined string once per row (tabular-aware, see
the digit gotcha below); `FullList` walks the segments, wrapping any with a non-null `ageClass` in an age-tier span.
Tooltips/MCP/status bar see the joined string via `FormattedDate.text`.

**Decision**: Column-width measurers (canvas in `full-list-utils.ts`, pretext in `measure-column-widths.ts`) cache their
measurer/context per text scale and rebuild on the **debounced** "settled" scale event from
`lib/text-size.svelte::onDebouncedScaleChange`, not on every reactive read. **Why**: the CSS layer reflows immediately
via `--font-scale`, so users see text grow live. Recomputing per-column widths on every slider step would thrash pretext
rebuilds. Coalescing to the same 1 s + idle window the font-metrics IPC uses keeps the UI smooth during drag and snaps
to correct widths once the user releases. `FullList` tracks the settle event via a local `scaleSettleTick` `$state` it
bumps from the subscription, then reads inside the column-width `$effect`. `BriefList`'s Brief-column widths come from
the backend `get_brief_column_text_widths` IPC, which uses the live font ID. The same `onDebouncedScaleChange` callback
triggers a refetch.

**Decision**: `listing.showExtensionInName` (default off) folds the extension back into the Name column and hides the
Ext column. **Why**: the Norton/Total Commander Name/Ext split confuses users who expect to see `launch.json` whole, not
`launch` with the `json` parked two columns over. Off keeps today's split. On: the Name cell renders `file.name`
verbatim (via `getNameColumnText`), and the Ext column header + cells aren't rendered. The renderer and the
width-measurer are one contract: `FullList`'s `gridTemplate` drops the Ext track and `computeFullListColumnWidths`
returns `ext: 0` in this mode, so the grid has no orphaned track and the Name column (`1fr`) absorbs the freed space.
The shared `getNameColumnText(name, isDirectory, showExtensionInName)` in `full-list-utils.ts` is the single name-text
decision both the cell and (implicitly, since name is `1fr` and unmeasured) the layout agree on.

Sort-by-extension keeps its CLICK affordance in this mode: `FullListHeader` splits the single Name-column header into
two `SortableHeader` triggers inside a `.header-name-ext` flex row (Name fills, Ext right-aligned and shrink-to-label),
both clickable, each showing its caret when active. The split lives INSIDE the `1fr` Name track, so the Ext trigger
costs the pane no column width and the measurer reserves none for it. ❌ Don't remove it: without it, `sort.byExtension`
(palette / shortcut) is the only route left. Pinned by `FullList.ext-in-name-header.test.ts` and
`FullListHeader.test.ts`.

Brief view is unaffected (it already renders `file.name` whole). The inline rename editor's column span shrinks in this
mode (`.col-rename.no-ext-col`) so it doesn't bleed into the Size column now that the Ext track is gone.

## Gotchas

**Gotcha**: The gutter that keeps the cursor and selection fills off the pane edges lives at a DIFFERENT level in each
view, and can't be hoisted to `FilePane`'s `.content`. **Why**: the column header has to keep spanning edge to edge. In
Full view the header is a `position: sticky` child of the scroll container, so the gutter sits on the inner
`.listbox-region` (rows only) and `.header-row` carries double horizontal padding instead, keeping its grid aligned with
the rows while its background stays full-bleed. In Brief view the header is a sibling ABOVE the scroll container, so the
gutter can sit on `.brief-list` itself. Padding it any further out (`.content`, `.full-list`) insets the header
background and leaves bare strips at both ends.

**Gotcha**: BOTH views measure their scroll surface with `bind:clientWidth` / `clientHeight`, which report the content
box PLUS the element's own padding — so any layout math that asks "how much fits" must subtract the gutter, or the
rightmost column and the last row of each column render clipped. Brief view derives `usableWidth` / `usableHeight` and
feeds those to the per-column cap, the virtual window, scroll-into-view, and items-per-column; the raw `containerWidth`
/ `containerHeight` survive only as `> 0` liveness checks.

**Gotcha**: Full view's vertical gutter shifts the virtual spacer inside the scroll content, so container `scrollTop`
and spacer offset differ by `GUTTER_PX`. **Why**: `.listbox-region`'s block padding sits ABOVE the spacer. Both
conversions are corrected — `spacerScrollTop` subtracts the gutter, and `scrollToIndex` adds it back when writing
`scrollContainer.scrollTop` (special-casing `0` so scrolling to the first row still shows the top gutter). Skipping
either drifts the cursor-into-view by a gutter at the list's ends. Each view keeps its own `GUTTER_PX`, which must stay
in sync with its element's `padding`.

**Gotcha**: Both views' header rows open with a dead `.header-icon` spacer, and their left inset is the scroll surface's
gutter PLUS `.file-entry`'s own padding. **Why**: without it the first heading sits above the file ICONS instead of the
file NAMES. Full view gets the alignment from sharing `gridTemplate` with the rows; Brief view's header is a plain flex
row, so it reproduces the row's geometry by hand (inset, icon-width spacer, `--spacing-sm` gap). `SortableHeader`'s
negative horizontal margin is what makes the LABEL, not the button box, land on the alignment point, so don't remove it
while "fixing" a 4px offset.

**Gotcha**: Both scroll containers carry a `data-file-list-surface` attribute (`.brief-list` and `.full-list`) — don't
drop it. **Why**: the pane's double-click-to-parent gesture (`pane/pane-background-dblclick.ts`, gated by
`behavior.doubleClickPaneNavigatesToParent`) hit-tests on it to tell "empty list background" from a row or the Full-view
sort header. It can't key off `[role="listbox"]`: in Full view the listbox spans only the rows, so the empty space below
a short listing falls outside it and the gesture silently no-ops there (the original bug). The surface fills the pane in
both views, so it covers that gap. Remove the attribute and double-click-to-parent quietly dies with no view-level test
catching it (the contract is covered in `pane/pane-background-dblclick.test.ts`).

**Gotcha**: `$state()` cannot live in `.ts` files **Why**: `virtual-scroll.ts` is pure functions. Reactive state must be
in `.svelte` or `.svelte.ts`. Math functions return plain objects consumed by `$derived` in components.

**Gotcha**: File watcher diffs shift indices while scrolled **Why**: If 20 files added before cursor, visible range
shifts by 20. Must recalculate virtual window when `totalCount` changes.

**Gotcha**: When `hasParent = true`, UI indices are 1-based **Why**: Index 0 is ".." parent entry (not in backend
cache). Real files start at index 1. Adjust: `cache_index = ui_index - 1`.

**Gotcha**: The ".." row shows the CURRENT folder's recursive size, not the parent folder's **Why**: The `..` row's size
column is otherwise wasted space. Showing the total for the folder the user is browsing (sum of everything visible plus
unloaded entries) answers "how much is in here?", more useful than "how big is the place I'd go if I pressed Backspace."
Implementation: `createParentEntry(parentPath, stats?)` in `file-list-utils.ts` takes optional stats;
`BriefList`/`FullList` fetch them via `getDirStatsBatch([currentPath])` on dir change and via
`updateIndexSizesInPlace(cachedEntries, currentPath)` on index refresh (single batch IPC call).

**Gotcha**: Scroll position must use `transform`, not absolute positioning **Why**: Absolute positioning causes full
layout recalc. `transform` uses GPU compositor for 60fps.

**Gotcha**: Cache re-fetch during scroll uses range expansion **Why**: If visible range is [100, 150] but cached is [0,
200], don't re-fetch. If scrolled to [250, 300], expand fetch to [0, 550] to include buffer. `shouldResetCache()`
handles this.

**Gotcha**: The Size and Modified columns render with `font-variant-numeric: tabular-nums`, but canvas/pretext can't
measure that OpenType feature (the canvas `font` shorthand has no slot for it). **Why**: `measure-column-widths.ts`
models it by substituting every digit with the font's widest digit (`tabularize`) before measuring, so the
shrink-wrapped column matches what the DOM draws. Without it, a row of narrow digits (`11/11/1111`) renders wider than
measured and ellipsizes. If you drop tabular figures from a numeric column, drop its `tabularize` call too.

**Gotcha**: `HEADER_CHROME_ACTIVE/INACTIVE` in `measure-column-widths.ts` are tied to `SortableHeader`'s flex gap +
caret glyph (4px gap + 8px caret = 12px active, 0px inactive). The button keeps 4px horizontal padding for hover-state
breathing room, but an equal negative margin (`margin: 0 calc(-1 * var(--spacing-xs))`) pulls it back out so the label
still lines up with the data cells below. Only gap+caret count toward the track width. **Why**: If you change those CSS
values or the caret size/markup, update the two constants or column widths drift. The values aren't derived from the
live DOM because pretext measurement runs without a reference element. Everything is computed from the pre-known chrome
formula.

**Gotcha**: Width transitions would "slide" on dir switches, because the header (FullList) and columns (BriefList)
persist across navs **Why**: When `shouldResetCache` fires, both lists set a `skipTransition` flag and clear it after
two `requestAnimationFrame` ticks (one to paint with `transition: none`, one more before re-enabling). FullList also
holds widths while `cachedEntries` is empty so the brief post-nav gap doesn't collapse to header-only floors. Combined,
nav = snap; within-dir scroll/resize/stream-in = animated.

**Gotcha**: CJK / complex-script filenames may be slightly mis-measured **Why**: The frontend canvas measurer
(`$lib/font-metrics/measure.ts`) iterates explicit Unicode ranges covering Latin, BMP-printable characters, and common
emoji (U+1F300–U+1FAFF). The backend stores those widths per code point and falls back to the cached `average_width` for
anything outside the measured set, so column widths for CJK, Arabic, and rare-symbol filenames are approximate. Emoji is
fine (measured). Latin is fine (measured). Expanding the measured set is a follow-up.

**Gotcha**: Index-size refresh (`refresh_listing_index_sizes`) triggers a column-width refetch through the existing
cache-reset path, not a separate trigger **Why**: When `recursive_size` enrichment lands, the listing may re-sort; the
existing `cacheGeneration` bump propagates into BriefList's reset-cache effect, which calls the width store's `reset()`
(dropping the widths and every in-flight response) then `request()`. Don't add a separate trigger: it would
double-fetch.

**Gotcha**: No `will-change: transform` on `.virtual-window` (`FullList.svelte`). **Why**: it force-promoted a permanent
GPU compositor layer that WebKit kept re-backing on every scroll/content change, ballooning `IOAccelerator` (GPU) memory
to 1+ GB under heavy re-render; the `translateY` scroll still composites on demand. Don't re-add it, and don't reach for
per-row `contain: layout paint` either (it backfires: one retained backing store per row). The full
GPU/compositor-memory investigation — findings, the reclaimable-not-a-leak conclusion, the measurement methodology and
its gotchas, and kick-off context for any future high-memory report — is in
`docs/notes/high-memory-gpu-compositor-investigation-2026-07.md`.
