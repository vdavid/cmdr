# Disk space display plan

Show free disk space for the active volume on each pane — always visible, zero extra layout cost. Three surfaces, progressive disclosure: a usage bar you glance at, status text you read, and volume selector detail you compare.

## Design

### Surface 1: Header usage bar (always visible, per-pane)

Each pane's vertical stack is: tab bar → breadcrumb/path header → column headers → file listing → status bar. The breadcrumb header (showing "Macintosh HD • Applications") already has a `border-bottom: 1px solid var(--color-border-strong)` separating it from the content below. Replace it with a 2px bar where the filled portion represents used space. This places the bar directly below the volume name in the breadcrumb — a natural visual association.

- Full pane width. Left portion = used %, right portion = normal border color.
- Takes zero additional vertical space (1px → 2px is imperceptible in layout).
- Always visible on both panes — instant left vs right comparison.
- Color encodes severity using three levels, derived from existing semantic colors:
  - < 80% used: `--color-allow` at ~50% opacity. Barely noticeable, just "info."
  - 80–95%: `--color-warning` at ~60% opacity. Amber nudge.
  - \> 95%: `--color-error` at full opacity. Red alert.
- Hover tooltip: full text like `420.50 GB of 1,000.21 GB (42%) free`.
- **Null/unavailable state:** When `volumeSpace` is null (not yet fetched, or unavailable for the volume type — cloud drives, network mounts, future S3 buckets), the bar renders as a plain 2px line of `--color-border-strong` with no fill. Visually identical to a regular separator — degrades gracefully.

Why a border and not a new component: it's the cheapest possible integration. No flexbox changes, no height shift, no layout reflow. The breadcrumb border already separates header from content — now it also communicates.

### Surface 2: Status bar text (per-pane)

SelectionInfo shows disk space text right-aligned in **all display modes except `selection-summary`** (when files are selected, selection info takes priority):

**Full mode, no selection (`no-selection`):**
```
No selection, 101 files and 15 dirs.          420 GB of 1 TB free (42%)
```

**Brief mode, no selection (`file-info`):**
```
TemporaryDirectory.1Vt435    0  2026-02-25 09:48    420 GB of 1 TB free (42%)
```
The filename already middle-truncates via ResizeObserver to accommodate size and date columns. Adding the disk space element (flex-shrink: 0) simply reduces available name width — the existing truncation algorithm handles this naturally.

**Empty directory (`empty`):**
```
Nothing in here.                              420 GB of 1 TB free (42%)
```

- Zero additional space — fills the empty right portion of an existing bar.
- When files are selected, disk space text hides (selection info is more immediately useful).

### Surface 3: Volume selector dropdown (on demand)

Under each volume name in the dropdown, add a thin usage bar + free space text:

```
 ✓ 🖥 Macintosh HD
     ████████░░░░  420 GB free of 1 TB

   💾 naspi
     ██████████░░  120 GB free of 2 TB
```

- Mini bar: 2px tall, same color logic as the header bar, rounded to `--radius-sm`.
- Text: rounded values (no decimals) since this is for quick comparison, not precision.
- Only shown for volumes with the `main_volume` or `attached_volume` category — cloud drives, favorites, and network mounts don't have meaningful local disk space.
- Fetched lazily when dropdown opens (not on mount) to avoid unnecessary IPC calls for volumes the user may never look at.

### Text format

All size formatting uses `formatFileSize` from `reactive-settings.svelte.ts`, which respects the user's binary (KB) vs SI (kB) setting. See step 2 for details.

- **Status bar:** `420 GB of 1 TB free (42%)` — rounded to save width; full precision is available in the tooltip.
- **Volume selector:** `420 GB free of 1 TB` — rounded, comparison-friendly.
- **Tooltip (header bar):** `420.50 GB of 1,000.21 GB free (42%)` — full precision on hover for anyone who needs exact numbers.

### What not to do

- No animation on the usage bar. It's a static indicator, not a progress bar.
- No separate disk space component or row — the three surfaces above cover all use cases.
- No periodic polling. Refresh on volume change and directory navigation only (see data flow below).

## Data flow

### Fetching space info

