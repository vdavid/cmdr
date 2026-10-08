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
        deriveRunLabel,
        activeStepHintKey,
        stepKindToLabelKey,
        computeSubPhaseToLabelKey,
        runLabelToLabelKey,
        coveragePhaseToLabelKey,
        type IndexRunKind,
        type AggregationSubPhase,
        type IndexStepStatus,
    } from './indexing-steps'
    import { deriveOverallEta } from './overall-eta'
    import type { VolumeIndexActivity, AggregationActivity } from './index-state.svelte'
    import type { ActivityPhase, CoveragePhase, ScanRunKind, StepsAheadMs } from '$lib/ipc/bindings'
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
         *  in its mid-sentence form (it only lands inside `indexing.progress.percentEta`)
         *  and "roughly"-wrapped for a rough first scan. `null` when there's no
         *  windowed estimate (before the window has samples). */
        windowedEta: string | null
        /** The calibrated scan ETA in seconds, the same estimate `windowedEta`
         *  shows, so the overall figure adds to exactly what the step says. `null`
         *  when there's none (or on a rough first scan). */
        windowedEtaSeconds?: number | null
        /** What the steps after each one took on this drive's last completed run of
         *  the same kind (the backend's honest sum), for the overall figure.
         *  `undefined` when there's no plan, which shows no overall line. */
        stepsAhead?: StepsAheadMs
        /** This volume's current top-level pipeline phase (from `getVolumePhase`).
         *  The authoritative driver for the catch-up step, and a tiebreaker for the
         *  others after a mid-scan reload drops the transition-only event. */
        phase: ActivityPhase | undefined
        /** A network (SMB/MTP) volume: its checklist omits the Save-the-file-list
         *  and Catch-up steps (they don't run for an inline network scan). */
        isNetwork: boolean
        /** A drive being covered branch by branch: its checklist collapses to the
         *  one step that happens, since sizes land as the walk goes rather than in
         *  a separate pass afterwards. */
        coveredInPhases: boolean
        /**
         * Which phase of a first index is running, when the backend has said.
         * The header prefers it over the run-kind label, because the ORDER is the
         * feature: someone watching wants to know their own folders come first.
         * `undefined` before the first phase event and after a mid-run reload,
         * which the run-kind label covers.
         */
        coveragePhase?: CoveragePhase
        /** What kind of run this is (from `getVolumeScanRunKind`), for the
         *  run-kind header and the per-step copy. `undefined` when unknown (a
         *  mid-scan reload): the header is omitted rather than guessed. */
        scanRunKind?: ScanRunKind
    }

    const {
        activity,
        aggregation,
        now,
        windowedEta,
        windowedEtaSeconds = null,
        stepsAhead,
        phase,
        isNetwork,
        coveredInPhases,
        coveragePhase,
        scanRunKind,
    }: Props = $props()

    // ── Steps ─────────────────────────────────────────────────────────
    const runKind = $derived<IndexRunKind>(
        activity.phase === 'replaying'
            ? 'replay'
            : coveredInPhases
              ? 'phased'
              : isNetwork
                ? 'network'
                : 'local',
    )
    const aggSubPhase = $derived(aggregation?.phase as AggregationSubPhase | undefined)
    const steps = $derived(deriveSteps({ runKind, phase, aggregationSubPhase: aggSubPhase, scanRunKind }))
    // The run-kind header ("First full scan" / "Full rescan" / "Checking for
    // changes" / "Quick update"), so the user can tell a full walk from a change
    // check from a quick roll-on at a glance.
    const runLabel = $derived(deriveRunLabel(runKind, scanRunKind))
    // What the header actually says: the phase when one has been announced (the
    // order is the feature), else the run-kind label. ❌ Not both — they answer
    // the same question at two zoom levels, and stacking them reads as two runs.
    const headerKey = $derived(
        coveragePhase ? coveragePhaseToLabelKey[coveragePhase] : runLabel ? runLabelToLabelKey[runLabel] : null,
    )
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
                  entries: entriesScanned,
                  dirsText: formatNumber(dirsFound),
                  dirs: dirsFound,
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
    const aggEtaSeconds = $derived.by(() => {
        if (aggTotal === 0 || aggCurrent === 0 || aggStartedAt === 0) return null
        const elapsed = (now - aggStartedAt) / 1000
        return computeElapsedEta(elapsed, aggCurrent, aggTotal - aggCurrent)
    })
    // Mid-sentence: it only ever lands inside `indexing.progress.percentEta`.
    const aggEta = $derived(aggEtaSeconds != null ? formatEta(aggEtaSeconds, 'midSentence') : null)

    // ── Replay inputs (the Update-index step's detail) ────────────────
    const eventsProcessed = $derived(activity.replayEventsProcessed)
    const estimatedTotal = $derived(activity.replayEstimatedTotal)
    const replayProgress = $derived(estimatedTotal > 0 ? Math.min(1, eventsProcessed / estimatedTotal) : 0)
    const replayDetail = $derived(tString('indexing.replay.detail', { eventsText: formatNumber(eventsProcessed), events: eventsProcessed }))

    // ── The active step's detail ──────────────────────────────────────
    // Keyed off the ACTIVE step (not a separate "mode"), so the synthetic
    // activity behind an aggregation-only or reconcile-only row never leaks scan
    // zeros: the catch-up step shows no detail, just its spinner.
    interface ActiveDetail {
        /** The reassuring sub-line above the counters, or `null` for none. */
        hintKey: MessageKey | null
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
                    hintKey: activeStepHintKey('findFiles', scanRunKind, scanRough, coveredInPhases),
                    subLine: scanDetailLine,
                    progress: scanRough ? null : scanProgress,
                    eta: windowedEta,
                }
            case 'saveFileList':
            case 'updateFileList':
                // `saving_entries` is determinate; the step label says it all, so
                // just the bar.
                return { hintKey: null, subLine: null, progress: aggFraction, eta: aggEta }
            case 'computeFolderSizes': {
                // computing/writing have a real fraction; loading/sorting are
                // indeterminate, conveyed by the folder-worded sub-line + spinner.
                const determinate = aggSubPhase === 'computing' || aggSubPhase === 'writing'
                const subKey = aggSubPhase ? computeSubPhaseToLabelKey[aggSubPhase] : undefined
                return {
                    hintKey: null,
                    subLine: subKey ? tString(subKey) : null,
                    progress: determinate ? aggFraction : null,
                    eta: determinate ? aggEta : null,
                }
            }
            case 'updateIndex':
                return { hintKey: null, subLine: replayDetail, progress: replayProgress, eta: windowedEta }
            default:
                // catchUp (indeterminate, spinner only) or no active step (done).
                return null
        }
    })

    // ── The overall figure ────────────────────────────────────────────
    // The active step's own estimate, in seconds: the same number its ETA line
    // shows (`null` where the step shows none, like loading/sorting or catch-up).
    const activeEtaSeconds = $derived.by(() => {
        switch (active?.kind) {
            case 'findFiles':
                return windowedEtaSeconds
            case 'saveFileList':
            case 'updateFileList':
                return aggEtaSeconds
            case 'computeFolderSizes':
                return aggSubPhase === 'computing' || aggSubPhase === 'writing' ? aggEtaSeconds : null
            default:
                return null
        }
    })
    const overall = $derived(deriveOverallEta(active?.kind, activeEtaSeconds, stepsAhead))
    const overallText = $derived(
        overall.kind === 'known'
            ? tString('indexing.overall.eta', { eta: formatEta(overall.seconds, 'midSentence') })
            : overall.kind === 'estimating'
              ? tString('indexing.overall.estimating')
              : null,
    )

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

{#if headerKey}
    <span class="run-kind">{tString(headerKey)}</span>
{/if}
{#if overallText}
    <!-- The whole run's "~X left", answering "when am I done?". The active step's
         own ETA stays in the list below, explaining a long step. -->
    <span class="overall-eta">{overallText}</span>
{/if}
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
                        {#if activeDetail.hintKey}
                            <span class="step-hint">{tString(activeDetail.hintKey)}</span>
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
    /* The run-kind header: what kind of run this checklist is. Quieter than the
       drive-name heading above it, louder than the step detail below. */
    .run-kind {
        color: var(--color-text-secondary);
    }

    /* The overall figure sits under the header at the header's weight: it answers
       the question people actually have, so it reads before the step detail.
       `tabular-nums` keeps it from reflowing as it counts down. */
    .overall-eta {
        color: var(--color-text-secondary);
        font-variant-numeric: tabular-nums;
    }

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

    /* The reassuring sub-line under the active step (a first scan's "this takes a
       while", a change check's "your folder sizes stay visible"): quiet and italic
       so it reads as an aside, not another data line. */
    .step-hint {
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
