# Indexing (frontend) details

Depth for the frontend indexing bridge. `CLAUDE.md` holds the must-knows; this file holds the full API, event table,
tooltip content, ETA mechanics, and tests.

## Public API (`index.ts`)

Call scan-state getters from `.svelte` files or `.svelte.ts` reactive contexts.

```ts
isScanning(): boolean
getEntriesScanned(): number
getDirsFound(): number
getBytesScanned(): number            // resolved post-dedup physical bytes scanned (tier-2 numerator)
getScanStartedAt(): number           // Date.now() at scan start; 0 on late-join (no wall-clock backfill)
getPriorTotalEntries(): number | null      // prior completed scan's entry total (tier-1 denominator)
getPriorScanDurationMs(): number | null    // prior completed scan's duration (tier-1 ETA seed)
getVolumeUsedBytes(): number | null        // scanned volume's used bytes (tier-2 denominator)
isAggregating(): boolean
getAggregationPhase(): string        // 'saving_entries' | 'loading' | 'sorting' | 'computing' | 'writing'
getAggregationCurrent(): number
getAggregationTotal(): number
getAggregationStartedAt(): number    // Date.now() timestamp
isReplaying(): boolean
getReplayEventsProcessed(): number
getReplayEstimatedTotal(): number
getReplayStartedAt(): number         // Date.now() timestamp
initIndexState(): Promise<void>      // call once at app mount
destroyIndexState(): void            // call at app teardown
initIndexEvents(onDirUpdated: (paths: string[]) => void): Promise<UnlistenFn>
```

## Scan-state events (`index-state.svelte.ts`)

Eight Tauri events drive the module-level `$state`:

- **`index-scan-started`** (`{ volumeId, priorTotalEntries, priorScanDurationMs, volumeUsedBytes }`): `scanning = true`,
  reset counters, `scanStartedAt = Date.now()`, stash the per-scan calibration.
- **`index-scan-progress`** (`{ volumeId, entriesScanned, dirsFound, bytesScanned }`): update counters.
- **`index-scan-complete`** (`{ volumeId, totalEntries, totalDirs, durationMs }`): `scanning = false`, set final counts,
  reset aggregation.
- **`index-rescan-notification`** (`{ volumeId, reason, details }`): show an info toast with a reason-specific message.
- **`index-replay-progress`** (`{ volumeId, eventsProcessed, estimatedTotal }`): `replaying = true` on first, update
  counters.
- **`index-replay-complete`** (`{ volumeId, durationMs }`): reset replay state.
- **`index-aggregation-progress`** (`{ phase, current, total }`): `aggregating = true`, update phase/progress/ETA.
- **`index-aggregation-complete`** (`()`): reset aggregation state.

## Status indicator tooltip content

The component renders the content inside a `<div hidden>` host and passes the inner content div (not the hidden host) to
the tooltip action via `contentEl`, so the adopted element doesn't carry `hidden` into the tooltip.

- **Scan**: a two-tier label + counters, plus `ProgressBar` + percent + ETA when a denominator exists. Tier 1
  (calibrated): "Scanning your drive... 42,000 entries, 1,200 dirs" with "42% · 1m 20s left". Tier 2 (first scan,
  rough): "Scanning your drive (first scan)... ..." with "36% · roughly 19m left". `computeScanProgress` null → the
  counter-only label.
