# File explorer views

Virtual-scrolling file list components rendering 100k+ directories without DOM performance issues.

## Module map

`BriefList.svelte` / `FullList.svelte` are the two views (horizontal columns / vertical rows), with `FullListHeader`
(and its date `DateSortChevron`), `full-list-cache.svelte.ts`, `full-list-git-column.svelte.ts`, `full-list-mouse.ts`,
and `full-list-column-fit.ts` (a narrow pane drops the date, then Ext) beside Full, and `brief-column-widths.svelte.ts`
beside Brief. `virtual-scroll.ts` is pure window math, `measure-column-widths.ts` is pixel-accurate width measurement
via `@chenglou/pretext`, and the `*-utils.ts` trio holds the rendering helpers. `list-focus.svelte.ts` gives both
listboxes their focus and `aria-activedescendant` (❌ focus never on a row), and `file-row-a11y.ts` is what every row
says (id, name, marked, position); ❌ no per-view copy. `mark-run.ts` gives the marked-file bar its run ends from the
marked set by global index, ❌ never from rendered neighbors (the virtual window's edge would fake an end).

## Must-knows

- **Data lives in the Rust listing cache (`cmdr-listing`), ❌ never in Svelte `$state`.** The frontend fetches visible
  ranges (`getFileRange`); 20k+ entries in `$state` freeze the UI for 9+ seconds.
- **`hasParent = true` makes UI indices 1-based**: index 0 is the `..` entry (not in backend cache), so
  `cache_index = ui_index - 1`. Forgetting it lands the cursor one row off.
- **The renderer and the measurer are ONE contract; change both or neither.** `getDirSizeDisplayState`, `wordGitMeta`,
  and `extCellText` (`full-list-utils.ts`) are each the single source for a cell's text, reached by the renderer through
  `pickSizeDisplay` and by `measure-column-widths.ts` directly. Same for `listing.showExtensionInName` (drops the Ext
  track AND returns `ext: 0`), the `tabularize` call that models `tabular-nums` canvas can't measure, and the `HEADER_*`
  constants mirroring the header's chrome (caret, fallback glyph, date chevron). Re-inline any of them and text and
  width drift. `getDirSizeDisplayState`'s `updating` arg is PER ROW (`isSizeUpdating(entry)`), ❌ never a per-volume
  flag.
- **Full shows ONE date, `displayedDateColumn` of the sort its rows were fetched under** (`rowsSort`), in cells, header
  label, width, row names, and tooltip. ❌ Never the live `effectiveSortBy` (retained rows would show the next folder's
  date) or a view's own derivation. The header still sorts by the REQUESTED date. `DETAILS.md`.
- **`FullList`'s column header is a SIBLING ABOVE the scroll container**, paying the measured scrollbar width back as
  right padding. ❌ Don't move it in, and ❌ don't reintroduce a header-height shift between `scrollTop` and the spacer
  offset: the clamp then hides row 0, the `..` cursor.
- **Nothing visible in Brief mode may wait on the width IPC** (how the cursor went invisible in prod). ❌ Don't gate
  `is-under-cursor` on widths, fall back to `capPx`, infer readiness from `rawWidths.length`, or make `capPx` a fetch
  trigger.
- **Cold-context refreshes replace the visible window atomically.** Navigation, sort, hidden files, and refresh keep the
  old rows until the forced fetch lands; an epoch drops late rows, and retained rows are paint-only (`getEntryAt` /
  `indexOfEntry` skip them, else Enter acts on the old folder).
- **Rows open only through `row-activation.ts`** (mouse: `dblclick`; touch/pen: two quick clicks, as Chrome's device
  mode sends no `dblclick`), exactly once. ❌ No per-view double-click logic.
- **A row-asserting unit test mounts through `mountFullList()` / `mountBriefList()`** (`test-full-list.ts` /
  `test-brief-list.ts`): unmeasured, a view renders zero rows, so a negative assertion passes for free.
- **Row chrome shared by both views lives in `src/app-file-list.css`**, and every selector there keeps a
  `.full-list-container` / `.brief-list-container` prefix, or it loses specificity ties to
  `:global(.file-entry.folder-drop-target)`.

`DETAILS.md` argues every rule above and owns the rest: scroll via `transform` (❌ never absolute positioning), why
`$state()` can't live in a `.ts`, the per-getter cache deps, the gutter shifting `scrollTop` against the spacer, the
hidden-entry dim, and git-portal wording. Read it before any non-trivial work here.
