# Indexing (frontend) details

Depth for the frontend indexing bridge. `CLAUDE.md` holds the must-knows; this file holds the full API, event table,
tooltip content, ETA mechanics, and tests.

## State model: per-volume map + a global aggregation signal

`index-state.svelte.ts` holds a `SvelteMap<volumeId, VolumeIndexActivity>` (`activity`): one entry per volume that's
actively scanning or replaying, removed the moment that volume finishes. Each `VolumeIndexActivity` carries a `phase`
(`'scanning' | 'replaying'`), the scan fields (`entriesScanned`, `dirsFound`, `bytesScanned`, `scanStartedAt`,
`priorTotalEntries`, `priorScanDurationMs`, `volumeUsedBytes`), and the replay fields (`replayEventsProcessed`,
`replayEstimatedTotal`, `replayStartedAt`). Scan and replay events each carry their own `volumeId`, so they key the map
directly — that's what makes the indicator track local + SMB + MTP, not just root.

Aggregation carries its own `volumeId` too (see below), so it lives in a second per-volume map keyed by `volumeId`, each
entry an `AggregationActivity` (`phase`, `current`, `total`, `startedAt`).

### Public API (`index.ts`)

The barrel exports only the lifecycle + the two cross-module reads (`isScanning`, `getEntriesScanned`, consumed by
SearchDialog), plus `initIndexState` / `destroyIndexState` / `initIndexEvents`. The indicator imports the rest directly
from `./index-state.svelte`:

```ts
// Multi-drive API (the indicator):
getActiveIndexVolumes(): VolumeIndexActivity[]   // every scanning/replaying volume
isAnyVolumeIndexing(): boolean                    // scan/replay map non-empty OR any volume aggregating (visibility gate)
getVolumeAggregation(volumeId): AggregationActivity | undefined  // that volume's live aggregation, or undefined
getAggregatingVolumeIds(): string[]               // every volume currently aggregating
isAggregating(): boolean                          // any volume aggregating
// Backward-compatible scalars (other consumers):
isScanning(): boolean                // any volume scanning (size-updating hourglass, search-unavailable)
getEntriesScanned(): number          // the ROOT volume's live count (SearchDialog index-build progress)
// Lifecycle:
initIndexState(): Promise<void>      // call once at app mount
destroyIndexState(): void            // call at app teardown
initIndexEvents(onDirUpdated: (paths: string[]) => void): Promise<UnlistenFn>
```

## Scan-state events (`index-state.svelte.ts`)

Eight Tauri events drive the state. All of them carry a `volumeId`: scan and replay key the live-`activity` map,
aggregation keys its own `aggregation` map.

- **`index-scan-started`** (`{ volumeId, priorTotalEntries, priorScanDurationMs, volumeUsedBytes }`): create/replace the
  volume's `activity` entry (`phase: 'scanning'`, `scanStartedAt = Date.now()`, stash the calibration).
- **`index-scan-progress`** (`{ volumeId, entriesScanned, dirsFound, bytesScanned }`): update that volume's counters
  (seeds a scanning entry if the started event was missed, e.g. mid-scan reload).
- **`index-scan-complete`** (`{ volumeId, totalEntries, totalDirs, durationMs }`): remove the volume's `activity` entry.
- **`index-rescan-notification`** (`{ volumeId, reason, details }`): show an info toast with a reason-specific message.
- **`index-replay-progress`** (`{ volumeId, eventsProcessed, estimatedTotal }`): create/replace the volume's `activity`
  entry as `phase: 'replaying'`, update counters.
- **`index-replay-complete`** (`{ volumeId, durationMs }`): remove the volume's replay entry.
- **`index-aggregation-progress`** (`{ volumeId, phase, current, total }`): upsert the volume's `aggregation` entry
  (phase/progress, plus a `startedAt` ETA clock reset on each phase change).
- **`index-aggregation-complete`** (`{ volumeId }`): remove that volume's `aggregation` entry.

### Aggregation is per-volume

`index-aggregation-progress` and `index-aggregation-complete` (`AggregationProgressEvent` /
`IndexAggregationCompleteEvent`, Rust `writer/mod.rs` + `events.rs`) both carry the `volumeId`. The writer is spawned
per volume, so the id is known at spawn time and threaded down to every emit site (the `saving_entries` phase in
`writer/entries.rs` and the compute/write phases via `writer/aggregation.rs::build_progress_callback`). The FE keeps a
separate `aggregation` map keyed by `volumeId`, so two drives aggregating at once each get their own progress — no
guessing from the last scan to complete. The indicator folds each volume's aggregation into that volume's row; a volume
aggregating with no live scan/replay entry (its scan already finished) gets a synthetic aggregation-only row.

## Status indicator tooltip content

