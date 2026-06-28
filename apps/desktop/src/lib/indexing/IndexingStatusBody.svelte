<script lang="ts">
    // The shared, presentational status body for ONE volume's live indexing: a
    // per-volume STEP CHECKLIST. Every step shows its state — waiting (a
    // hollow marker), in progress (a spinner), or done (a check) — and the active
    // step carries the live detail beneath it (counters + elapsed, a progress
    // bar + percent + ETA, or an aggregation sub-phase line). Rendered by BOTH
    // surfaces (the corner indicator's drive rows and the breadcrumb badge's
    // scanning tooltip) so they show the identical representation.
    //
    // The steps are COMPOSED from the events that fire for this volume (the pure,
    // unit-tested `deriveSteps`), never a fixed list: a network drive omits the
    // Save and Catch-up steps, a roll-on collapses to one Update step. ALL steps
    // render up-front so the tooltip's height stays stable as steps tick (the
    // tooltip measures once on show; see `IndexingStatusIndicator`'s comment) —
    // only the per-step marker and the single active detail line change.
    //
    // Deliberately presentational: it owns NO stateful `$effect` glue. The ETA
    // sliding-window state and the 1 Hz tick live in the WRAPPER (so two surfaces
    // rendering the same volume can't collide), which injects `now`, `windowedEta`,
    // `phase`, and `isNetwork`.
    import { computeScanProgress, computeElapsedEta, formatEta } from './eta'
    import { formatElapsedClock } from './elapsed'
    import {
        deriveSteps,
        activeStep,
        stepKindToLabelKey,
        computeSubPhaseToLabelKey,
        type IndexRunKind,
        type AggregationSubPhase,
        type IndexStepStatus,
    } from './indexing-steps'
    import type { VolumeIndexActivity, AggregationActivity } from './index-state.svelte'
    import type { ActivityPhase } from '$lib/ipc/bindings'
    import type { MessageKey } from '$lib/intl/keys.gen'
    import { formatNumber } from '$lib/file-explorer/selection/selection-info-utils'
    import ProgressBar from '$lib/ui/ProgressBar.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        activity: VolumeIndexActivity
        /** This volume's aggregation progress, folded into the active step when
         *  present; `undefined` when this drive isn't aggregating. */
        aggregation: AggregationActivity | undefined
        /** The wrapper's 1 Hz tick (`Date.now()`), so the first-scan elapsed clock
         *  and the aggregation ETA advance live even when progress events stall. */
        now: number
        /** The scan/replay ETA from the wrapper's sliding window, already formatted
         *  (and "roughly"-wrapped for a rough first scan). `null` when there's no
         *  windowed estimate (before the window has samples). */
        windowedEta: string | null
        /** This volume's current top-level pipeline phase (from `getVolumePhase`).
         *  The authoritative driver for the catch-up step, and a tiebreaker for the
         *  others after a mid-scan reload drops the transition-only event. */
        phase: ActivityPhase | undefined
        /** A network (SMB/MTP) volume: its checklist omits the Save-the-file-list
         *  and Catch-up steps (they don't run for an inline network scan). */
        isNetwork: boolean
    }

    const { activity, aggregation, now, windowedEta, phase, isNetwork }: Props = $props()

    // ── Steps ─────────────────────────────────────────────────────────
    const runKind = $derived<IndexRunKind>(
        activity.phase === 'replaying' ? 'replay' : isNetwork ? 'network' : 'local',
    )
    const aggSubPhase = $derived(aggregation?.phase as AggregationSubPhase | undefined)
    const steps = $derived(deriveSteps({ runKind, phase, aggregationSubPhase: aggSubPhase }))
    const active = $derived(activeStep(steps))
    const activeLabel = $derived(active ? tString(stepKindToLabelKey[active.kind]) : '')

    const statusToLabelKey: Record<IndexStepStatus, MessageKey> = {
        done: 'indexing.step.statusDone',
        active: 'indexing.step.statusActive',
        pending: 'indexing.step.statusPending',
    }

    // ── Scan inputs (the Find-files step's detail) ────────────────────
    const entriesScanned = $derived(activity.entriesScanned)
    const dirsFound = $derived(activity.dirsFound)
    const bytesScanned = $derived(activity.bytesScanned)
    const scanStartedAt = $derived(activity.scanStartedAt)
    const priorTotalEntries = $derived(activity.priorTotalEntries)
    const volumeUsedBytes = $derived(activity.volumeUsedBytes)

    // The live entry/dir tally, empty before the first progress event so the step
    // falls back to its bare label, never "0 entries, 0 dirs".
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
    // The rough first scan has no trustworthy percent (the byte-ratio sits near 0
    // on a big volume), so it shows count + an elapsed clock instead of a bar.
    const scanRough = $derived(scanProgressInfo?.rough ?? false)
    const scanElapsed = $derived(scanStartedAt > 0 ? formatElapsedClock(now - scanStartedAt) : null)

    const scanDetailLine = $derived.by(() => {
        if (scanCounters === '') return null
        if (scanRough && scanElapsed != null) {
            return tString('indexing.scan.countersElapsed', { counters: scanCounters, elapsed: scanElapsed })
        }
        return scanCounters
    })

    // ── Aggregation inputs (the Save + Compute steps' detail) ─────────
    const aggCurrent = $derived(aggregation?.current ?? 0)
    const aggTotal = $derived(aggregation?.total ?? 0)
    const aggStartedAt = $derived(aggregation?.startedAt ?? 0)
    const aggFraction = $derived(aggTotal > 0 ? Math.min(1, aggCurrent / aggTotal) : null)
    // Aggregation's ETA needs no sliding window (a single elapsed extrapolation),
    // so it's computed here from the wrapper's `now` tick rather than injected.
    const aggEta = $derived.by(() => {
        if (aggTotal === 0 || aggCurrent === 0 || aggStartedAt === 0) return null
        const elapsed = (now - aggStartedAt) / 1000
        const remaining = computeElapsedEta(elapsed, aggCurrent, aggTotal - aggCurrent)
        return remaining != null ? formatEta(remaining) : null
    })

    // ── Replay inputs (the Update-index step's detail) ────────────────
    const eventsProcessed = $derived(activity.replayEventsProcessed)
    const estimatedTotal = $derived(activity.replayEstimatedTotal)
    const replayProgress = $derived(estimatedTotal > 0 ? Math.min(1, eventsProcessed / estimatedTotal) : 0)
    const replayDetail = $derived(tString('indexing.replay.detail', { eventsText: formatNumber(eventsProcessed) }))

    // ── The active step's detail ──────────────────────────────────────
    // Keyed off the ACTIVE step (not a separate "mode"), so the synthetic
    // activity behind an aggregation-only or reconcile-only row never leaks scan
    // zeros: the catch-up step shows no detail, just its spinner.
    interface ActiveDetail {
        /** Show the reassuring "first scan" sub-line above the counters. */
        firstScanHint: boolean
        /** A muted sub-line under the step label (counters, sub-phase, or replay count). */
        subLine: string | null
        /** The progress-bar fraction, or `null` for an indeterminate step. */
        progress: number | null
        /** The ETA to pair with the bar, or `null`. */
        eta: string | null
    }

    const activeDetail = $derived.by<ActiveDetail | null>(() => {
        switch (active?.kind) {
            case 'findFiles':
                return {
                    firstScanHint: scanRough,
                    subLine: scanDetailLine,
                    progress: scanRough ? null : scanProgress,
                    eta: windowedEta,
                }
            case 'saveFileList':
                // `saving_entries` is determinate; the step label says it all, so
                // just the bar.
                return { firstScanHint: false, subLine: null, progress: aggFraction, eta: aggEta }
            case 'computeFolderSizes': {
                // computing/writing have a real fraction; loading/sorting are
                // indeterminate, conveyed by the folder-worded sub-line + spinner.
                const determinate = aggSubPhase === 'computing' || aggSubPhase === 'writing'
                const subKey = aggSubPhase ? computeSubPhaseToLabelKey[aggSubPhase] : undefined
                return {
                    firstScanHint: false,
                    subLine: subKey ? tString(subKey) : null,
                    progress: determinate ? aggFraction : null,
                    eta: determinate ? aggEta : null,
                }
            }
            case 'updateIndex':
                return { firstScanHint: false, subLine: replayDetail, progress: replayProgress, eta: windowedEta }
            default:
                // catchUp (indeterminate, spinner only) or no active step (done).
                return null
        }
    })

    const percent = $derived(
        activeDetail?.progress != null ? Math.min(100, Math.round(activeDetail.progress * 100)) : null,
    )
    const percentDisplay = $derived(
        percent == null
            ? null
            : activeDetail?.eta
              ? tString('indexing.progress.percentEta', { percent: String(percent), eta: activeDetail.eta })
              : `${String(percent)}%`,
    )