`getVolumeSpace(path)` already exists in `storage.ts` and returns `{ totalBytes, availableBytes }`. It calls the Rust backend's `get_volume_space` command. Returns null on non-macOS (safe fallback).

### Where state lives

Disk space state lives in **FilePane** as a reactive `$state`:

```typescript
let volumeSpace: VolumeSpaceInfo | null = $state(null)
```

FilePane already knows its `volumeId` and `volumePath`. It fetches space info:
1. On mount (alongside the initial directory load).
2. When the volume changes (the `onVolumeChange` callback already fires).
3. When switching tabs — tab switches trigger path/volume changes, so `refreshVolumeSpace()` fires naturally alongside the directory load. No per-tab caching needed; the `statvfs` call is <1ms so any flash of stale data is imperceptible.
4. After a file operation completes (copy, move, delete) — piggybacked on the existing `refreshView()` flow.

This is cheap (single `statvfs` syscall on the backend, <1ms) and keeps data fresh without polling.

### Passing to children

- **SelectionInfo** gets a new optional prop: `volumeSpace: VolumeSpaceInfo | null`.
- **VolumeBreadcrumb** fetches space info independently when the dropdown opens (per-volume, on demand).
- The header usage bar renders inside FilePane's template directly (it's just a styled div replacing the border).

### Refresh on file operations

`DualPaneExplorer.svelte` (or its extracted transfer module — search for `refreshPanesAfterTransfer`) has a `refreshPanesAfterTransfer()` function that calls `paneRef?.refreshView?.()` after copy/move/delete operations complete (or are cancelled/errored). `refreshView()` in FilePane increments a `cacheGeneration` counter to force list re-fetch.

To refresh disk space: FilePane should expose a `refreshVolumeSpace()` method. `refreshPanesAfterTransfer()` calls it alongside `refreshView()`. **Both panes should always refresh** after any transfer — even a copy only writes to the destination, the source pane might be on the same underlying volume. Detect same-volume by comparing `volumeId` from each pane ref, but in practice it's simplest to always refresh both since the call is <1ms.

## Implementation

### Step 1: CSS variables for usage bar

In `app.css`, add disk usage color variables for both light and dark modes. Three levels, derived from existing semantic colors via `color-mix`:

```css
/* Disk usage bar — derived from existing --color-allow, --color-warning, --color-error */
--color-disk-ok: color-mix(in srgb, var(--color-allow) 50%, transparent);
--color-disk-warning: color-mix(in srgb, var(--color-warning) 60%, transparent);
--color-disk-danger: var(--color-error);
--color-disk-track: var(--color-border-strong);
```

The `color-mix` with transparent gives us the low-opacity versions without needing separate opacity layers. Use the same variable names in dark mode — dark mode's `--color-allow` and `--color-warning` are already adjusted for dark backgrounds, so the mixes should work as-is. Verify visually during step 9.

### Step 2: Disk space utility module

New file: `apps/desktop/src/lib/file-explorer/disk-space-utils.ts`

Pure functions, no state, no DOM. Size formatting is delegated to a caller-provided `formatSize` function so these utils stay pure and testable while respecting the user's binary/SI setting via `formatFileSize` from `reactive-settings.svelte.ts`.

```typescript
import type { VolumeSpaceInfo } from '$lib/tauri-commands/storage'

type FormatSize = (bytes: number) => string

export interface DiskUsageLevel { cssVar: string; label: string }

/** Returns the CSS variable name for the usage bar color based on percentage used. */
export function getDiskUsageLevel(usedPercent: number): DiskUsageLevel { ... }

/** Returns used percentage (0–100), clamped. */
export function getUsedPercent(space: VolumeSpaceInfo): number { ... }

/** Formats the status bar text: "420 GB of 1 TB free (42%)" — rounded to save width. */
export function formatDiskSpaceStatus(space: VolumeSpaceInfo, formatSize: FormatSize): string { ... }

/** Formats the short volume selector text: "420 GB free of 1 TB". */
export function formatDiskSpaceShort(space: VolumeSpaceInfo, formatSize: FormatSize): string { ... }

/** Formats the tooltip text: "420.50 GB of 1,000.21 GB free (42%)". */
export function formatDiskSpaceTooltip(space: VolumeSpaceInfo, formatSize: FormatSize): string { ... }
```