`IndexingStatusIndicator.svelte` is the thin shell: the hourglass icon (shown iff `isAnyVolumeIndexing()`) plus a
tooltip that renders one `IndexingDriveRow` per active drive (from `getActiveIndexVolumes()`, plus a synthetic row for
any aggregating volume with no live entry, from `getAggregatingVolumeIds()`). It resolves each `volumeId` to a display
name via the volume store's `getVolumes()` (falling back to the id). The per-drive heading (`indexing.drive.heading`, a
`{name}` passthrough) shows only when more than one drive is active, so the common single-drive case is as terse as
before. The component renders the content inside a `<div hidden>` host and passes the inner content div (not the hidden
host) to the tooltip action via `contentEl`, so the adopted element doesn't carry `hidden` into the tooltip.

Each `IndexingDriveRow` owns its own ETA sliding-window (so several drives indexing at once each get an independent rate
estimate) and renders one of three modes (priority aggregation > scan > replay), reading from its `VolumeIndexActivity`
plus its own `AggregationActivity | undefined` (this volume's aggregation, or `undefined` when it isn't aggregating):

- **Scan**: a two-tier label + counters, plus `ProgressBar` + percent + ETA when a denominator exists. Tier 1
  (calibrated): "Scanning your drive... 42,000 entries, 1,200 dirs" with "42%, 1m 20s left". Tier 2 (first scan, rough):
  "Scanning your drive (first scan)... ..." with "36%, roughly 19m left". `computeScanProgress` null → the counter-only
  label.
