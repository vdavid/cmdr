# File explorer views

Virtual-scrolling file list components rendering 100k+ directories without DOM performance issues.

## Module map

`BriefList.svelte` / `FullList.svelte` are the two views (horizontal columns / vertical rows), with `FullListHeader`,
`full-list-cache.svelte.ts`, `full-list-git-column.svelte.ts`, and `full-list-mouse.ts` beside Full, and
`brief-column-widths.svelte.ts` beside Brief. `virtual-scroll.ts` is pure window math, `measure-column-widths.ts` is
pixel-accurate width measurement via `@chenglou/pretext`, and the `*-utils.ts` trio holds the rendering helpers.

## Must-knows

- **Data lives in Rust `LISTING_CACHE`, ❌ never in Svelte `$state`.** The frontend fetches visible ranges on demand
  (`getFileRange`); only the visible window enters reactivity. Loading 20k+ entries in causes 9+ second freezes.
- **`hasParent = true` makes UI indices 1-based**: index 0 is the `..` entry (not in backend cache), so
  `cache_index = ui_index - 1`. Forgetting it lands the cursor one row off.
- **The renderer and the measurer are ONE contract; change both or neither.** `getDirSizeDisplayState` and `wordGitMeta`
  (`full-list-utils.ts`) are each the single source for a cell's text, reached by the renderer through `pickSizeDisplay`
  and by `measure-column-widths.ts` directly. Same for `listing.showExtensionInName` (drops the Ext track AND returns
  `ext: 0`), the `tabularize` call that models `tabular-nums` canvas can't measure, and `HEADER_CHROME_ACTIVE/INACTIVE`
  mirroring `SortableHeader`'s gap + caret. Re-inline any of them and text and width drift. `getDirSizeDisplayState`'s
  `updating` arg is PER ROW (`isSizeUpdating(entry)`), ❌ never a per-volume flag.
- **`FullList`'s column header is a SIBLING ABOVE the scroll container**, paying the measured scrollbar width back as
  right padding. ❌ Don't move it in, and ❌ don't reintroduce a header-height shift between `scrollTop` and the spacer
  offset: the clamp then hides row 0, the `..` cursor.
- **Nothing visible in Brief mode may wait on the width IPC** (how the cursor went invisible in prod). ❌ Don't gate
  `is-under-cursor` on widths, fall back to `capPx`, infer readiness from `rawWidths.length`, or make `capPx` a fetch
  trigger.
- **A row-asserting unit test mounts through `mountFullList()` / `mountBriefList()`** (`test-full-list.ts` /
  `test-brief-list.ts`): unmeasured, a view renders zero rows, so a negative assertion passes for free.
- **Row chrome shared by both views lives in `src/app-file-list.css`**, and every selector there keeps a
  `.full-list-container` / `.brief-list-container` prefix, or it loses specificity ties to
  `:global(.file-entry.folder-drop-target)`.

`DETAILS.md` argues every rule above and owns the rest: scroll via `transform` (❌ never absolute positioning), why
`$state()` can't live in a `.ts`, the per-getter cache deps, the gutter shifting `scrollTop` against the spacer, the
hidden-entry dim, and git-portal wording. Read it before any non-trivial work here.
