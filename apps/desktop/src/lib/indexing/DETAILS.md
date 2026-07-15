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

**Gotcha — store a FRESH object on every progress tick, never mutate-in-place + re-set.** `SvelteMap.set` bumps its
per-key reactive source only when the stored value REFERENCE changes; re-setting the same (just-mutated) object is a
no-op to its reactivity. So the scan-progress and replay-progress handlers build a new object literal each tick
(`activity.set(id, { ...prev, entriesScanned, ... })`), exactly as the aggregation handler does. Mutating `prev` in
place and re-setting it instead leaves `$derived`/`$effect` consumers (the live scan/replay counter in
`IndexingStatusBody`) frozen at the first value while the backend scans on — the data is correct, only the reactive
notification is lost. The reactivity regression test in `index-state.svelte.test.ts` runs a real `$effect` over the
getter and asserts it re-fires on the second tick (the getter-only tests can't catch this, since they read the stored
value directly).

### Public API (`index.ts`)

The barrel exports the lifecycle + the cross-module reads: `isVolumeScanning` / `getEntriesScanned` / `ROOT_VOLUME_ID`
(SearchDialog), `getVolumeActivity` / `getVolumeAggregation` (the breadcrumb badge's scanning tooltip, in
`navigation/`), `getVolumePhase` (the per-volume step checklist), plus `initIndexState` / `destroyIndexState` /
`initIndexEvents`. The indicator (same dir) imports the rest directly from `./index-state.svelte`:

```ts
// Multi-drive API (the indicator):
getActiveIndexVolumes(): VolumeIndexActivity[]   // every scanning/replaying volume
getVolumeActivity(volumeId): VolumeIndexActivity | undefined  // ONE volume's scan/replay activity (the breadcrumb badge)
isAnyVolumeIndexing(): boolean                    // scan/replay map non-empty OR any volume aggregating (visibility gate)
getVolumeAggregation(volumeId): AggregationActivity | undefined  // that volume's live aggregation, or undefined
getVolumePhase(volumeId): ActivityPhase | undefined  // that volume's current mid-pipeline phase, or undefined (step checklist)
getAggregatingVolumeIds(): string[]               // every volume currently aggregating
getActivePhaseVolumeIds(): string[]               // every volume with a live phase (incl. reconcile, no scan/agg entry)
placeholderActivity(volumeId): VolumeIndexActivity  // a zero-valued activity for an aggregation-/reconcile-only row
ROOT_VOLUME_ID: 'root'                             // the boot-disk volume id (the checklist shape is keyed on `category`, not this — see `isNetworkIndexRun`)
// Per-volume predicates (per-folder size hourglass scopes to the folder's own volume):
isVolumeScanning(volumeId): boolean  // is THIS volume scanning (NOT replaying) right now
isVolumeAggregating(volumeId): boolean  // is THIS volume aggregating right now
getEntriesScanned(): number          // the ROOT volume's live count (SearchDialog index-build progress)
// Lifecycle:
initIndexState(): Promise<void>      // call once at app mount
destroyIndexState(): void            // call at app teardown
initIndexEvents(onDirUpdated: (paths: string[]) => void): Promise<UnlistenFn>
```

`index-events.ts` bridges the `index-dir-updated` event to `onDirUpdated`. Each callback gets a BATCH of paths (multiple
during DB replay, typically one in live FS-watch). `DualPaneExplorer` checks each path against each pane's current dir
with a path-prefix comparison, which relies on trailing-slash normalization.

## Scan-state events (`index-state.svelte.ts`)

Ten Tauri events drive the state. All of them carry a `volumeId`: scan and replay key the live-`activity` map,
aggregation keys its own `aggregation` map, and the phase event keys its own `phase` map.

- **`index-scan-started`** (`{ volumeId, priorTotalEntries, priorScanDurationMs, volumeUsedBytes }`): create/replace the
  volume's `activity` entry (`phase: 'scanning'`, `scanStartedAt = Date.now()`, stash the calibration).
- **`index-scan-progress`** (`{ volumeId, entriesScanned, dirsFound, bytesScanned }`): update that volume's counters
  (seeds a scanning entry if the started event was missed, e.g. mid-scan reload).
