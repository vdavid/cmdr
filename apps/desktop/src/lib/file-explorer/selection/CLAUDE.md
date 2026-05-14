# Selection display components

Renders selection state and file metadata in the status bar and list headers. Does NOT manage the selection set (that
lives in `FilePane.svelte` as a `Set<number>`).

## Key files

| File                           | Purpose                                              |
| ------------------------------ | ---------------------------------------------------- |
| `selection-info-utils.ts`      | Pure utilities, no DOM deps, fully tested            |
| `SelectionInfo.svelte`         | Status bar below each pane                           |
| `FileIcon.svelte`              | 16x16 icon with emoji fallback and overlay badges    |
| `SortableHeader.svelte`        | Clickable column header with sort direction triangle |
| `selection-info-utils.test.ts` | Unit tests for all util functions                    |
| `components.test.ts`           | Component render tests                               |

Per-component age-tier mapping (`tierForYear` / `tierForMonth` / `tierForDay` / `tierForTime`) and the
`appearance.dateColors` palette live in [`$lib/settings/age-tier-utils.ts`](../../settings/age-tier-utils.ts): they
belong with the setting, not the selection components. The renderer side is in
[`$lib/ui/DateLabel.svelte`](../../ui/DateLabel.svelte). See `$lib/settings/CLAUDE.md` § "Date display" for the full
pipeline.

## `selection-info-utils.ts`

Exported functions:

- `formatSizeTriads(bytes)`: splits byte count into digit triads, each tagged with a `tierClass`. Uses U+2009
  thin-space as separator between triads.
- `formatSizeForDisplay(bytes, { humanFriendly, format })`: single entry point used by views and the status bar to
  render byte counts. In raw-bytes mode delegates to `formatSizeTriads`. In human-friendly mode returns one tier-tagged
  span like `{ value: '1.02 MB', tierClass: 'size-mb' }`. The tier is picked from the chosen unit via
  `tierClassForUnit`, so coloring stays consistent with the triad mode.
- `tierClassForUnit(unit)`: maps the unit suffix from `formatFileSizeWithFormat` (`bytes`, `KB`/`kB`, `MB`, `GB`, `TB`,
  `PB`) to one of `sizeTierClasses`. TB and PB cap at `size-tb`.
- `formatDate(timestamp)`: Unix seconds → `"YYYY-MM-DD HH:MM:SS"` local time.
- `buildDateTooltip(entry, nowMs?)`: returns `{ html }` where each timestamp is wrapped in its age-tier span so the
  `appearance.dateColors` palette colors the date portion.
- `getSizeDisplay(entry, isBrokenSymlink, isPermissionDenied)`: returns triads array, `'DIR'`, or `null`.
- `getDateDisplay(entry, ...)`: returns formatted date string or `'(broken symlink)'` / `'(permission denied)'`.
- `isBrokenSymlink(entry)`: checks `entry.isSymlink && entry.iconId === 'symlink-broken'`. Does NOT use filesystem
  flags.
- `isPermissionDenied(entry)`: `!isSymlink && permissions === 0 && size === undefined`.
- `pluralize`, `formatNumber`, `calculatePercentage`: selection summary helpers.

`sizeTierClasses` export: `['size-bytes', 'size-kb', 'size-mb', 'size-gb', 'size-tb']`. CSS rules for these classes must
exist in the consuming view, not here.

## `SelectionInfo.svelte`

Status bar rendered below each pane. Four display modes via `$derived displayMode`:

| Mode                | Condition                                                |
| ------------------- | -------------------------------------------------------- |
| `empty`             | `stats.totalFiles === 0 && stats.totalDirs === 0`        |
| `selection-summary` | `selectedCount > 0` (any view mode)                      |
| `no-selection`      | Full mode, no selection: shows total file/dir counts     |
| `file-info`         | Brief mode, no selection: shows name, size triads, date  |

In `selection-summary` mode, directory recursive sizes are included in the size display when available (from the drive
index). The `hasOnlyDirs` branch shows size triads when `totalSize > 0`; when sizes are unavailable (indexing off), it
falls back to showing only dir count and percentage.

Stale indicator (Lucide hourglass icon via `~icons/lucide/hourglass`, rendered in accent color) appears in
`selection-summary` when `isScanning()` is true and directories are selected, because dir sizes may be incomplete during
scanning.

Symlink hint (Lucide info icon via `~icons/lucide/info`, rendered in tertiary text color) appears next to a directory's
size in `file-info` mode when `entry.recursiveHasSymlinks === true`. The tooltip reads: "This folder contains symlinks.
Symlinked content is not counted in the total to avoid double counting." This explains why a folder of symlinks may show
`0 bytes`. Cmdr deliberately matches `du`/Finder behavior. The flag is computed by the indexing module
(`recursive_has_symlinks` on `dir_stats`) and surfaced through enrichment.

Filename truncation in `file-info` mode uses the `useShortenMiddle` action with `preferBreakAt: '.'` to preserve file
extensions. The action uses pretext for canvas-based measurement and a built-in ResizeObserver.