- **Aggregation**: phase label ("Saving entries...", "Loading directories...", "Sorting directories...", "Computing
  directory sizes...", "Saving directory sizes...") + `ProgressBar` + percent + ETA for the phases that have progress
  (`saving_entries`, `computing`, `writing`).
- **Replay**: "Updating index..." + "N events processed" + `ProgressBar` + blended ETA.

The percent and ETA render as one span joined through `indexing.progress.percentEta` (`"{percent}%, {eta}"`), so the
comma separator is translatable per locale (e.g. a full-width comma in `zh`, a space before `%` in `de`/`fr`); without
an ETA yet, just the bare percent shows. The label span has no `white-space: nowrap`: the scan counters grow without
bound, so the label wraps within the tooltip's `max-width` (on `.cmdr-tooltip`) instead of overflowing past the
right-anchored, viewport-clamped box and clipping off the window edge.

The hourglass is a ~14px `<Icon>` (the same icon as the size-column stale indicator), `position: absolute` top/right at
`var(--spacing-sm)`, tertiary text color, gentle opacity pulse gated behind `prefers-reduced-motion: reduce`.

## Two-tier scan progress (`computeScanProgress`)

- **Tier 1** (`priorTotalEntries` present): `entriesScanned / priorTotalEntries`, clamped to 0.99, apples-to-apples.
- **Tier 2** (`volumeUsedBytes` present): `bytesScanned / volumeUsedBytes`, clamped to 0.95 (APFS clones overshoot the
  statfs denominator), flagged `rough`.
- **Neither** → `null` (counter-only, no bar).

The ETA window samples the SAME counter the tier divides by (entries for tier 1, bytes for tier 2) — don't mix them.
`formatEta` carries a `Number.isFinite` guard so a dropped null gate can't surface "Infinitym left".

## ETA mechanics (`eta.ts`)

Pure helpers. Aggregation uses a single elapsed extrapolation. Scan and replay blend that 50-50 with a sliding-window
rate over the last ~5 seconds (early extrapolation alone is wildly wrong). The window-snapshot collection is the only
stateful glue and stays in each `IndexingDriveRow` (so per-drive rates don't collide); it feeds the pure
`pruneSnapshots` / `computeWindowEta` / `blendEtas` / `formatEta`. Tier 1's prior-duration seed
(`priorScanDurationMs − elapsed`, ms→seconds) covers the gap before the window has samples. Tier 2's ETA is prefixed
"roughly".

## Tests

- **`eta.test.ts`**: the pure ETA helpers (thresholds, elapsed + window estimation, blending, snapshot pruning), plus
  `computeScanProgress` (tier selection, both clamps, null/zero-denominator fallbacks) and the `formatEta` non-finite
  pin.
- **`IndexingStatusIndicator.a11y.test.ts`**: tier-3 axe checks for idle (renders nothing), single-drive scanning
  (counter-only and calibrated-with-bar), aggregating-with-progress, and multi-drive (a heading per drive). Mocks both
  `index-state.svelte` and the volume store's `getVolumes` (drive-name resolution).

The reactive event-driven glue in `index-state.svelte.ts` is allowlisted in `coverage-allowlist.json`. Manual end-to-end
testing runs the Rust indexer via `pnpm dev`.

## Dependencies

- `$lib/ipc/bindings`: `commands` (status query).
- `$lib/tauri-commands`: the `tauri-specta`-typed indexing event wrappers (`onIndexScan*`, `onIndexAggregation*`,
  `onIndexReplay*`, `onIndexRescanNotification`, `onIndexDirUpdated`) + `UnlistenFn`, in `tauri-commands/indexing.ts`.
- `$lib/ui/toast`: `addToast` (rescan notification toasts).
- `$lib/file-explorer/selection/selection-info-utils`: `formatNumber` (indicator only, `'en-US'` locale).
- `$lib/tooltip/tooltip`: `tooltip` action with the `contentEl` live-content param (indicator only).
- `$lib/ui/ProgressBar.svelte`: size `sm` (drive row).
- `$lib/stores/volume-store.svelte`: `getVolumes` (indicator resolves `volumeId` → display name).

## i18n

All user-facing copy here lives in `$lib/intl/messages/en/indexing.json` (prefix `indexing.*`), resolved via `tString()`
from `$lib/intl`; `cmdr/no-raw-user-facing-string` is enforced on `lib/indexing/`. Don't hardcode copy. The backend's
typed rescan-reason and aggregation-phase discriminators map to catalog KEYS (`rescanReasonToMessageKey` /
`phaseToLabelKey`), resolved at render time — branch on the typed enum, never on wording. The `'bytes'` / `'entries'`
scan-unit tags and `'scan'`/`'aggregation'`/`'replay'` mode strings are internal discriminators, not copy. Base-en
output is parity-pinned by `indexing-i18n-parity.test.ts`.

## Honest size rendering

The drive index serves directory sizes that are sometimes exact, sometimes a lower bound, sometimes unknown, and
sometimes accurate-but-stale. The backend collapses its epoch model into two booleans per `FileEntry` / `DirStats`
(`recursiveSizeComplete`, `recursiveSizeStale`); the FE renders from `{recursiveSize, complete, stale}` and never sees
raw epochs. The full data model is in the backend `indexing/DETAILS.md` § "Honest sizes".

**Content state — `getDirSizeDisplayState(recursiveSize, complete, stale, updating)`** (`views/full-list-utils.ts`), a
pure function and the single source of truth:

- `recursiveSize == null` → `'dir'` (the `<dir>` placeholder), or `'scanning'` when `updating`.
- `complete === false && size === 0` → unknown, which collapses into `'dir'`/`'scanning'` → the familiar `<dir>`
  placeholder (the same render as a not-yet-scanned dir), never a settled-looking value. The crux: distinct from a
  genuinely-empty `0 bytes`. (A size we don't yet know shows the placeholder it always showed, not a `—`.)
- `complete === false && size > 0` → `'lower-bound'` → `≥` (`LOWER_BOUND_GLYPH`) prefix + the formatted size.
- `complete === true && stale === true` → `'size-stale'` → the formatted size, muted (reduced opacity, matching the
  yellow=stale freshness badge; tunable).
- otherwise → `'size'` → the plain formatted size (incl. a genuinely-empty `0 bytes`).
- Absent `complete`/`stale` (a dir enriched before the flags, or a fixture) ⇒ treated as exact + fresh.

**The in-flux hourglass is ORTHOGONAL** — `isDirSizeUpdating(indexing, pending)` (`indexing || pending`), applied on top
of any content state. A dir can be both `'size-stale'` (freshness) and updating (in-flux). The `≥` is a symbol, not
translatable copy; the per-state explanation is a one-line label in `buildDirSizeTooltip` (keys
`fileExplorer.dirSize.{lowerBoundLine,unknownTooltip,staleLine}`). `unknownTooltip` is the tooltip for the unknown
(`<dir>`-placeholder) state — incomplete subtree, size 0.

**Three consumers, kept in lockstep** (or rendered text and pre-measured width drift): `FullList.svelte` (the Size
cell), `SelectionInfo.svelte` (Brief-mode status bar, so it matches Full), and `measure-column-widths.ts` (reserves the
`≥` glyph + `<dir>`-placeholder widths and the hourglass icon when `isDirSizeUpdating`). The `..` parent row carries the
flags too (it renders the current dir's own stats), so a partially-scanned dir shows `..` as `≥` or the `<dir>`
placeholder.

**Sort-by-size keeps the three classes distinct** and runs in Rust (`file_system/listing/sorting.rs`), not the FE.
`known_dir_size` returns `None` (sorts LAST, by name, regardless of order) for an unknown dir — either incomplete + size
0 (the `<dir>` placeholder) or a not-yet-enriched `None`; a genuinely-empty `0 bytes` and a lower-bound both return
their known numeric value and sort by it. Don't re-conflate unknown with exact-0 in the comparator.