- **`index-scan-complete`** (`{ volumeId, totalEntries, totalDirs, durationMs }`): remove the volume's `activity` entry.
- **`index-scan-aborted`** (`{ volumeId }`): a scan ended WITHOUT completing — a network (SMB/MTP) disconnect/cancel/
  timeout, or a local external drive whose root became unlistable because the volume was yanked mid-scan — so no
  `index-scan-complete` fires. Remove the volume's `activity` AND `aggregation` entries — otherwise the partial scan
  leaves a stuck "scanning" row in the corner and the badge tooltip. Carries no completion facts (it isn't a finished
  index). The badge dot color is handled separately by the manager's freshness subscription. Emitted by
  `network_scan.rs`'s disconnect (→ Stale) and cancel/fail (→ not-indexed) arms.
- **`index-rescan-notification`** (`{ volumeId, reason, details }`): show an info toast with a reason-specific message.
- **`index-replay-progress`** (`{ volumeId, eventsProcessed, estimatedTotal }`): create/replace the volume's `activity`
  entry as `phase: 'replaying'`, update counters.
- **`index-replay-complete`** (`{ volumeId, durationMs }`): remove the volume's replay entry.
- **`index-aggregation-progress`** (`{ volumeId, phase, current, total }`): upsert the volume's `aggregation` entry
  (phase/progress, plus a `startedAt` ETA clock reset on each phase change).
- **`index-aggregation-complete`** (`{ volumeId }`): remove that volume's `aggregation` entry.
- **`index-phase-changed`** (`{ volumeId, phase: ActivityPhase }`): the volume's top-level pipeline phase changed. Set
  the `phase` map entry for the active steps (`scanning` / `aggregating` / `reconciling` / `replaying`); DELETE it on
  the terminal `live` / `idle` transitions (the pipeline ended) and on `index-scan-aborted` (the cancel/fail abort arm
  fires no phase event). So a present `phase` entry always means "this volume is at this step right now" — the spine of
  the step checklist, and the only signal for the reconcile step. Per-volume, unlike the global debug-window phase
  timeline. Fires only on transitions, so after a mid-scan reload the current phase is unknown until the next
  transition; the reconcile step is briefly unobservable then (accepted — `index-phase-changed` is transition-only and
  `VolumeIndexStatus` carries no phase by design; see the backend `indexing/DETAILS.md`). Branch on the typed
  `ActivityPhase` variant, never the wording.

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
`{name}` passthrough) is ALWAYS shown — even for a single drive — so the user can always tell which drive is indexing;
it reads as a title (full-strength `--color-text-primary`, bold) above the status line. The component renders the
content inside a `<div hidden>` host and passes the inner content div (not the hidden host) to the tooltip action via
`contentEl`, so the adopted element doesn't carry `hidden` into the tooltip.

**Body / wrapper split.** `IndexingDriveRow` is a thin WRAPPER: it owns the stateful glue — this volume's two ETA
sliding windows (scan + replay) and a 1 Hz `now` tick (gated on scanning/aggregating) — plus the reactive reads
`getVolumePhase(volumeId)` and `isNetwork` (`isNetworkIndexRun(volumeId, getVolumes())`, keyed on the volume's
`category`: `network`/`mobile_device` → network checklist, every local category → local — so a non-root LOCAL drive like
a USB stick gets the Save + Catch-up steps, NOT the network shape a `volumeId !== root` test would have handed it), and
renders the heading + a `IndexingStatusBody`. The body is PRESENTATIONAL: it takes the `activity`, `aggregation`, `now`,
`windowedEta`, `phase`, and `isNetwork`, and renders the step checklist. No `$effect`, no window state in the body, so
two surfaces rendering the same volume can't collide on window state — each WRAPPER instance keeps its own window. Both
surfaces (the corner indicator and the breadcrumb badge's scanning tooltip) render `IndexingDriveRow` (the badge with
the heading off), so the representation is identical.

## Step checklist

The body renders a per-volume CHECKLIST (`<ul>`/`<li>`): every step shows its state — waiting (a hollow `circle`), in
progress (a `<Spinner>`), or done (a `circle-check`) — and the ACTIVE step carries the live detail beneath its label. A
visually-hidden status word ("Done" / "In progress" / "Not started") conveys state to screen readers (the markers are
decorative `aria-hidden`); the active step's bar takes the step label as its `aria-label`.