Date column width is computed via `measureDateColumnWidth(formatDateTime)` to stay in sync with FullList.
`formatDateTime` comes from `reactive-settings.svelte`.

## `FileIcon.svelte`

Props: `file: FileEntry`, `syncIcon?: string` (URL for sync overlay badge).

- Primary: `<img>` from `getCachedIcon(file.iconId)`.
- Fallback: emoji via `getFallbackEmoji(file)` from `file-list-utils`.
- Symlink badge: 🔗 emoji, bottom-right by default. Moves to top-left when `syncIcon` is also present.
- Sync badge: 10×10px `<img>` at bottom-right.
- Reactivity: subscribes to `$iconCacheVersion` store, re-renders when the icon cache is populated.

## `SortableHeader.svelte`

Props: `column`, `label`, `currentSortColumn`, `currentSortOrder`, `onClick`, `align?` (`'left'` default, `'right'` for
numeric columns).

Renders a `<button>` with a sort-direction triangle (▲/▼). The triangle is `display: none` on inactive columns so it
doesn't reserve width. `FullList` shrink-wraps column widths and `opacity: 0` would have baked ~12px of dead space into
every unsorted header. Handles both `onclick` and `onkeydown` (Enter/Space).

## Key decisions

**Decision**: Size column / status bar primary readout follows the `listing.humanFriendlySizeUnits` toggle. ON (default)
shows "1.02 MB" via `formatFileSizeWithFormat`. OFF shows colored digit triads via `formatSizeTriads`. Both modes flow
through the shared `formatSizeForDisplay` helper. **Why**: Human-readable is friendlier for most users, but power users
(and David) want precise byte counts to compare similarly-sized files. The tier-based CSS coloring
(`size-bytes`/`size-kb`/`size-mb`/`size-gb`/`size-tb`) is preserved in both modes. In human-friendly mode the entire
formatted string takes the tier of its chosen unit. Tooltips on file/dir/selection size still always show both formats
so the other one is always one hover away.

**Decision**: Middle truncation in `file-info` mode uses the `useShortenMiddle` Svelte action (from `$lib/utils/`) with
`preferBreakAt: '.'` and `startRatio: 0.7`, not CSS `text-overflow: ellipsis` **Why**: CSS ellipsis truncates from the
right, losing the file extension. Middle truncation with dot-snapping preserves both the start of the filename and the
extension (e.g. `very-lon….txt`). The action uses pretext for pixel-accurate canvas measurement (no DOM reflow) with a
built-in ResizeObserver.

**Decision**: `SelectionInfo` derives display mode from props rather than accepting an explicit `mode` prop **Why**: The
display mode depends on `viewMode`, `selectedCount`, and `stats` together. Letting the component derive it internally
avoids duplicating the mode-determination logic in every parent and keeps the truth in one place.

**Decision**: `isBrokenSymlink` checks `iconId === 'symlink-broken'` instead of filesystem flags **Why**: The backend
already resolves broken symlink status when computing the icon ID. Re-checking via stat would be redundant and possibly
stale. Using `iconId` keeps the frontend consistent with what the user actually sees.

**Decision**: Stale indicator only shown when directories are selected during scanning **Why**: File sizes come from
metadata and are always accurate. Directory sizes come from the drive index (recursive scan). During scanning, directory
sizes may be incomplete, so the warning targets that specific case.

## Gotchas

**Gotcha**: `sizeTierClasses` CSS rules must be defined in the consuming view, not in `selection-info-utils.ts` **Why**:
The utility file is pure TypeScript with no DOM or style dependencies. The CSS classes it references (`size-bytes`,
`size-kb`, etc.) are defined in the parent list view's stylesheet, keeping style ownership with the view layer.

**Gotcha**: Thin space (U+2009) is used between digit triads, not a regular space **Why**: Regular spaces are too wide
for numeric grouping and look jarring in a compact status bar. Thin space matches typographic convention for digit
grouping and renders consistently across platforms.

**Gotcha**: `buildDateTooltip(entry, formatter)` returns `{ html }` and takes a `formatter` callback **Why**: Each
timestamp is rendered via `formatter` (the caller passes `formattedDate` from `reactive-settings.svelte.ts`), then the
year portion of each line is wrapped in an age-tier `<span>` so the tooltip picks up the active date palette. The
formatter callback keeps the util pure (no reactive imports here); the `tooltip` action accepts `{ html }` directly.

## Dependencies

- `../types`: `FileEntry`, `SortColumn`, `SortOrder`
- `../views/full-list-utils`: `measureDateColumnWidth`
- `../views/file-list-utils`: `getFallbackEmoji`
- `$lib/icon-cache`: `getCachedIcon`, `iconCacheVersion`
- `$lib/settings/reactive-settings.svelte`: `formatFileSize`, `formatDateTime`
- `$lib/indexing/index-state.svelte`: `isScanning`
