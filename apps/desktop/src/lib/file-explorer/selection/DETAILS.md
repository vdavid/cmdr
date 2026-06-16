# Selection display components details

Depth for the status-bar / header components. `CLAUDE.md` holds the must-knows.

## `selection-info-utils.ts` exports

- `formatSizeTriads(bytes)`: splits a byte count into digit triads, each tagged with a `tierClass`. The inter-triad
  separator follows the active locale (`getGroupSeparator` from `$lib/intl`), so byte grouping matches localized counts;
  see [`$lib/intl/DETAILS.md`](../../intl/DETAILS.md) (Decision 4, the en-US comma vs old thin-space change).
- `formatSizeForDisplay(bytes, { unit, format })`: single entry point used by views and the status bar. `unit: 'bytes'`
  delegates to `formatSizeTriads`; `unit: 'dynamic'` picks the friendliest unit per value ("1.02 MB");
  `unit: 'kB' | 'MB' | 'GB'` forces a fixed unit so a mixed-size directory reads apples-to-apples. Returns one
  tier-tagged span (or one per digit triad in bytes mode). Tier color tracks the underlying byte magnitude in every mode
  via `dynamicTierIndex(bytes, format)` from `format-utils.ts`. The kilobyte label casing (`kB` vs `KB`) follows
  `format`.
- `tierClassForUnit(unit)`: maps the unit suffix from `formatFileSizeWithFormat` (`bytes`, `KB`/`kB`, `MB`, `GB`, `TB`,
  `PB`) to one of `sizeTierClasses`. TB and PB cap at `size-tb`.
- `formatDate(timestamp)`: Unix seconds → `"YYYY-MM-DD HH:MM:SS"` local time.
- `buildDateTooltip(entry, formatter)`: returns `{ html }`. Each timestamp is rendered via the `formatter` callback (the
  caller passes `formattedDate` from `reactive-settings.svelte.ts`), then the year portion of each line is wrapped in an
  age-tier `<span>` so the tooltip picks up the active date palette. The callback keeps the util pure (no reactive
  imports); the `tooltip` action accepts `{ html }` directly.
- `getSizeDisplay(entry, isBrokenSymlink, isPermissionDenied)`: returns triads array, `'DIR'`, or `null`.
- `getDateDisplay(entry, ...)`: returns the formatted date or `'(broken symlink)'` / `'(permission denied)'`.
- `isBrokenSymlink(entry)`: `entry.isSymlink && entry.iconId === 'symlink-broken'` (not filesystem flags).
- `isPermissionDenied(entry)`: `!isSymlink && permissions === 0 && size === undefined`.
- `formatNumber`, `calculatePercentage`: selection summary helpers. `formatNumber` delegates to `formatInteger`
  (`$lib/intl`), so counts group per the active locale. Count + noun formatting goes through
  [`$lib/utils/pluralize`](../../utils/pluralize.ts).
- `sizeTierClasses`: `['size-bytes', 'size-kb', 'size-mb', 'size-gb', 'size-tb']`. CSS rules for these live in the
  consuming view, not here.

## `SelectionInfo.svelte` display modes

Four `$derived displayMode` values, each with its condition:

- `empty`: `stats.totalFiles === 0 && stats.totalDirs === 0`.
- `selection-summary`: `selectedCount > 0` (any view mode). Directory recursive sizes are included when available (from
  the drive index). The `hasOnlyDirs` branch shows size triads when `totalSize > 0`; when sizes are unavailable
  (indexing off), it falls back to dir count and percentage.
- `no-selection`: Full mode, no selection: total file/dir counts.
- `file-info`: Brief mode, no selection: name, size triads, date.

