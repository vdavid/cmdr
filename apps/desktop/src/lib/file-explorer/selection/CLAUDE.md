# Selection display components

Renders selection state and file metadata in the status bar and list headers. Does NOT manage the selection set — that
lives in `FilePane.svelte` as a `Set<number>`.

## Key files

| File                           | Purpose                                              |
| ------------------------------ | ---------------------------------------------------- |
| `selection-info-utils.ts`      | Pure utilities — no DOM deps, fully tested           |
| `SelectionInfo.svelte`         | Status bar below each pane                           |
| `FileIcon.svelte`              | 16x16 icon with emoji fallback and overlay badges    |
| `SortableHeader.svelte`        | Clickable column header with sort direction triangle |
| `selection-info-utils.test.ts` | Unit tests for all util functions                    |
| `components.test.ts`           | Component render tests                               |

## `selection-info-utils.ts`

Exported functions:

- `formatSizeTriads(bytes)` — splits byte count into digit triads, each tagged with a `tierClass`. Uses U+2009
  thin-space as separator between triads.
- `formatDate(timestamp)` — Unix seconds → `"YYYY-MM-DD HH:MM:SS"` local time.
- `buildDateTooltip(entry)` — multiline string with created/opened/added/modified dates.
- `getSizeDisplay(entry, isBrokenSymlink, isPermissionDenied)` — returns triads array, `'DIR'`, or `null`.
- `getDateDisplay(entry, ...)` — returns formatted date string or `'(broken symlink)'` / `'(permission denied)'`.
- `isBrokenSymlink(entry)` — checks `entry.isSymlink && entry.iconId === 'symlink-broken'`. Does NOT use filesystem
  flags.
- `isPermissionDenied(entry)` — `!isSymlink && permissions === 0 && size === undefined`.
- `pluralize`, `formatNumber`, `calculatePercentage` — selection summary helpers.

`sizeTierClasses` export: `['size-bytes', 'size-kb', 'size-mb', 'size-gb', 'size-tb']`. CSS rules for these classes must
exist in the consuming view, not here.

## `SelectionInfo.svelte`

Status bar rendered below each pane. Four display modes via `$derived displayMode`:

| Mode                | Condition                                                |
| ------------------- | -------------------------------------------------------- |
| `empty`             | `stats.totalFiles === 0 && stats.totalDirs === 0`        |
| `selection-summary` | `selectedCount > 0` (any view mode)                      |
| `no-selection`      | Full mode, no selection — shows total file/dir counts    |
| `file-info`         | Brief mode, no selection — shows name, size triads, date |

Stale indicator (`⚠️`) appears in `selection-summary` when `isScanning()` is true and directories are selected (dir
sizes may be incomplete).

Filename truncation in `file-info` mode uses a ResizeObserver + throwaway `<span>` measurement for middle truncation
(preserves file extension). The truncation runs binary search via `getTruncatedName`, triggered reactively by
`containerWidth` state.

Date column width is computed via `measureDateColumnWidth(formatDateTime)` to stay in sync with FullList —
`formatDateTime` comes from `reactive-settings.svelte`.

## `FileIcon.svelte`

Props: `file: FileEntry`, `syncIcon?: string` (URL for sync overlay badge).

- Primary: `<img>` from `getCachedIcon(file.iconId)`.
- Fallback: emoji via `getFallbackEmoji(file)` from `file-list-utils`.
- Symlink badge: 🔗 emoji, bottom-right by default. Moves to top-left when `syncIcon` is also present.
- Sync badge: 10×10px `<img>` at bottom-right.
- Reactivity: subscribes to `$iconCacheVersion` store — re-renders when the icon cache is populated.

## `SortableHeader.svelte`

Props: `column`, `label`, `currentSortColumn`, `currentSortOrder`, `onClick`, `align?` (`'left'` default, `'right'` for
numeric columns).

Renders a `<button>` with a sort-direction triangle (▲/▼). Triangle is hidden (opacity 0) when column is not active.
Handles both `onclick` and `onkeydown` (Enter/Space).

## Key decisions

**Decision**: Size displayed as raw byte count with colored digit triads, not as human-readable "1.23 MB" **Why**:
Human-readable values lose precision and make it impossible to compare similarly-sized files. Triads with tier-based CSS
coloring (bytes/KB/MB/GB/TB) give both precision and quick visual scanning. Human-readable is available as a tooltip.

**Decision**: Middle truncation in `file-info` mode uses a throwaway `<span>` + binary search, not CSS
`text-overflow: ellipsis` **Why**: CSS ellipsis truncates from the right, losing the file extension. Middle truncation
preserves both the start of the filename and the extension (e.g. `very-lon...me.txt`). Binary search against measured
pixel width handles variable-width fonts correctly.

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

**Gotcha**: `containerWidth` state exists only to trigger reactivity for `truncatedName` **Why**: `ResizeObserver`
callbacks run outside Svelte's reactive graph. Writing to a `$state` variable inside the observer callback bridges the
gap, causing `truncatedName` (which reads `containerWidth` via `void containerWidth`) to recompute when the container
resizes.

**Gotcha**: `sizeTierClasses` CSS rules must be defined in the consuming view, not in `selection-info-utils.ts` **Why**:
The utility file is pure TypeScript with no DOM or style dependencies. The CSS classes it references (`size-bytes`,
`size-kb`, etc.) are defined in the parent list view's stylesheet, keeping style ownership with the view layer.

**Gotcha**: Thin space (U+2009) is used between digit triads, not a regular space **Why**: Regular spaces are too wide
for numeric grouping and look jarring in a compact status bar. Thin space matches typographic convention for digit
grouping and renders consistently across platforms.

## Dependencies

- `../types` — `FileEntry`, `SortColumn`, `SortOrder`
- `../views/full-list-utils` — `measureDateColumnWidth`
- `../views/file-list-utils` — `getFallbackEmoji`
- `$lib/icon-cache` — `getCachedIcon`, `iconCacheVersion`
- `$lib/settings/reactive-settings.svelte` — `formatFileSize`, `formatDateTime`
- `$lib/indexing/index-state.svelte` — `isScanning`