**Steps are composed from the events that fire for THIS volume** by the pure `deriveSteps` (`indexing-steps.ts`), never
a fixed list. The run kind picks the ordered list:

- **local** (the `root` volume): Find files → Save the file list → Compute folder sizes → Catch up on recent changes.
- **network** (SMB/MTP): Find files → Compute folder sizes. The Save and Catch-up steps don't appear: a network scan
  inserts entries inline during the walk (no `saving_entries` sub-phase) and emits no top-level Aggregating/Reconciling
  phase (Scanning → Live directly), so those steps never run. Compute is driven off the aggregation SUB-phase events,
  which DO fire for network.
- **replay** (an event-log roll-on): a single Update index step.

State comes from a "furthest reached" index across the signals (the typed `ActivityPhase` from `getVolumePhase` + the
live aggregation sub-phase), so steps stay monotonic (everything before the active one done, everything after pending)
and survive a mid-scan reload: when the transition-only phase event is gone, the aggregation sub-phase still proves how
far we are. The accepted gap: after a reload landing mid-RECONCILE (no scan, no aggregation, no phase), the catch-up
step shows not-yet-active — but in that window the surface isn't rendered at all (no live entry), so it falls back to
the badge's static "Scanning your drive…" text.

**Two label maps, separate on purpose**: `indexing-steps.ts`'s `stepKindToLabelKey` keys the step labels off the typed
`IndexStepKind`; its `computeSubPhaseToLabelKey` (imported by `IndexingStatusBody`) keys the compute step's
folder-worded sub-line off the aggregation sub-phase string. Branch on the typed discriminants, never wording.

**The active step's detail**, keyed off the active step (not a separate "mode"), so the synthetic activity behind an
aggregation-only or reconcile-only row never leaks scan zeros:

- **Find files**: the count-first scan detail. Tier 1 (calibrated, prior-scan denominator) → counters + a `ProgressBar`
  with "42%, 1m left". Tier 2 (rough first scan) → a "First scan, so this can take a while" sub-line + "42,000 entries,
  1,200 dirs · 2:34" (count + an **elapsed clock**), NO bar (the byte-ratio sits near 0 early, so a precise percent
  would lie). The clock advances off the wrapper's 1 Hz tick (`Date.now()` in a `$derived` isn't reactive, so it'd
  freeze on a stall — the reported NAS case); `formatElapsedClock` returns `null` under a second so it never flashes
  "0:00".
- **Save the file list** (`saving_entries`): a `ProgressBar` + percent + ETA. No sub-line (the step label says it).
- **Compute folder sizes** (`loading → sorting → computing → writing`): a folder-worded sub-line ("Loading folders…",
  "Sorting folders…", "Computing folder sizes…", "Saving folder sizes…") + a bar for the determinate sub-phases
  (`computing`, `writing`); `loading`/`sorting` are indeterminate (the step spinner + sub-line convey liveness, NO
  nested second spinner). Aggregation's window-free ETA is computed in the body from `now`.
- **Catch up on recent changes** (the `reconciling` phase): indeterminate, no detail — just the spinner + label.
- **Update index** (replay): "N events processed" + a bar + the blended replay ETA.

**Per-step ETA, no overall ETA.** Only the active step's own ETA shows, where its denominator is trustworthy. A true
overall "~Xm left" is deliberately deferred with its backend per-phase calibration (the step-of-N structure carries "how
far" honestly without it): see
[`docs/specs/later/drive-index-overall-eta.md`](../../../../../docs/specs/later/drive-index-overall-eta.md).

**Reconcile visibility.** The catch-up step has no scan/aggregation entry — only the `phase` event marks it — so
`isAnyVolumeIndexing` and the indicator/badge include `phase`-only volumes (`getActivePhaseVolumeIds`, a
`placeholderActivity` for the row). Without that the surface would vanish the instant aggregation completes.

**Multi-volume collapse.** The corner expands the PRIMARY (first) drive's full `IndexingDriveRow` checklist and
collapses each secondary drive to a one-line `IndexingDriveSummary` (heading + active step label + a compact metric: a
percent where the denominator is trustworthy, else the running count via `indexing.summary.found`). The badge always
shows its own volume's full checklist.

**Height stability.** ALL steps render up-front so the tooltip's height stays stable as steps tick (the tooltip action
measures once on show and doesn't re-measure as the adopted body's content grows; see `IndexingStatusIndicator`'s
`min-width` comment). Only the per-step marker and the single active detail line change.

