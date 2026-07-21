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

## `TagDots.svelte` + `tag-dots-utils.ts`

The colored Finder-tag cluster shown at the right edge of the Name cell in both views (and gated by the
`listing.showTags` setting). `TagDots.svelte` is pure presentational; all logic is in `tag-dots-utils.ts`:

- `tagDotsModel(tags)` → `{ dots, overflowCount, label }`. Drops colourless tags (color 0) from the dots but keeps every
  tag name in `label` (the cluster's `aria-label` / `title`). Up to three colored tags show that many dots; beyond that,
  two dots plus a `+N` chip (`N = colored − 2`).
- `tagColorVar(color)` maps index 1-7 to the `var(--color-tag-*)` token (`app.css`, light + dark); each dot draws a 1px
  `--color-tag-border` ring so a pale fill (yellow) reads on white and the cluster survives the selection highlight.
- `tagClusterWidthPx(coloredCount)` is the reserved pixel width (gap + overlapping dot slots + optional chip), a pure
  function of the colored count. Brief mode is width-constrained, so `brief_columns.rs::tag_cluster_width` mirrors these
  constants to reserve room per row; Full mode needs no width math (Name is `1fr`). **Keep the two in sync.** The
  geometry constants (`TAG_DOT_SIZE`, `TAG_DOT_OVERLAP_OFFSET`, `TAG_CHIP_EXTRA`, `TAG_CLUSTER_GAP`) are exported and
  duplicated in the CSS literally; the dots overlap via negative margin, leftmost on top via descending inline z-index.

The dots are decorative (`aria-hidden`, `pointer-events: none`); the cluster carries the accessible label. Data flow and
the enrich/sweep wiring live in [`../views/DETAILS.md`](../views/DETAILS.md) (the Finder-tags decision).

## `SelectionInfo.svelte` display modes

Four `$derived displayMode` values, each with its condition:

- `empty`: `stats.totalFiles === 0 && stats.totalDirs === 0`.
- `selection-summary`: `selectedCount > 0` (any view mode). Directory recursive sizes are included when available (from
  the drive index). The `hasOnlyDirs` branch shows size triads when `totalSize > 0`; when sizes are unavailable
  (indexing off), it falls back to dir count and percentage.
- `no-selection`: Full mode, no selection: total file/dir counts.
- `file-info`: Brief mode, no selection: name, size triads, date.

Stale-size hourglass detail: in `file-info` mode the shared
`getDirSizeDisplayState(recursiveSize, complete, stale, updating)` drives it, where `complete`/`stale` come from the
entry's `recursiveSizeComplete`/`recursiveSizeStale` and
`updating = isVolumeScanning(volumeId) || isVolumeAggregating(volumeId) || recursiveSizePending`, scoped to the pane's
own `volumeId` (a scan on another drive must not flag this pane's folders). An unindexed or never-listed dir shows the
unknown state; a partially-scanned one is a lower bound (`≥`), and the `updating` flag adds the "Size not ready yet"
hourglass on top. The per-folder `recursiveSizePending` flag lives only on `DirStats` (not `get_file_range`), so
`FilePane.fetchEntryUnderCursor` overlays it onto the cursor entry via `updateIndexSizesInPlace([entry])` (skipping
`..`, whose entry path is the parent folder) and re-runs on `index-dir-updated` so the hourglass tracks a storm live.

Other layout: filename truncation uses `useShortenMiddle` with `preferBreakAt: '.'`. Date column width is computed via
`measureDateColumnWidth(formatDateTime)` to stay in sync with FullList; `formatDateTime` comes from
`reactive-settings.svelte`.

## Image-search readout (`FolderIndexStatus`)

What image search covers in the pane's current folder, rendered by `SelectionInfo` in all four display modes. The
component lives with its feature (`$lib/media-index/FolderIndexStatus.svelte`); the decision is the pure
`folder-index-state.ts::deriveFolderIndexState`.

- **Placement**: it's the last child of `.selection-info`, but CSS `order` puts it between the mode-specific content
  (order 0) and the free-space text (order 2). That's what keeps one instance serving four modes whose markup differs;
  rendering it inside each mode branch would be four copies to keep in step.
- **States and where each comes from**: `excluded` (an `mediaIndex.excludedFolders` entry at or above the folder — the
  hard veto, checked first because the backend gives it precedence too), `indexing` (covered AND this volume has live
  non-paused `media-enrich-progress` activity), `indexed` (a `mediaIndex.alwaysIndexFolders` entry at or above the
  folder), `automatic` (`ByImportance` scope, no explicit entry), `notIndexed` (`ChosenFolders` scope, no entry), and
  `off` (master toggle off, or no OS path), which renders nothing.
- **Decision: coverage, never completion.** There's no cheap per-folder count (`media_index/DETAILS.md` § What's left
  for later), so nothing here claims images ARE indexed, only that the settings cover them; the tooltips carry the
  caveat, and the progress percentage in the `indexing` tooltip is explicitly voiced as the whole drive's.
- **Decision: `automatic` is a state, not a guess.** In the `ByImportance` scope a folder's importance score is a
  backend fact with no FE query, so the readout says the choice is automatic rather than picking yes or no. Inventing a
  per-folder importance IPC just to color this label wasn't worth a new query on the pane's hot path.
- **Decision: local panes only.** `FilePane` passes an empty `currentPath` unless `caps.kind === 'local'`. The folder
  lists match absolute OS paths, and an archive / MTP / virtual pane's path isn't one, so it would miss every list and
  read as "not indexed". A mounted SMB share loses the readout too (the `smb` kind also covers direct-SMB volumes, whose
  paths aren't OS paths) — an honest omission, and closable when the two are distinguishable.
- **Informational, not actionable**: no button, matching the deliberate call not to make the bulk-rename bail
  actionable. The add/remove and exclude actions live in the folder's right-click menu.

## Phone-storage hint (MTP)

On a phone reached over USB (MTP), the disk-space readout reports the whole device userdata partition, but Cmdr can only
browse the shared-storage subtree; apps and system data make up the rest and aren't reachable over MTP. So the visible
folders add up to far less than the space reported as used, which reads as a Cmdr bug. A hover hint closes that gap.

- The copy lives in `fileExplorer.navigation.spaceMtpHint`. `FilePane` resolves it to the `mtpSpaceHint` prop only when
  `caps.kind === 'mtp'` (the A6-correct discriminant, not a volume-id string); it's `undefined` for every other kind.
- Both surfaces that show the figure carry it: `SelectionInfo` tooltips the visible free-space text (the number users
  read), and `FilePane` passes the same string as the third arg of `formatBarTooltip` for the disk-usage bar, appended
  after the size/level sentences. One catalog key, so the two never drift.
- It rides on `use:tooltip`, so it's hover-only: a touch/keyboard user reading the footer text never sees it. Making the
  footer text itself focusable is the honest a11y fix if that matters later.

## `FileIcon.svelte`

Props: `file: FileEntry`, `syncIcon?: string` (URL for sync overlay badge).

- Primary: `<img>` from `getCachedIcon(file.iconId)`.
- Fallback (cache miss only): bundled macOS default folder/file icon (`static/icons/default-folder.png` /
  `default-file.png`, extracted from the system `GenericFolderIcon`/`GenericDocumentIcon`), chosen by `isFolderIcon`.
- Symlink badge: the `link` glyph via `<Icon>` (size 10, `--color-accent-pop`, a mode-aware higher-contrast accent),
  bottom-right by default, moving to top-left when `syncIcon` is present.
- Sync badge: 10×10px `<img>` at bottom-right.
- Reactivity: subscribes to `$iconCacheVersion`, re-renders when the cache is populated.

### Cmdr-gold folder recolor scope

When the app color is "Cmdr gold" (`getIsCmdrGold()`), folder icons get the `.gold-folder` CSS filter
(`grayscale(1) sepia(1) hue-rotate(3deg) saturate(2.5) brightness(0.95)`). Because it starts with `grayscale(1)`, the
folder's baked-in tint is discarded first, so a folder macOS rendered in any system accent re-tints to the same gold.

`isFolderIcon` gates which ids get it, and the scope is a deliberate contract (pinned by
`FileIcon.gold-recolor.test.ts`):

- **Included**: `dir` / `symlink-dir` (the generic folder) and `special:*` (the standard folders macOS badges with a
  white glyph: Downloads, Desktop, Documents, Movies, Music, Pictures, Public, Trash, home). Without `special:*`, those
  keep the raw OS bitmap, whose folder tint is the macOS _system_ accent, so they'd leak through non-gold while every
  generic folder is gold. (The glyph goes gold-on-gold rather than white-on-color; accepted as consistent-over-crisp.)
- **Excluded, on purpose**: `pkg:*` (full-color `.app`/bundle icons the grayscale+sepia filter would flatten into a gold
  blob) and `path:*` (folders with a user-assigned Finder custom icon we must not override). ❌ Don't widen the gate to
  either; a "make all folders gold" refactor is exactly the regression the test guards.

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
- `$lib/indexing/index-state.svelte`: `isVolumeScanning`, `isVolumeAggregating` (keyed on the pane's `volumeId`).
