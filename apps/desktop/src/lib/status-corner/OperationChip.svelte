<script lang="ts">
    // The corner progress chip: an action word and a short bar saying that
    // something is still running after the user backgrounded it. It's a preview,
    // not a queue — one operation, no numbers, no overflow count. The detail
    // lives in its tooltip, and the whole queue is one click away.
    //
    // Which operation, and every rule about when to stay quiet: `operation-chip.ts`.
    import Icon from '$lib/ui/Icon.svelte'
    import ProgressBar from '$lib/ui/ProgressBar.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { formatInteger } from '$lib/intl/number-format'
    import { formatDuration } from '$lib/units'
    import { getMainWindowOperationRows } from '$lib/file-operations/queue/main-window-operations.svelte'
    import { getForegroundFailureId, getForegroundOperationId } from '$lib/file-operations/foreground-operation.svelte'
    import { openQueueWindow } from '$lib/file-operations/queue/queue-window'
    import { bindOperationSession } from '$lib/file-operations/operation-session/bind-operation-session.svelte'
    import { rollbackConfirmVariant, reversalLabelKey } from '$lib/file-operations/reversal-wording'
    import { CHIP_SETTLE_MS, destinationName, pickChipState } from './operation-chip'

    const chipState = $derived(
        pickChipState(getMainWindowOperationRows(), getForegroundOperationId(), getForegroundFailureId()),
    )
    /** The operation being previewed, or null in the failure state. */
    const candidate = $derived(chipState?.kind === 'progress' ? chipState.operation : null)
    /** Derived on its own so the settle effect below depends on WHAT is up, not
     *  on its progress: a `$derived` string doesn't re-notify while its value is
     *  unchanged, so the 200 ms progress ticks don't restart the timer (which
     *  would keep the chip hidden forever). The failure state is one key
     *  whatever the count, so a second failure can't hide the first.  */
    const candidateId = $derived(
        chipState === null
            ? null
            : chipState.kind === 'failure'
              ? 'failure'
              : chipState.operation.row.snapshot.operationId,
    )

    /** The corner's looking glass onto whatever it's previewing, following the
     *  candidate as it changes. The chip is a minimal view: all it takes from
     *  the session is the one smoothed ETA every other view of that operation
     *  reads too. */
    const session = bindOperationSession(() => candidate?.row.snapshot.operationId ?? null)

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

    const visible = $derived(chipState !== null && settledId === candidateId)

    /** The action word: "Copying", "Moving to trash". Only the running chip
     *  shows it; `chipLabel` is what any surface describing the chip leads with.
     *
     *  A REVERSAL is named by what it will do to the files ("Putting files
     *  back"), ❌ never by the operation type it runs as: undoing a copy is
     *  journaled as a delete, and a corner chip reading "Deleting" over an undo
     *  the person just asked for is the one thing this wording exists to
     *  prevent. Same variant the confirmation was worded from, and the same
     *  string the queue row shows, so no two surfaces can disagree. */
    const reversalVariant = $derived(
        candidate === null || candidate.row.snapshot.reverses === null
            ? null
            : rollbackConfirmVariant(candidate.row.snapshot.reverses),
    )
    const verb = $derived(
        candidate === null
            ? ''
            : reversalVariant !== null
              ? tString(reversalLabelKey(reversalVariant))
              : tString('queue.row.label', { type: candidate.row.snapshot.operationType }),
    )
    const pausedWord = $derived(tString('queue.row.status', { status: 'paused' }))
    /** The same "Couldn't finish" the failed queue row shows, so the two
     *  surfaces can't describe the same thing with different words. */
    const failedWord = $derived(tString('queue.row.status', { status: 'failed' }))
    const chipLabel = $derived(
        chipState?.kind === 'failure' ? failedWord : candidate?.paused === true ? pausedWord : verb,
    )
    const percentText = $derived(formatInteger(candidate?.percent ?? 0))

    /** The operation hasn't started writing, so there is no fraction to draw.
     *  The corner says what it honestly knows: this is happening, and it's
     *  counting. Same wording the dialog and the delete confirmation use for
     *  the same moment. */
    const isScanning = $derived(candidate?.scanning === true)
    const scanningText = $derived(tString('fileOperations.shared.scanningTooltip'))

    /** One sentence for both the tooltip and the spoken label in the failure
     *  state: the count, and the promise that clicking opens the queue. */
    const failedText = $derived(
        chipState?.kind !== 'failure'
            ? ''
            : tString('queue.chip.failed', { count: chipState.count, countText: formatInteger(chipState.count) }),
    )

    /** The tooltip's trailing fact: how long is left. Absent only while the
     *  backend's estimate is still warming up. It SURVIVES a pause, like every
     *  other surface showing this operation: the seconds a person spends
     *  deciding are kept out of the backend's rate window, so the countdown is
     *  still what remains once they resume. */
    const detail = $derived.by(() => {
        if (candidate === null) return null
        // The session's SMOOTHED ETA, never `progress.etaSeconds`: the queue
        // window renders that same number for that same operation, and two
        // surfaces disagreeing about one operation is a bug we've shipped once.
        const eta = session.current?.etaSecondsDisplay ?? null
        if (eta === null) return null
        return tString('fileOperations.transferProgress.etaRemaining', { duration: formatDuration(eta) })
    })

    const tooltipText = $derived.by(() => {
        if (chipState?.kind === 'failure') return failedText
        if (candidate === null) return ''
        // ❌ Not the progress tooltip: its "· 0%" clause would be a percentage
        // that cannot move, for as long as the walk takes.
        if (isScanning) return scanningText
        const count = candidate.row.progress?.filesTotal ?? 0
        const destination = destinationName(candidate.row.snapshot.destination)
        // `chipLabel`, never `verb`: hovering a chip that reads "Paused" must
        // not open a line claiming the copy is running right now. English's
        // aspect-free "Copying" hid that; zh's 正在拷贝 says it outright.
        return tString('queue.chip.tooltip', {
            label: chipLabel,
            count,
            countText: formatInteger(count),
            hasDestination: destination === '' ? 'no' : 'yes',
            destination,
            percentText,
            hasDetail: detail === null ? 'no' : 'yes',
            detail: detail ?? '',
        })
    })

    /** The scan state gets its OWN spoken label, not the tooltip's "Scanning…":
     *  it drops the percentage (the one dishonest part) but keeps what the
     *  sighted chip has — the visible verb, which voice control needs to press
     *  the chip by name (WCAG 2.5.3), and the promise that pressing it opens
     *  the queue, which is the chip's whole affordance. */
    const ariaLabel = $derived(
        chipState?.kind === 'failure'
            ? failedText
            : isScanning
              ? tString('queue.chip.scanningAriaLabel', { label: chipLabel })
              : tString('queue.chip.ariaLabel', { label: chipLabel, percentText }),
    )

    /** The tooltip action ADOPTS this element, and an adopted element keeps its
     *  own `hidden` attribute — so it's the inner div that's bound here, never
     *  the hidden wrapper (an empty tooltip is what you get otherwise). */
    let tooltipContent = $state<HTMLDivElement>()
