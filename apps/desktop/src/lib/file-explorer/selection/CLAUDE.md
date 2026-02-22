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
- `formatHumanReadable(bytes)` — e.g. `"1.23 MB"`, used for tooltips.
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

## Dependencies

- `../types` — `FileEntry`, `SortColumn`, `SortOrder`
- `../views/full-list-utils` — `measureDateColumnWidth`
- `../views/file-list-utils` — `getFallbackEmoji`
- `$lib/icon-cache` — `getCachedIcon`, `iconCacheVersion`
- `$lib/settings/reactive-settings.svelte` — `formatFileSize`, `formatDateTime`
- `$lib/indexing/index-state.svelte` — `isScanning`
