<script lang="ts">
    // The corner progress chip: an action word and a short bar saying that
    // something is still running after the user backgrounded it. It's a preview,
    // not a queue — one operation, no numbers, no overflow count. The detail
    // lives in its tooltip, and the whole queue is one click away.
    //
    // Which operation, and every rule about when to stay quiet: `operation-chip.ts`.
    import ProgressBar from '$lib/ui/ProgressBar.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { formatInteger } from '$lib/intl/number-format'
    import { formatDuration } from '$lib/units'
    import { getMainWindowOperationRows } from '$lib/file-operations/queue/main-window-operations.svelte'
    import { getForegroundOperationId } from '$lib/file-operations/foreground-operation.svelte'
    import { openQueueWindow } from '$lib/file-operations/queue/queue-window'
    import { CHIP_SETTLE_MS, destinationName, pickChipOperation } from './operation-chip'

    const candidate = $derived(pickChipOperation(getMainWindowOperationRows(), getForegroundOperationId()))
    /** Derived on its own so the settle effect below depends on WHICH operation
     *  is up, not on its progress: a `$derived` string doesn't re-notify while
     *  its value is unchanged, so the 200 ms progress ticks don't restart the
     *  timer (which would keep the chip hidden forever). */
    const candidateId = $derived(candidate?.row.snapshot.operationId ?? null)

    /** The operation the chip has settled on. See the settle effect. */
    let settledId = $state<string | null>(null)
    /** Non-reactive mirror of "the chip is currently showing something". The
     *  effect reads it and writes `settledId`; reading the state it writes would
     *  make it re-run itself. */
    let showing = false

    // Work shorter than a moment shouldn't flash the corner (the house rule:
    // under ~1 second, no indicator). Waiting a beat before the chip's FIRST
    // appearance also closes a race: an operation reaches this window's store
    // when the backend registers it, a hair before the start command's response
    // lets the foreground modal claim it, so showing instantly could blink a
    // chip for an operation the modal is about to own. Once the chip is up, a
    // handover to the next operation is immediate — the corner stays put rather
    // than blinking between two running transfers.
    $effect(() => {
        const id = candidateId
        if (id === null) {
            showing = false
            settledId = null
            return
        }
        if (showing) {
            settledId = id
            return
        }
        const timer = setTimeout(() => {
            showing = true
            settledId = id
        }, CHIP_SETTLE_MS)
        return () => {
            clearTimeout(timer)
        }
    })

    const visible = $derived(candidate !== null && settledId === candidateId)

    /** The action word, always: "Copying", "Moving to trash". The tooltip leads
     *  with it even while paused, where the chip itself says "Paused". */
    const verb = $derived(
        candidate === null ? '' : tString('queue.row.label', { type: candidate.row.snapshot.operationType }),
    )
    const pausedWord = $derived(tString('queue.row.status', { status: 'paused' }))
    const chipLabel = $derived(candidate?.paused === true ? pausedWord : verb)
    const percentText = $derived(formatInteger(candidate?.percent ?? 0))

    /** The tooltip's trailing fact: how long is left, or that it's paused (a
     *  paused operation has no honest countdown). Absent while the backend's
     *  estimate is still warming up. */
    const detail = $derived.by(() => {
        if (candidate === null) return null
        if (candidate.paused) return pausedWord
        const eta = candidate.row.etaSecondsDisplay
        // The SMOOTHED ETA from the store, never `progress.etaSeconds`: the
        // queue window renders the smoothed one, and two surfaces disagreeing
        // about the same operation is a bug we've already shipped once.
        if (eta === null) return null
        return tString('fileOperations.transferProgress.etaRemaining', { duration: formatDuration(eta) })
    })

    const tooltipText = $derived.by(() => {
        if (candidate === null) return ''
        const count = candidate.row.progress?.filesTotal ?? 0
        const destination = destinationName(candidate.row.snapshot.destination)
        return tString('queue.chip.tooltip', {
            label: verb,
            count,
            countText: formatInteger(count),
            hasDestination: destination === '' ? 'no' : 'yes',
            destination,
            percentText,
            hasDetail: detail === null ? 'no' : 'yes',
            detail: detail ?? '',
        })
    })

    const ariaLabel = $derived(tString('queue.chip.ariaLabel', { label: chipLabel, percentText }))

    /** The tooltip action ADOPTS this element, and an adopted element keeps its
     *  own `hidden` attribute — so it's the inner div that's bound here, never
     *  the hidden wrapper (an empty tooltip is what you get otherwise). */
    let tooltipContent = $state<HTMLDivElement>()
</script>

{#if visible && candidate}
    <button
        class="operation-chip"
        type="button"
        aria-label={ariaLabel}
        onclick={() => {
            void openQueueWindow()
        }}
        use:tooltip={{ contentEl: tooltipContent }}
    >
        <span class="chip-label">{chipLabel}</span>
        <!-- The bar repeats what the aria-label already says as a percentage,
             so screen readers hear it once. -->
        <span class="chip-bar" aria-hidden="true">
            <ProgressBar value={candidate.fraction} size="sm" animated={!candidate.paused} />
        </span>
    </button>

    <div hidden>
        <div bind:this={tooltipContent} class="tooltip-content">{tooltipText}</div>
    </div>
{/if}

<style>
    /* Placement belongs to `StatusCorner`; the chip only describes itself.
       Quiet by default (tertiary text, no fill), the way the hourglass beside
       it is: ambient status shouldn't compete with the file list. */
    .operation-chip {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-xxs) var(--spacing-xs);
        border: none;
        border-radius: var(--radius-sm);
        background: transparent;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
        transition: background var(--transition-base), color var(--transition-base);
    }

    .operation-chip:hover {
        background: var(--color-tint-hover);
        color: var(--color-text-secondary);
    }

    .chip-label {
        white-space: nowrap;
    }

    .chip-bar {
        /* Wide enough to read as progress, narrow enough to stay a chip. No
           token covers it: the readout's own bar minimum is the same 80px, set
           the same way. */
        width: 80px;
        display: flex;
    }

    .tooltip-content {
        /* The tooltip positions once on show and can't see later growth, so the
           line holds a stable width while the percentage and the countdown
           tick. */
        min-width: 220px;
    }
</style>