No separate `formatDiskSpace(bytes)` function — we reuse the existing `formatFileSize` (which calls `formatFileSizeWithFormat` from `format-utils.ts` with the user's current binary/SI preference).

**Cleanup:** `formatHumanReadable` in `selection-info-utils.ts` is currently unused (only referenced in tests). It duplicates `formatFileSizeWithFormat` from `format-utils.ts` but doesn't respect the user's kB/KB setting. Remove it and update its tests to use `formatFileSizeWithFormat` instead.

Unit test the new module thoroughly (pure functions = easy to test). Edge cases: zero total bytes, 0 available, totalBytes === availableBytes, very small volumes.

### Step 3: Header usage bar in FilePane

In `FilePane.svelte`:

1. Add `volumeSpace` state and fetch logic:
   ```typescript
   let volumeSpace: VolumeSpaceInfo | null = $state(null)

   async function refreshVolumeSpace(): Promise<void> {
       volumeSpace = await getVolumeSpace(currentPath)
   }
   ```

2. Call `refreshVolumeSpace()` on mount, on volume change, and after directory listing refreshes.

3. Export `refreshVolumeSpace` so DualPaneExplorer can call it after file operations.

4. Replace the breadcrumb/path header's `border-bottom` (below the tab bar, above column headers) with a usage bar div:
   ```svelte
   <div class="header">
       <!-- existing breadcrumb + path -->
   </div>
   <div
       class="disk-usage-bar"
       title={volumeSpace ? formatDiskSpaceTooltip(volumeSpace, formatFileSize) : ''}
       role="meter"
       aria-label="Disk usage"
       aria-valuenow={volumeSpace ? getUsedPercent(volumeSpace) : 0}
       aria-valuemin={0}
       aria-valuemax={100}
   >
       {#if volumeSpace}
           <div
               class="disk-usage-fill"
               style:width="{getUsedPercent(volumeSpace)}%"
               style:background-color="var({getDiskUsageLevel(getUsedPercent(volumeSpace)).cssVar})"
           ></div>
       {/if}
   </div>
   ```

5. CSS:
   ```css
   .disk-usage-bar {
       height: 2px;
       background-color: var(--color-disk-track);
       flex-shrink: 0;
   }

   .disk-usage-fill {
       height: 100%;
       transition: none; /* no animation — it's a static indicator */
   }
   ```

6. Remove `border-bottom` from the breadcrumb/path header class (the usage bar replaces it as the visual separator between breadcrumb and content).

### Step 4: Status bar text in SelectionInfo

1. Add `volumeSpace` prop to SelectionInfo:
   ```typescript
   interface Props {
       // ... existing props
       volumeSpace?: VolumeSpaceInfo | null
   }
   ```

2. Show disk space text in `empty`, `no-selection`, **and** `file-info` display modes — all except `selection-summary`:
   ```svelte
   {#if displayMode === 'empty'}
       <span class="summary-text">Nothing in here.</span>
       {#if volumeSpace}
           <span class="disk-space-text">{formatDiskSpaceStatus(volumeSpace, formatFileSize)}</span>
       {/if}
   {:else if displayMode === 'file-info' && entry}
       <!-- Brief mode without selection: show file info -->
       <span class="name" bind:this={nameElement} title={displayName}>{truncatedName}</span>
       <span class="size" title={sizeTooltip}>...</span>
       <span class="date" ...>{dateDisplay}</span>
       {#if volumeSpace}
           <span class="disk-space-text">{formatDiskSpaceStatus(volumeSpace, formatFileSize)}</span>
       {/if}
   {:else if displayMode === 'no-selection'}
       <span class="summary-text">{noSelectionText}</span>
       {#if volumeSpace}
           <span class="disk-space-text">{formatDiskSpaceStatus(volumeSpace, formatFileSize)}</span>
       {/if}
   {:else if displayMode === 'selection-summary' && stats}
       <!-- selection summary — no disk space text, selection takes priority -->
       ...
   {/if}
   ```

3. CSS:
   ```css
   .disk-space-text {
       flex-shrink: 0;
       margin-left: auto;
       padding-left: var(--spacing-md);
       color: var(--color-text-tertiary);
       white-space: nowrap;
   }
   ```

4. The text uses `--color-text-tertiary` so it's visible but clearly secondary to the file/dir info on the left.

5. Pass `volumeSpace` from FilePane to SelectionInfo alongside existing props.

6. In brief mode's `file-info` display, the disk-space-text element is flex-shrink: 0 alongside size and date. The name element (flex: 1) absorbs the squeeze — but the middle-truncation algorithm needs updating. It currently measures `.size` and `.date` element widths to compute available space for the name (`availableWidth = containerWidth - sizeWidth - dateWidth - 24`). Add a `.disk-space-text` measurement to this calculation so the ellipsis lands at the correct position instead of the name being hard-clipped by `overflow: hidden`.

### Step 5: Volume selector disk space

In `VolumeBreadcrumb.svelte`:

1. Add a `SvelteMap` state (from `svelte/reactivity`) to cache fetched space info. Plain `Map` mutations don't trigger Svelte 5 reactivity — `SvelteMap` does:
   ```typescript
   import { SvelteMap } from 'svelte/reactivity'

   let volumeSpaceMap = new SvelteMap<string, VolumeSpaceInfo>()
   ```

2. When the dropdown opens, fetch space for all `main_volume` and `attached_volume` entries that aren't already cached:
   ```typescript
   async function fetchVolumeSpaces(volumes: VolumeInfo[]): Promise<void> {
       const physicalVolumes = volumes.filter(
           (v) => v.category === 'main_volume' || v.category === 'attached_volume'
       )
       await Promise.all(
           physicalVolumes
               .filter((v) => !volumeSpaceMap.has(v.id))
               .map(async (v) => {
                   const space = await getVolumeSpace(v.path)
                   if (space) volumeSpaceMap.set(v.id, space)
               })
       )
   }
   ```

3. Call `fetchVolumeSpaces()` in the existing `openDropdown()` function. Clear the map on `volume-mounted`/`volume-unmounted` events so remounted volumes get fresh data.

4. Render under each volume item (only for physical volumes with space data):
   ```svelte
   {#if volumeSpaceMap.has(volume.id)}
       {@const space = volumeSpaceMap.get(volume.id)}
       <div class="volume-space-info">
           <div class="volume-space-bar">
               <div
                   class="volume-space-fill"
                   style:width="{getUsedPercent(space)}%"
                   style:background-color="var({getDiskUsageLevel(getUsedPercent(space)).cssVar})"
               ></div>
           </div>
           <span class="volume-space-text">{formatDiskSpaceShort(space, formatFileSize)}</span>
       </div>
   {/if}
   ```

5. CSS for the mini bar and text:
   ```css
   .volume-space-info {
       display: flex;
       align-items: center;
       gap: var(--spacing-sm);
       padding: 0 var(--spacing-md) var(--spacing-xs) calc(14px + var(--spacing-sm) + 16px + var(--spacing-sm));
       /* Left padding aligns with volume label text (checkmark + gap + icon + gap) */
   }

   .volume-space-bar {
       flex: 1;
       height: 2px;
       background-color: var(--color-disk-track);
       border-radius: var(--radius-sm);
   }

   .volume-space-fill {
       height: 100%;
       border-radius: var(--radius-sm);
   }

   .volume-space-text {
       font-size: var(--font-size-xs);
       color: var(--color-text-tertiary);
       white-space: nowrap;
       flex-shrink: 0;
   }
   ```

### Step 6: Refresh after file operations

In `DualPaneExplorer.svelte` (note: this file is being refactored — the logic may have moved to an extracted module by implementation time; search for `refreshPanesAfterTransfer` to find the current location):

1. The existing `refreshPanesAfterTransfer()` function calls `paneRef?.refreshView?.()` on destination (and source for moves). Add a companion `paneRef?.refreshVolumeSpace?.()` call for **both panes** in this function — always refreshing both is simplest since the call is <1ms, and handles the same-volume case (left and right pane on the same disk) without needing to compare `volumeId`.

2. `refreshPanesAfterTransfer()` is already called from `handleTransferComplete`, `handleTransferCancelled`, and `handleTransferError`. No new call sites needed — just extend the function.

### Step 7: Accessibility

- The header usage bar uses `role="meter"` with `aria-label="Disk usage"`, `aria-valuenow` (used percentage, matching the visual fill), `aria-valuemin={0}`, `aria-valuemax={100}`.
- The status bar text is plain text in the DOM — screen readers pick it up naturally.
- The volume selector space info is inside the dropdown's accessible tree.
- Color is never the sole indicator — the percentage text and tooltip provide the same info without color.

### Step 8: Tests

**Unit tests** (`disk-space-utils.test.ts`):
- `getDiskUsageLevel` returns correct level for 0%, 50%, 75%, 90%, 99% used.
- `getUsedPercent` computes correctly, handles edge cases.
- `formatDiskSpaceStatus` produces the expected format string using a mock `formatSize`.
- `formatDiskSpaceShort` rounds correctly.
- `formatDiskSpaceTooltip` includes full precision.
- Edge cases: 0 total bytes, 0 available, totalBytes === availableBytes.

**Cleanup test**: Verify `formatHumanReadable` removal from `selection-info-utils.ts` doesn't break anything — update or remove its tests.

**Component tests** (if feasible with existing test setup):
- SelectionInfo renders disk space text when `volumeSpace` is provided and no selection (both full and brief modes).
- SelectionInfo hides disk space text when files are selected.
- SelectionInfo shows disk space text in empty directory state.

### Step 9: Visual polish with MCP

After the implementation compiles and runs:

1. Use MCP to take a screenshot and verify the header usage bar is visible and correctly proportioned.
2. Check both light and dark modes — the `color-mix` variables need visual verification.
3. Verify the status bar layout doesn't overflow on narrow windows — if it does, truncate or hide the disk space text at small widths (CSS `min-width: 0` + overflow hidden on the left summary, `flex-shrink: 0` on the right disk text, with a reasonable `max-width` or media query fallback).
4. Verify brief mode specifically — does the filename truncation handle the extra disk space element gracefully? Test with long filenames.
5. Open the volume selector and verify the mini bars align with volume label text.
6. Test with a nearly-full volume (if available) to see the red/danger color in action.
7. Test with a volume that has no space info (cloud drive, network) — should show nothing, no errors. The header bar should look like a regular separator.
8. Copy a file between panes and verify both panes' space info updates.

Iterate on:
- Bar color opacity — may need to be more or less prominent than the initial `color-mix` values.
- Status bar text color — `--color-text-tertiary` might be too faint or too prominent.
- Volume selector layout — spacing between the volume name and the space info row.
- Tooltip formatting — verify comma grouping looks right.

### Step 10: Run checks

- `./scripts/check.sh --svelte` — covers eslint, prettier, svelte-check, stylelint, tests.
- `./scripts/check.sh --check knip` — verify no dead exports from the new utility module and that removing `formatHumanReadable` doesn't leave dangling imports.
- Add `disk-space-utils.ts` to test coverage expectations (it's pure functions, should be easy to hit 70%+).

## Files touched

| File | Change |
|------|--------|
| `app.css` | Add `--color-disk-ok`, `--color-disk-warning`, `--color-disk-danger`, `--color-disk-track` variables (light + dark) |
| `disk-space-utils.ts` | New: pure formatting + threshold functions, accepts `formatSize` parameter |
| `disk-space-utils.test.ts` | New: unit tests for the utility module |
| `selection-info-utils.ts` | Remove unused `formatHumanReadable` |
| `selection-info-utils.test.ts` | Remove/update `formatHumanReadable` tests |
| `FilePane.svelte` | Add `volumeSpace` state, fetch logic, export `refreshVolumeSpace`, render usage bar below breadcrumb (above column headers), pass to SelectionInfo. Note: tab bar sits above the breadcrumb — verify current template structure before editing. |
| `SelectionInfo.svelte` | Add `volumeSpace` prop, render disk space text in `empty`, `file-info`, and `no-selection` modes |
| `VolumeBreadcrumb.svelte` | Add `SvelteMap` space cache, fetch on dropdown open, render mini bars + text |
| `DualPaneExplorer.svelte` (or extracted module) | Call `refreshVolumeSpace()` on both panes wherever `refreshPanesAfterTransfer()` lives |