The percent and ETA render as one span joined through `indexing.progress.percentEta` (`"{percent}%, {eta}"`), so the
comma separator is translatable per locale (e.g. a full-width comma in `zh`, a space before `%` in `de`/`fr`); without
an ETA yet, just the bare percent shows. The label span has no `white-space: nowrap`: the scan counters grow without
bound, so the label wraps within the tooltip's `max-width` (on `.cmdr-tooltip`) instead of overflowing past the
right-anchored, viewport-clamped box and clipping off the window edge.

The hourglass is a ~14px `<Icon>` (the same icon as the size-column stale indicator), `position: absolute` top/right at
`var(--spacing-sm)`, tertiary text color, gentle opacity pulse gated behind `prefers-reduced-motion: reduce`.

## Image-enrichment publisher (plan M5)

Image indexing (`media_index/`, the on-device Vision OCR + tags + embeddings pass) joins the SAME top-right indicator as
a second publisher, alongside the drive indexer — not a second corner widget. `media-enrich-state.svelte.ts` is its
reactive core (mirroring `index-state.svelte`), and `IndexingStatusIndicator` renders an `IndexingEnrichRow` per
enriching volume below the drive rows.

**Two backend events** (`media_index/events.rs`, typed `on*` wrappers in `tauri-commands/media-index.ts`):

- **`media-enrich-progress`** (`{ volumeId, done, total, bytesDone, bytesTotal }`): throttled (pass start, then ≤ every
  500 ms or 100 images, and a final tick). `total` / `bytesTotal` are the ENRICHABLE-subset denominators (images passing
  the coverage gates), NEVER the full walked set — a raw walked-set denominator rebuilds the never-finishes bug the
  slider fixed, inside the indicator. `done` counts every processed subset image (enriched, already-current, or a quiet
  vanished/tiny/phantom skip), so it reaches `total` on completion. Upserts the volume's row (a FRESH object per tick),
  clearing any paused flag (the pass resumed).
- **`media-enrich-terminal`** (`{ volumeId, reason }`): exactly one per pass on EVERY exit path (a `Drop`-guard in the
  scheduler guarantees it — an error `?`-bubble still emits `Failed`). The typed `reason` decides the FE behavior:
  `completed` / `cancelled` / `failed` CLEAR the row; `pausedWaitingForIdle` / `pausedDisconnected` RE-VOICE it paused
  (keeping the last counts), so it never sticks at "enriching" (the stuck-row bug `index-scan-aborted` fixed for drive
  scans). Branch on `reason.kind`, never wording.

**The visibility gate** (`isAnyVolumeEnriching`) counts only ACTIVELY-enriching entries, not paused ones: a paused-only
volume (a disconnected NAS) never pins the hourglass up forever, but its row still shows while the hourglass is up for
another reason (the settings panel also voices `paused` via `media_index_volume_state`). This is the deliberate
reconciliation of "terminal events clear the row" with "paused states voiced": a pause re-labels rather than clears, and
the gate keeps it from lighting the corner on its own.

**Listen-first-then-query at init** (`initMediaEnrichState`): with plan M1, enrichment can start at backend setup BEFORE
the frontend mounts, so the pass-start event is lost. After registering the listeners, seed the ROOT volume from
`media_index_volume_state` if it's enriching (`done = enrichedCount − keptCount` capped, `total = coveredQualifyingCount`),
mirroring `initIndexState`'s root-only backfill; network volumes hydrate from their next progress tick.

**`IndexingEnrichRow`** is the WRAPPER (owns its own rate/ETA sliding window over `done` + a 1 Hz tick, like
`IndexingDriveRow`): an images bar + a bytes bar (both aria-labeled), an "N of M images" line, images/min
(`computeWindowRate`), and the per-volume ETA (`blendEtas` over elapsed + windowed). A paused row shows the paused
message and no bars. No overall ETA (per-row only, consistent with the drive rows). Tests:
`media-enrich-state.svelte.test.ts` (terminal clears / re-voices, listen-first seeding, fresh-object reactivity) and
`IndexingEnrichRow.a11y.test.ts` (mirrors `IndexingDriveRow.a11y.test.ts`).