Stale-size hourglass detail: in `file-info` mode the shared
`getDirSizeDisplayState(displaySize, indexing, recursiveSizePending)` drives it, with
`indexing = isScanning() || isAggregating()`. An unindexed dir shows `DIR`; while indexing it adds a "Size not ready
yet" hourglass (tooltip "Sizes appear as the scan progresses"). The per-folder `recursiveSizePending` flag lives only on
`DirStats` (not `get_file_range`), so `FilePane.fetchEntryUnderCursor` overlays it onto the cursor entry via
`updateIndexSizesInPlace([entry])` (skipping `..`, whose entry path is the parent folder) and re-runs on
`index-dir-updated` so the hourglass tracks a storm live.

Other layout: filename truncation uses `useShortenMiddle` with `preferBreakAt: '.'`. Date column width is computed via
`measureDateColumnWidth(formatDateTime)` to stay in sync with FullList; `formatDateTime` comes from
`reactive-settings.svelte`.

## `FileIcon.svelte`

Props: `file: FileEntry`, `syncIcon?: string` (URL for sync overlay badge).

- Primary: `<img>` from `getCachedIcon(file.iconId)`.
- Fallback (cache miss only): bundled macOS default folder/file icon (`static/icons/default-folder.png` /
  `default-file.png`, extracted from the system `GenericFolderIcon`/`GenericDocumentIcon`), chosen by `isFolderIcon`.
- Symlink badge: the `link` glyph via `<Icon>` (size 10, `--color-accent-pop`, a mode-aware higher-contrast accent),
  bottom-right by default, moving to top-left when `syncIcon` is present.
- Sync badge: 10×10px `<img>` at bottom-right.
- Reactivity: subscribes to `$iconCacheVersion`, re-renders when the cache is populated.

## `SortableHeader.svelte`

Props: `column`, `label`, `currentSortColumn`, `currentSortOrder`, `onClick`, `align?` (`'left'` default, `'right'` for
numeric columns), `isFocused?` (`true` default; pass the pane's focus state).

Renders a `<button>` with a sort-direction triangle (▲/▼). The triangle is `display: none` on inactive columns so it
doesn't reserve width (`FullList` shrink-wraps column widths; `opacity: 0` would bake ~12px of dead space into every
unsorted header). Handles `onclick` and `onkeydown` (Enter/Space). The tooltip carries the sort command's name plus its
current keyboard shortcut as a `<kbd>` chip: the command id derives from `column` via the internal
`columnToCommandIdMap`, the shortcut from `getFirstShortcutReactive` (`$lib/shortcuts/reactive-shortcuts.svelte`), so a
rebind updates live. The shortcut shows only when `isFocused` is true (truthfulness rule), and the tooltip action
live-updates so a focus flip or rebind mid-hover is reflected immediately.

## Decisions

- **Size readout follows the `listing.sizeUnit` setting** (`Dynamic` default / `Bytes` / forced `kB`/`MB`/`GB`), all
  flowing through `formatSizeForDisplay`. Dynamic is friendliest; bytes lets power users compare exactly; forced units
  put every row on the same scale (David's case). Tier coloring is preserved across all modes (tracking byte magnitude,
  not the displayed unit). Tooltips on file/dir/selection size always show both formats.
- **Middle truncation via `useShortenMiddle`**, not CSS ellipsis, to preserve file extensions (`very-lon….txt`). Pretext
  gives pixel-accurate measurement with no DOM reflow.
- **`SelectionInfo` derives display mode from props** so the mode-determination logic isn't duplicated in every parent.
- **`isBrokenSymlink` checks `iconId`** rather than re-statting, keeping the frontend consistent with what's rendered.
- **Stale indicator only when directories are selected during scanning**: file sizes are always accurate; directory
  sizes come from the recursive scan and may be incomplete.

## Dependencies

- `../types`: `FileEntry`, `SortColumn`, `SortOrder`.
- `../views/full-list-utils`: `measureDateColumnWidth`.
- `$lib/icon-cache`: `getCachedIcon`, `iconCacheVersion`.
- `$lib/settings/reactive-settings.svelte`: `formatFileSize`, `formatDateTime`.
- `$lib/indexing/index-state.svelte`: `isScanning`.
