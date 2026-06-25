# Selection display components

Renders selection state and file metadata in the status bar and list headers. Does NOT manage the selection set (that
lives in `FilePane.svelte` as a `Set<number>`).

## Key files

- **`selection-info-utils.ts`**: pure utilities (size/date formatting, display deciders), no DOM deps, fully tested.
- **`SelectionInfo.svelte`**: status bar below each pane. Four display modes derived from props.
- **`FileIcon.svelte`**: 16x16 icon with a bundled macOS-default fallback and overlay badges.
- **`SortableHeader.svelte`**: clickable column header with sort-direction triangle and shortcut tooltip.

## Must-knows

- **`sizeTierClasses` CSS rules must live in the consuming view, not in `selection-info-utils.ts`.** The util file is
  pure TypeScript with no DOM/style deps; the classes (`size-bytes`, `size-kb`, `size-mb`, `size-gb`, `size-tb`) are
  defined in the parent list view's stylesheet. A util that references a class it doesn't own keeps style ownership with
  the view layer.
- **Size-tier color tracks the underlying byte magnitude in every mode, not the displayed unit.** A 349-byte file shown
  as `"0.00 MB"` (forced-MB) still tiers as `size-bytes` (green), via `dynamicTierIndex(bytes, format)`. Don't tier off
  the displayed unit.
- **Age-tier mapping and the `appearance.dateColors` palette live in
  [`$lib/settings/age-tier-utils.ts`](../../settings/age-tier-utils.ts)**, not here; they belong with the setting. The
  renderer is [`$lib/ui/DateLabel.svelte`](../../ui/DateLabel.svelte). See `$lib/settings/CLAUDE.md` § "Date display".
- **`isBrokenSymlink` checks `iconId === 'symlink-broken'`, NOT filesystem flags.** The backend already resolves broken-
  symlink status when computing the icon ID; re-checking via stat would be redundant and possibly stale. Keep the
  frontend consistent with what the user sees.
- **`SelectionInfo` derives its display mode from props** (`viewMode`, `selectedCount`, `stats`), never an explicit
  `mode` prop. Keeps mode-determination in one place. The four modes: `empty`, `selection-summary`, `no-selection`
  (Full, no selection), `file-info` (Brief, no selection).
- **Middle truncation in `file-info` mode uses the `useShortenMiddle` action** (`$lib/utils/`) with `preferBreakAt: '.'`
  and `startRatio: 0.7`, NOT CSS `text-overflow: ellipsis`: CSS truncates from the right and loses the file extension.
  The action uses pretext for pixel-accurate measurement plus a built-in ResizeObserver.
- **Counts, size decimals, and triad separators all follow the active locale via `$lib/intl`** (`formatNumber`,
  `formatSizeTriads`); never hardcode a locale or separator. Keep an ASCII space between size value and unit, since
  `colorizeSizeString` parses the unit by the last ASCII space. See [`$lib/intl/CLAUDE.md`](../../intl/CLAUDE.md).
- **`SortableHeader`'s shortcut shows only when `isFocused` is true** (the `sort.by*` commands act on the focused pane).
  Hovering the unfocused pane's header shows the command name only; clicking still sorts that pane. Pinned by
  `SortableHeader.svelte.test.ts`.
- **`FileIcon` fallback is the bundled macOS default icon, not an emoji.** It shows only on cache miss (cold launch, or
  briefly after a theme/accent change clears the cache) and swaps seamlessly to the live accent-tinted OS icon once
  `get_icons` populates the cache. The component subscribes to `$iconCacheVersion` to re-render then.

## Status-bar hints (`SelectionInfo`)

- **Phone-storage hint (MTP)** tooltips the free-space readout on `caps.kind === 'mtp'` volumes (`mtpSpaceHint` from
  `FilePane`), explaining the folders-vs-used-space gap. See [DETAILS.md](DETAILS.md).
- **Stale (hourglass) indicator** appears when directory sizes may be incomplete: in `selection-summary` mode while
  `isScanning()` and dirs are selected, and in `file-info` mode via the shared `getDirSizeDisplayState(...)` (the same
  decider FullList uses, so Brief's status bar matches Full's size column). File sizes come from metadata and are always
  accurate, so the hint targets only directory sizes during scanning/aggregation.
- **Symlink hint (info glyph)** appears next to a directory's size in `file-info` mode when
  `entry.recursiveHasSymlinks === true`. Explains why a folder of symlinks may show `0 bytes`: Cmdr deliberately matches
  `du`/Finder by not double-counting symlinked content. The flag is computed by indexing (`recursive_has_symlinks` on
  `dir_stats`) and surfaced through enrichment.

Full details (the full `selection-info-utils` export catalog, per-mode conditions, `recursiveSizePending` overlay flow,
size-unit decision rationale, and component props): [DETAILS.md](DETAILS.md).
