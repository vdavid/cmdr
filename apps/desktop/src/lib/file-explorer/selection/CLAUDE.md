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

- `formatSizeTriads(bytes)`: splits byte count into digit triads, each tagged with a `tierClass`. Uses U+2009 thin-space
  as separator between triads.
- `formatSizeForDisplay(bytes, { unit, format })`: single entry point used by views and the status bar to render byte
  counts. `unit: 'bytes'` delegates to `formatSizeTriads`. `unit: 'dynamic'` picks the friendliest unit per value ("1.02
  MB"). `unit: 'kB' | 'MB' | 'GB'` forces a fixed unit so a directory of mixed sizes reads apples-to-apples. Returns one
  tier-tagged span like `{ value: '1.02 MB', tierClass: 'size-mb' }` (or one per digit triad in bytes mode). **Tier
  color tracks the underlying byte magnitude in every mode**, not the displayed unit: a 349-byte file shown as
  `"0.00 MB"` (forced MB) still tiers as `size-bytes` (green) — same color a user gets from dynamic mode. Magnitude is
  derived via `dynamicTierIndex(bytes, format)` from `format-utils.ts`. The kilobyte label casing (`kB` vs `KB`) follows
  `format`.
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
- `formatNumber`, `calculatePercentage`: selection summary helpers. (Count + noun formatting goes through
  [`$lib/utils/pluralize`](../../utils/pluralize.ts).)

`sizeTierClasses` export: `['size-bytes', 'size-kb', 'size-mb', 'size-gb', 'size-tb']`. CSS rules for these classes must
exist in the consuming view, not here.

## `SelectionInfo.svelte`

Status bar rendered below each pane. Four display modes via `$derived displayMode`:

| Mode                | Condition                                               |
| ------------------- | ------------------------------------------------------- |
| `empty`             | `stats.totalFiles === 0 && stats.totalDirs === 0`       |
| `selection-summary` | `selectedCount > 0` (any view mode)                     |
| `no-selection`      | Full mode, no selection: shows total file/dir counts    |
| `file-info`         | Brief mode, no selection: shows name, size triads, date |

In `selection-summary` mode, directory recursive sizes are included in the size display when available (from the drive
index). The `hasOnlyDirs` branch shows size triads when `totalSize > 0`; when sizes are unavailable (indexing off), it
falls back to showing only dir count and percentage.

Stale indicator (Lucide hourglass icon via `~icons/lucide/hourglass`, rendered in accent color) appears in two places:

- `selection-summary` mode, when `isScanning()` is true and directories are selected (aggregate signal across the
  selection — a per-folder check wouldn't fit N mixed items).
- `file-info` mode, next to a directory's size, driven by the shared
  `getDirSizeDisplayState(displaySize, indexing, recursiveSizePending)` — the same decider FullList uses, so Brief's
  status bar matches Full's size column. Here `indexing = isScanning() || isAggregating()` (the aggregation phase
  matters too), and `recursiveSizePending` lights the hourglass during a live delete/copy even with no full scan. An
  unindexed dir always shows `DIR`; while indexing it adds a "Size not ready yet" hourglass (tooltip: "Sizes are usually
  ready after 3 minutes"), the same de-emphasized treatment Full's size column gives its `scanning` state. The
  per-folder `recursiveSizePending` flag lives only on `DirStats` (not `get_file_range`), so
  `FilePane.fetchEntryUnderCursor` overlays it onto the cursor entry via `updateIndexSizesInPlace([entry])` (skipping
  `..`, whose entry path is the parent folder), and re-runs on `index-dir-updated` so the hourglass tracks a storm live.

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

**Decision**: Size column / status bar primary readout follows the `listing.sizeUnit` setting. `Dynamic` (default) shows
"1.02 MB" via `formatFileSizeWithFormat`; `Bytes` shows colored digit triads via `formatSizeTriads`; `kB`/`MB`/`GB`
force a single unit per row so a mixed-size directory reads apples-to-apples. All modes flow through the shared
`formatSizeForDisplay` helper. **Why**: Dynamic is friendliest for most users, bytes lets power users compare
similarly-sized files exactly, and the forced units cover users who want every row in the same scale (David's case). The
tier-based CSS coloring (`size-bytes`/`size-kb`/`size-mb`/`size-gb`/`size-tb`) is preserved across all modes. In dynamic
mode the displayed unit IS the magnitude so the tier matches the label. In forced-unit modes the tier still tracks the
underlying byte magnitude (via `dynamicTierIndex`), not the displayed unit — a 349-byte file shown as `"0.00 MB"` keeps
the bytes-tier color, so the at-a-glance size signal survives even when every row reads in MB. The kilobyte label casing
(`kB` vs `KB`) follows the binary/SI setting and updates live in the settings UI's toggle group too. Tooltips on
file/dir/selection size still always show both formats so the other one is always one hover away.

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
