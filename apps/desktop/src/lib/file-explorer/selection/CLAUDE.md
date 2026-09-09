# Selection display components

Renders selection state and file metadata in the status bar and list headers. Does NOT manage the selection set (that
lives in `FilePane.svelte` as a `Set<number>`).

## Key files

- **`selection-info-utils.ts`**: pure utilities (size/date formatting, display deciders), no DOM deps, fully tested.
- **`context-menu-target.ts`**: the words and digits the NATIVE context menu's header line shows.
- **`SelectionInfo.svelte`**: status bar below each pane. Four display modes derived from props.
- **`FileIcon.svelte`**: 16x16 icon with overlay badges (fallback below).
- **`SortableHeader.svelte`**: clickable column header with sort-direction triangle and shortcut tooltip.
- **`TagDots.svelte`** + **`tag-dots-utils.ts`**: Finder-tag dot cluster (Name cell, right edge); logic in the util.

## Must-knows

- **`sizeTierClasses` CSS rules live in the consuming view**, ❌ not in `selection-info-utils.ts`: the util is pure
  TypeScript, and `size-bytes` … `size-tb` belong to the list view's stylesheet.
- **Size-tier color tracks the underlying byte magnitude in every mode, not the displayed unit.** A 349-byte file shown
  as `"0.00 MB"` (forced-MB) still tiers as `size-bytes` (green), via `dynamicTierIndex(bytes, format)`. Don't tier off
  the displayed unit.
- **Age-tier mapping and the `appearance.dateColors` palette live with the setting**, in
  `$lib/settings/age-tier-utils.ts`, ❌ not here; the renderer is `$lib/ui/DateLabel.svelte`. See
  `$lib/settings/CLAUDE.md` § "Date display".
- **`isBrokenSymlink` checks `iconId === 'symlink-broken'`, NOT filesystem flags.** The backend already resolves broken-
  symlink status when computing the icon ID; re-checking via stat would be redundant and possibly stale.
- **❌ Never infer "no access" from entry metadata** (`permissions === 0`, missing `size`): every non-local backend
  leaves `permissions` at its `0` default, so the guess fires on EVERY SMB / archive / MTP folder. `DETAILS.md`.
- **The native context menu's header gets ALL its text from HERE, pre-rendered** (`contextMenuCountText`,
  `contextMenuSizeText`): Rust picks the shape, but every number in it needs a locale's separator, plural, and size
  unit, which `menu_t` hasn't got. ❌ `undefined` over a stand-in size (a folder, an unknown size, a selection holding a
  folder). `DETAILS.md`.
- **`SelectionInfo` derives its display mode from props** (`viewMode`, `selectedCount`, `stats`), never an explicit
  `mode` prop. The four modes: `empty`, `selection-summary`, `no-selection` (Full, no selection), `file-info` (Brief, no
  selection).
- **Middle truncation in `file-info` mode uses the `useShortenMiddle` action** (`$lib/utils/`) with `preferBreakAt: '.'`
  and `startRatio: 0.7`, NOT CSS `text-overflow: ellipsis`: CSS truncates from the right and loses the file extension.
- **Counts, size decimals, and triad separators follow the active locale via `$lib/intl`**; ❌ never a hardcoded locale
  or separator. Keep an ASCII space between value and unit: `colorizeSizeString` parses on the last one.
- **`SortableHeader`'s shortcut shows only when `isFocused` is true** (the `sort.by*` commands act on the focused pane).
  Hovering the unfocused pane's header shows the command name only; clicking still sorts that pane. Pinned by
  `SortableHeader.svelte.test.ts`.
- **`FileIcon` resolves a folder by PATH before `iconId`** (`getCachedCustomFolderIcon`): a custom-icon folder keeps the
  generic `dir` id, so gold recolor must skip it (`src-tauri/src/icons/DETAILS.md`). A cache miss draws the bundled
  macOS default, ❌ not an emoji, then swaps to the live icon on `$iconCacheVersion`.

## Status-bar hints (`SelectionInfo`)

- **Phone-storage hint (MTP)** tooltips the free-space readout on `caps.kind === 'mtp'` volumes (`mtpSpaceHint` from
  `FilePane`), explaining the folders-vs-used-space gap.
- **Stale (hourglass) indicator** flags possibly-incomplete DIRECTORY sizes (file sizes are always accurate):
  `selection-summary` keys on `isVolumeScanning(volumeId)` plus selected dirs, `file-info` on the shared
  `getDirSizeDisplayState(...)`, so Brief matches Full's size column.
- **Symlink hint (info glyph)** sits by a directory's size in `file-info` mode when `entry.recursiveHasSymlinks`,
  explaining a symlink-heavy folder's `0 bytes`: Cmdr matches `du`/Finder and doesn't double-count. Set by indexing
  (`recursive_has_symlinks`), surfaced through enrichment.

Full details (the full `selection-info-utils` export catalog, per-mode conditions, `recursiveSizePending` overlay flow,
size-unit decision rationale, and component props): `DETAILS.md`.