- **Aggregation**: phase label ("Saving entries...", "Loading directories...", "Sorting directories...", "Computing
  directory sizes...", "Saving directory sizes...") + `ProgressBar` + percent + ETA for the phases that have progress
  (`saving_entries`, `computing`, `writing`).
- **Replay**: "Updating index..." + "N events processed" + `ProgressBar` + blended ETA.

The hourglass is a ~14px `<Icon>` (the same icon as the size-column stale indicator), `position: absolute` top/right at
`var(--spacing-sm)`, tertiary text color, gentle opacity pulse gated behind `prefers-reduced-motion: reduce`.

## ETA mechanics (`eta.ts`)

Pure helpers. Aggregation uses a single elapsed extrapolation. Scan and replay blend that 50-50 with a sliding-window
rate over the last ~5 seconds (early extrapolation alone is wildly wrong). The window-snapshot collection is the only
stateful glue and stays in the component; it feeds the pure `pruneSnapshots` / `computeWindowEta` / `blendEtas` /
`formatEta`. Tier 1's prior-duration seed (`priorScanDurationMs − elapsed`, ms→seconds) covers the gap before the window
has samples. Tier 2's ETA is prefixed "roughly".

## Tests

- **`eta.test.ts`**: the pure ETA helpers (thresholds, elapsed + window estimation, blending, snapshot pruning), plus
  `computeScanProgress` (tier selection, both clamps, null/zero-denominator fallbacks) and the `formatEta` non-finite
  pin.
- **`IndexingStatusIndicator.a11y.test.ts`**: tier-3 axe checks for idle (renders nothing), scanning (counter-only and
  calibrated-with-bar), and aggregating-with-progress, mocking `index-state.svelte`.

The reactive event-driven glue in `index-state.svelte.ts` is allowlisted in `coverage-allowlist.json`. Manual end-to-end
testing runs the Rust indexer via `pnpm dev`.

## Dependencies

- `$lib/ipc/bindings`: `commands` (status query).
- `$lib/tauri-commands`: the `tauri-specta`-typed indexing event wrappers (`onIndexScan*`, `onIndexAggregation*`,
  `onIndexReplay*`, `onIndexRescanNotification`, `onIndexDirUpdated`) + `UnlistenFn`, in `tauri-commands/indexing.ts`.
- `$lib/ui/toast`: `addToast` (rescan notification toasts).
- `$lib/file-explorer/selection/selection-info-utils`: `formatNumber` (indicator only, `'en-US'` locale).
- `$lib/tooltip/tooltip`: `tooltip` action with the `contentEl` live-content param (indicator only).
- `$lib/ui/ProgressBar.svelte`: size `sm` (indicator tooltip).

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
raw epochs. The full data model is in the backend `indexing/DETAILS.md` § "Honest sizes" and
`docs/specs/2026-06-25-honest-index-sizes-plan.md`.

**Content state — `getDirSizeDisplayState(recursiveSize, complete, stale, updating)`** (`views/full-list-utils.ts`), a
pure function and the single source of truth:

- `recursiveSize == null` → `'dir'` (the `<dir>` placeholder), or `'scanning'` when `updating`.
- `complete === false && size === 0` → `'unknown'` → `—` (`UNKNOWN_SIZE_GLYPH`). The crux: distinct from a
  genuinely-empty `0 bytes`.
- `complete === false && size > 0` → `'lower-bound'` → `≥` (`LOWER_BOUND_GLYPH`) prefix + the formatted size.
- `complete === true && stale === true` → `'size-stale'` → the formatted size, muted (reduced opacity, matching the
  yellow=stale freshness badge; tunable, plan §1I open decision #1).
- otherwise → `'size'` → the plain formatted size (incl. a genuinely-empty `0 bytes`).
- Absent `complete`/`stale` (a dir enriched before the flags, or a fixture) ⇒ treated as exact + fresh.

**The in-flux hourglass is ORTHOGONAL** — `isDirSizeUpdating(indexing, pending)` (`indexing || pending`), applied on top
of any content state. A dir can be both `'size-stale'` (freshness) and updating (in-flux). The `≥`/`—` are symbols, not
translatable copy; the per-state explanation is a one-line label in `buildDirSizeTooltip` (keys
`fileExplorer.dirSize.{lowerBoundLine,unknownTooltip,staleLine}`).

**Three consumers, kept in lockstep** (or rendered text and pre-measured width drift): `FullList.svelte` (the Size
cell), `SelectionInfo.svelte` (Brief-mode status bar, so it matches Full), and `measure-column-widths.ts` (reserves the
`—`/`≥` glyph widths and the hourglass icon when `isDirSizeUpdating`). The `..` parent row carries the flags too (it
renders the current dir's own stats), so a partially-scanned dir shows `..` as `≥`/`—`.

**Sort-by-size keeps the three classes distinct** and runs in Rust (`file_system/listing/sorting.rs`), not the FE.
`known_dir_size` returns `None` (sorts LAST, by name, regardless of order) for an unknown dir — either `—` (incomplete +
size 0) or a not-yet-enriched `None`; a genuinely-empty `0 bytes` and a lower-bound both return their known numeric
value and sort by it. Don't re-conflate unknown with exact-0 in the comparator.