## Two-tier scan progress (`computeScanProgress`)

- **Tier 1** (`priorTotalEntries` present): `entriesScanned / priorTotalEntries`, clamped to 0.99, apples-to-apples.
- **Tier 2** (`volumeUsedBytes` present): `bytesScanned / volumeUsedBytes`, clamped to 0.95 (APFS clones overshoot the
  statfs denominator), flagged `rough`.
- **Neither** → `null` (counter-only, no bar).

`rough` is computed but its fraction is NOT rendered as a bar/percent: the row shows count + an elapsed clock for the
first-scan tier instead (see "Status indicator tooltip content" above). The flag still drives the "(first scan)" label
and the render-gate. The ETA window samples the SAME counter the tier divides by (entries for tier 1, bytes for tier 2)
— don't mix them. `formatEta` carries a `Number.isFinite` guard so a dropped null gate can't surface "Infinitym left".

### Tier recovery after a mid-scan reload

`get_index_status` backfills the tier inputs (root-only) after a window reload lands mid-scan, so the bar/ETA recover
without waiting for the next full scan. `scanStartedAt` can't cross IPC, so it stays 0 and the ETA degrades until the
sliding window fills again (accepted). Tier 1 reads the prior scan's totals from the nested `indexStatus` meta via
`parseMetaNumber`.

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
- **`indexing-steps.test.ts`**: the pure `deriveSteps` (TDD'd red→green) — local full scan through all four steps,
  network (no Save/Catch-up), replay (one step), the aggregation-sub-phase-alone derivation after a reload, and the
  accepted reconcile-after-reload gap (catch-up stays pending).
- **`IndexingStatusIndicator.a11y.test.ts`**: tier-3 axe checks for idle (renders nothing), single-drive scanning
  (counter-only, first-scan tier-2, calibrated-with-bar — each asserting the always-on drive heading), aggregating, the
  primary-expands-secondary-collapses multi-drive case, and a phase-only mid-reconcile volume (visible, catch-up
  active). Mocks both `index-state.svelte` (incl. `getVolumePhase` / `getActivePhaseVolumeIds` / `placeholderActivity` /
  `ROOT_VOLUME_ID`) and the volume store's `getVolumes`.
- **`IndexingDriveRow.a11y.test.ts`**: per-row axe checks for the wrapper, including the first-scan tier-2 row (count +
  elapsed, no `progressbar` role).
- **`IndexingStatusBody.svelte.test.ts`** + **`IndexingStatusBody.a11y.test.ts`**: the shared checklist body, mounted
  per scenario from a fixture — the four local steps with state, tier-1 bar, tier-2 first scan (count + elapsed +
  first-scan hint, NO progressbar), counter-only, the compute step (sub-phase line + bar), the reconcile step (catch-up
  active, no detail), network (no Save/Catch-up), and replay (one step). The a11y test pins the `<ul>`/`<li>` roles +
  the visually-hidden status words.
- **`index-state.svelte.test.ts`**: per-volume aggregation attribution, plus `index-scan-aborted` clearing a volume's
  activity + aggregation (the network-abort stuck-row regression), plus the per-volume `phase` map
  (`index-phase-changed` sets the active phase, `live` / `idle` and an abort clear it, volumes stay independent).
- **`elapsed.test.ts`**: `formatElapsedClock` (sub-second → `null`, `m:ss` formatting, zero-padding, flooring).

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
typed discriminators map to catalog KEYS (`rescanReasonToMessageKey`; the checklist's `stepKindToLabelKey` /
`computeSubPhaseToLabelKey` in `indexing-steps.ts`), resolved at render time — branch on the typed enum, never on
wording. The `'bytes'` / `'entries'` scan-unit tags and `'scan'`/`'aggregation'`/`'replay'` mode strings are internal
discriminators, not copy. Base-en output is parity-pinned by `indexing-i18n-parity.test.ts`.

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
