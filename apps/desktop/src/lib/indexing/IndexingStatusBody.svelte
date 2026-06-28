<script lang="ts">
    // The shared, presentational status body for ONE volume's live indexing:
    // the label / counters+elapsed / detail / bar+percent+ETA, per the M2
    // count-first policy. Rendered by BOTH surfaces — the top-right indicator's
    // drive rows (via `IndexingDriveRow`) and the breadcrumb badge's scanning
    // tooltip — so they show the identical representation.
    //
    // Deliberately presentational: it owns NO stateful `$effect` glue. The
    // ETA sliding-window state lives in each WRAPPER (so two surfaces rendering
    // the same volume can't collide on window state), which injects the result
    // as `windowedEta`. The wrapper also injects `now` (its 1 Hz tick) so the
    // first-scan elapsed clock advances even when progress events stall.
    import { computeScanProgress, computeElapsedEta, formatEta } from './eta'
    import { formatElapsedClock } from './elapsed'
    import type { VolumeIndexActivity, AggregationActivity } from './index-state.svelte'
    import { formatNumber } from '$lib/file-explorer/selection/selection-info-utils'
    import ProgressBar from '$lib/ui/ProgressBar.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import type { MessageKey } from '$lib/intl/keys.gen'

    interface Props {
        activity: VolumeIndexActivity
        /** This volume's aggregation progress, folded in when present; `undefined`
         *  when this drive isn't aggregating. */
        aggregation: AggregationActivity | undefined
        /** The wrapper's 1 Hz tick (`Date.now()`), so the first-scan elapsed clock
         *  advances live even when progress events stall. */
        now: number
        /** The scan/replay ETA from the wrapper's sliding window, already formatted
         *  (and "roughly"-wrapped for a rough first scan). `null` when there's no
         *  windowed estimate (aggregation, or before the window has samples). */
        windowedEta: string | null
    }

    const { activity, aggregation, now, windowedEta }: Props = $props()

    const aggregating = $derived(aggregation != null)
    const aggPhase = $derived(aggregation?.phase ?? '')
    const aggCurrent = $derived(aggregation?.current ?? 0)
    const aggTotal = $derived(aggregation?.total ?? 0)
    const aggStartedAt = $derived(aggregation?.startedAt ?? 0)

    const scanning = $derived(activity.phase === 'scanning')

    // ── Scan ──────────────────────────────────────────────────────────
    const entriesScanned = $derived(activity.entriesScanned)
    const dirsFound = $derived(activity.dirsFound)
    const bytesScanned = $derived(activity.bytesScanned)
    const scanStartedAt = $derived(activity.scanStartedAt)
    const priorTotalEntries = $derived(activity.priorTotalEntries)
    const volumeUsedBytes = $derived(activity.volumeUsedBytes)

    // The live entry/dir tally on its own detail line under the label (the honest
    // primary signal, always shown once the first progress event lands), empty
    // before then so the row falls back to the bare label, never "0 entries, 0 dirs".
    const scanCounters = $derived(
        entriesScanned > 0
            ? tString('indexing.scan.counters', {
                  entriesText: formatNumber(entriesScanned),
                  dirsText: formatNumber(dirsFound),
              })
            : '',
    )

    const scanProgressInfo = $derived(
        computeScanProgress(entriesScanned, bytesScanned, priorTotalEntries, volumeUsedBytes),
    )
    const scanProgress = $derived(scanProgressInfo?.fraction ?? null)
    const scanRough = $derived(scanProgressInfo?.rough ?? false)

    const scanLabel = $derived(tString(scanRough ? 'indexing.scan.labelFirst' : 'indexing.scan.label'))

    // The rough first scan has no trustworthy percent (the byte-ratio sits near
    // 0 on a big volume), so it shows count + elapsed instead of a bar. `null`
    // under a second so the clock never flashes "0:00".
    const scanElapsed = $derived(scanStartedAt > 0 ? formatElapsedClock(now - scanStartedAt) : null)

    // The scan detail line: the counters, plus the elapsed clock for the rough
    // first-scan tier (a calibrated rescan shows its percent + ETA on the bar
    // instead). Empty counters (pre-first-event) → no detail line at all.
    const scanDetail = $derived.by(() => {
        if (scanCounters === '') return null
        if (scanRough && scanElapsed != null) {
            return tString('indexing.scan.countersElapsed', { counters: scanCounters, elapsed: scanElapsed })
        }
        return scanCounters
    })

    // ── Replay ────────────────────────────────────────────────────────
    const eventsProcessed = $derived(activity.replayEventsProcessed)
    const estimatedTotal = $derived(activity.replayEstimatedTotal)
    const replayProgress = $derived(estimatedTotal > 0 ? Math.min(1, eventsProcessed / estimatedTotal) : 0)
    const replayDetail = $derived(tString('indexing.replay.detail', { eventsText: formatNumber(eventsProcessed) }))

    // ── Aggregation (folded into this row when attributed here) ────────
    const phaseToLabelKey: Record<string, MessageKey> = {
        saving_entries: 'indexing.aggregation.savingEntries',
        loading: 'indexing.aggregation.loading',
        sorting: 'indexing.aggregation.sorting',
        computing: 'indexing.aggregation.computing',
        writing: 'indexing.aggregation.writing',
    }

    const aggLabel = $derived(tString(phaseToLabelKey[aggPhase] ?? 'indexing.aggregation.computing'))
    const aggHasProgress = $derived(
        aggPhase === 'saving_entries' || aggPhase === 'computing' || aggPhase === 'writing',
    )
    const aggProgress = $derived(aggHasProgress && aggTotal > 0 ? Math.min(1, aggCurrent / aggTotal) : null)
    // Aggregation's ETA needs no sliding window (a single elapsed extrapolation),
    // so it's computed here from the wrapper's `now` tick rather than injected.
    const aggEta = $derived.by(() => {
        if (aggTotal === 0 || aggCurrent === 0 || aggStartedAt === 0) return null
        const elapsed = (now - aggStartedAt) / 1000
        const remaining = computeElapsedEta(elapsed, aggCurrent, aggTotal - aggCurrent)
        return remaining != null ? formatEta(remaining) : null
    })

    // ── Mode selection (aggregation > scan > replay) ──────────────────
    type Mode = 'aggregation' | 'scan' | 'replay'
    const mode = $derived<Mode>(aggregating ? 'aggregation' : scanning ? 'scan' : 'replay')

    const label = $derived(
        mode === 'aggregation' ? aggLabel : mode === 'scan' ? scanLabel : tString('indexing.replay.label'),
    )
    const detail = $derived(mode === 'scan' ? scanDetail : mode === 'replay' ? replayDetail : null)
    // The scan bar/percent shows ONLY for the calibrated tier (`!scanRough`): the
    // rough first scan leads with count + elapsed and no fabricated percent.
    const scanBarProgress = $derived(scanRough ? null : scanProgress)
    const progress = $derived(
        mode === 'aggregation' ? aggProgress : mode === 'scan' ? scanBarProgress : replayProgress,
    )
    const eta = $derived(mode === 'aggregation' ? aggEta : windowedEta)

    const percent = $derived(progress != null ? Math.min(100, Math.round(progress * 100)) : null)

    const percentDisplay = $derived(
        percent == null
            ? null
            : eta
              ? tString('indexing.progress.percentEta', { percent: String(percent), eta })
              : `${String(percent)}%`,
    )
</script>

<span>{label}</span>
{#if detail}
    <span class="tooltip-detail">{detail}</span>
{/if}
{#if percent != null}
    <div class="tooltip-progress">
        <ProgressBar value={progress ?? 0} size="sm" ariaLabel={label} />
        <span class="tooltip-percent">{percentDisplay}</span>
    </div>
{/if}

<style>
    /* The detail line under the label: the scan's live entry/dir counters (plus
       a "· M:SS" elapsed clock on a first scan) or replay's event count. The
       counters grow without bound, so no `white-space: nowrap`: the line wraps
       within the tooltip's own `max-width` (on `.cmdr-tooltip`) instead of
       overflowing past the right-anchored, viewport-clamped box and clipping
       off the window edge. */
    .tooltip-detail {
        color: var(--color-text-tertiary);
    }

    .tooltip-progress {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    /* Holds the combined "95%, roughly 8s left" line. `tabular-nums` keeps the
       leading percent from reflowing as it ticks. */
    .tooltip-percent {
        font-variant-numeric: tabular-nums;
    }
</style>