</script>

{#if visible && chipState}
    <button
        class="operation-chip"
        class:failed={chipState.kind === 'failure'}
        type="button"
        aria-label={ariaLabel}
        onclick={() => {
            void openQueueWindow()
        }}
        use:tooltip={{ contentEl: tooltipContent }}
    >
        {#if chipState.kind === 'failure'}
            <!-- No bar: there's no progress left to describe, and the glyph is
                 what makes the corner readable at a glance. -->
            <Icon name="triangle-alert" size={13} />
        {/if}
        <span class="chip-label">{chipLabel}</span>
        {#if chipState.kind === 'progress'}
            {#if chipState.operation.scanning}
                <!-- Indeterminate: the totals are what the scan is looking for,
                     so a bar would sit at 0% for the whole walk. -->
                <span class="chip-spinner" aria-hidden="true">
                    <Spinner size="sm" />
                </span>
            {:else}
                <!-- The bar repeats what the aria-label already says as a percentage,
                     so screen readers hear it once. -->
                <span class="chip-bar" aria-hidden="true">
                    <ProgressBar value={chipState.operation.fraction} size="sm" animated={!chipState.operation.paused} />
                </span>
            {/if}
        {/if}
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

    .chip-spinner {
        display: inline-flex;
        align-items: center;
    }

    .operation-chip:hover {
        background: var(--color-tint-hover);
        color: var(--color-text-secondary);
    }

    /* A failure earns colour where live progress doesn't: it's the one thing in
       the corner the user has to notice. Warning, not error, because severity
       follows the THING: the chip only points, naming neither the operation nor
       the reason. The toast and the failed queue row, which do both, are red. */
    .operation-chip.failed,
    .operation-chip.failed:hover {
        color: var(--color-warning-text);
    }

    .chip-label {
        white-space: nowrap;
        /* Capped, or a long localized verb grows the chip leftward across the
           pane: German's "Wird in den Papierkorb bewegt" is 29 characters
           against English's 15, and Dutch and French aren't far behind. 12em
           clears every English label with room to spare and ellipsizes the
           outliers, whose full text the tooltip carries. `em`, not a px token,
           so the cap follows the app's text-size setting. */
        max-width: 12em;
        overflow: hidden;
        text-overflow: ellipsis;
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