</script>

<ul class="step-list">
    {#each steps as step (step.kind)}
        <li
            class="step"
            class:step-active={step.status === 'active'}
            class:step-done={step.status === 'done'}
            class:step-pending={step.status === 'pending'}
        >
            <span class="step-marker" aria-hidden="true">
                {#if step.status === 'active'}
                    <Spinner size="sm" />
                {:else if step.status === 'done'}
                    <Icon name="circle-check" size={14} />
                {:else}
                    <Icon name="circle" size={14} />
                {/if}
            </span>
            <div class="step-body">
                <span class="step-label">{tString(stepKindToLabelKey[step.kind])}</span>
                <span class="sr-only">{tString(statusToLabelKey[step.status])}</span>
                {#if step.status === 'active' && activeDetail}
                    <div class="step-detail">
                        {#if activeDetail.firstScanHint}
                            <span class="first-scan-hint">{tString('indexing.step.findFilesFirstScan')}</span>
                        {/if}
                        {#if activeDetail.subLine}
                            <span class="tooltip-detail">{activeDetail.subLine}</span>
                        {/if}
                        {#if percent != null}
                            <div class="tooltip-progress">
                                <ProgressBar value={activeDetail.progress ?? 0} size="sm" ariaLabel={activeLabel} />
                                <span class="tooltip-percent">{percentDisplay}</span>
                            </div>
                        {/if}
                    </div>
                {/if}
            </div>
        </li>
    {/each}
</ul>

<style>
    .step-list {
        list-style: none;
        margin: 0;
        padding: 0;
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
    }

    /* Marker and the step body sit side by side; the marker stays top-aligned so
       it pins to the label even when the active step's detail wraps below. */
    .step {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-xs);
    }

    /* A fixed-size slot so the spinner (sm, 12px) and the 14px markers share one
       footprint — the row never shifts as a step ticks from waiting to done. */
    .step-marker {
        flex-shrink: 0;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 14px;
        height: 14px;
        /* Nudge to optically center on the label's cap height. */
        margin-top: 1px;
    }

    .step-body {
        flex: 1;
        min-width: 0;
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
    }

    /* The focal hierarchy: the active step is full-strength, done steps recede
       (their work is finished), pending steps are quietest (not here yet). The
       eye lands on the one step that's live. */
    .step-label {
        color: var(--color-text-tertiary);
    }
    .step-active .step-label {
        color: var(--color-text-primary);
    }
    .step-done .step-marker {
        /* The completed check reads as quietly affirmative, not a loud success. */
        color: var(--color-text-secondary);
    }
    /* The active marker is the <Spinner>, which carries its own accent ring, so it
       needs no color here. */
    .step-pending .step-marker {
        color: var(--color-text-tertiary);
        opacity: 0.6;
    }

    .step-detail {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
    }

    /* The detail lines under the active step: the scan's live counters (plus a
       "· M:SS" clock on a first scan), the folder-sizing sub-phase, or the replay
       count. The counters grow without bound, so no `white-space: nowrap`: the
       line wraps within the tooltip's `max-width` (on `.cmdr-tooltip`) instead of
       overflowing past the right-anchored, viewport-clamped box. */
    .tooltip-detail {
        color: var(--color-text-tertiary);
    }

    /* The reassuring first-scan sub-line: quiet and italic so it reads as an aside,
       not another data line. */
    .first-scan-hint {
        color: var(--color-text-tertiary);
        font-style: italic;
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
        color: var(--color-text-tertiary);
    }
</style>
