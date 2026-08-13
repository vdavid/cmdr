<script lang="ts">
    import Button from '$lib/ui/Button.svelte'
    import Checkbox from '$lib/ui/Checkbox.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import type { OperationRow } from './operations-store.svelte'
    import { operationTypeIcon } from './operation-icon'
    import { failureReasonFor } from './failure-reason'
    import { transferReadout } from '../progress-readout'
    import TransferProgressReadout from '../TransferProgressReadout.svelte'
    import ScanPhaseBody from '../transfer/ScanPhaseBody.svelte'
    import { stallNoticeFor } from '../transfer/transfer-stall'
    import { bindOperationSession } from '../operation-session/bind-operation-session.svelte'

    interface Props {
        row: OperationRow
        selected: boolean
        onToggleSelect: () => void
        /** Stop showing a retained failure. Not a command on the operation (it
         *  has none left): the page drops the row the backend retained. The only
         *  way one leaves the list — no timer, no window close, no next
         *  operation. */
        onDismiss: () => void
    }

    const { row, selected, onToggleSelect, onDismiss }: Props = $props()

    const snapshot = $derived(row.snapshot)
    const progress = $derived(row.progress)
    const status = $derived(snapshot.status)

    /** This row's looking glass onto the operation, and how it talks back. It
     *  owns the one ETA smoother and the one scan-rate estimator for this
     *  operation in this window, so the numbers here can't drift from what any
     *  other view of it shows — and the Pause / Cancel / Rollback it issues are
     *  the same commands, against the same guards, whatever else is watching. */
    const session = bindOperationSession(() => snapshot.operationId)

    /** A paused op stays in the write-op-state map and reports `is_running:true`,
     *  so the bar-is-moving truth is the SNAPSHOT status, never the progress
     *  event. Only a `running` op shows the live spinner / animated bar. */
    const isRunning = $derived(status === 'running')
    const isPaused = $derived(status === 'paused')
    const isQueued = $derived(status === 'queued')
    const isActionable = $derived(status === 'running' || status === 'paused' || status === 'queued')

    /** The operation stopped and the backend kept its reason. The row is a
     *  record now: nothing to pause, cancel, roll back, or select in bulk, and
     *  the only control left is Dismiss. */
    const isFailed = $derived(status === 'failed')

    /** Why it stopped, in the error dialog's own words. Null on every live row.
     *  `reason.message` is markup from the pipeline (escaped names, size tiers),
     *  so it goes through `{@html}` exactly as the dialog's body does. */
    const reason = $derived(failureReasonFor(snapshot))

    const typeIcon = $derived(operationTypeIcon(snapshot.operationType))

    /** The backend switched this op to undoing what it wrote. The lifecycle
     *  status stays `running` throughout (rollback is an INTENT, not a
     *  lifecycle state), so the live progress phase is the only signal. */
    const isRollingBack = $derived(progress?.phase === 'rolling_back')

    /** The operation is counting, not yet writing: it holds an `operationId`
     *  and its lanes from the moment the user confirmed, and the backend's own
     *  task is waiting on the `TransferDialog` preview. `supportsRollback` is a
     *  promise about the OPERATION, so the phase is what decides which controls
     *  make sense right now. */
    const isScanning = $derived(progress?.phase === 'scanning')

    /** Rollback is offered on exactly the operations the backend can reverse
     *  (`supportsRollback`), and only while there's still something running to
     *  reverse. A scanning operation has written nothing, so it has nothing to
     *  put back. Same affordance the progress dialog shows, same wording. */
    const canRollback = $derived(
        snapshot.supportsRollback && (isRunning || isPaused) && !isRollingBack && !isScanning,
    )

    const label = $derived(tString('queue.row.label', { type: snapshot.operationType }))
    const statusLabel = $derived(
        isRollingBack
            ? tString('fileOperations.transferProgress.titleRollingBack')
            : tString('queue.row.status', { status }),
    )

    /** The dual-bar readout shows once there's something to fill either bar.
     *  Instant ops (rename, create folder/file) emit no `write-progress` at all,
     *  so their rows stay a single line. A scanning row is excluded on purpose:
     *  `filesTotal` means "what the scan concluded", and during the scan there
     *  is no such thing — the counting line below is what it renders instead. */
    const showReadout = $derived(
        (isRunning || isPaused) && !isScanning && progress !== null && (progress.bytesTotal > 0 || progress.filesTotal > 0),
    )

    /** The scan-phase line, on a `queued` row as well as a running one. An
     *  operation admitted behind another on the same lane keeps counting while
     *  it waits, and on a busy lane that's the common case: "Waiting" over a
     *  bare row reads as a hung queue, where "Waiting" over a moving file count
     *  is exactly what's happening. Costs no new strings — `ScanPhaseBody`
     *  resolves the same catalog keys the progress dialog uses. */
    const showScanLine = $derived(isScanning && (isRunning || isPaused || isQueued))

    /** Non-null once the BACKEND reports this transfer has stopped moving for a
     *  reason that isn't deliberate. Same classifier the copy dialog uses, so
     *  the two windows can't disagree about whether something is stuck. */
    const stall = $derived(stallNoticeFor(progress?.activity))

    /** Speed and ETA describe a transfer that's moving, so a paused row shows
     *  neither. The ETA is the session's SMOOTHED value, never
     *  `progress.etaSeconds`: every view of this operation reads that one
     *  number, so no two of them can disagree about how long is left. */
    const byteRate = $derived(isRunning && progress ? transferReadout(progress).bytesPerSecond : null)
    const fileRate = $derived(isRunning ? (progress?.filesPerSecond ?? null) : null)
    const etaSeconds = $derived(isRunning ? (session.current?.etaSecondsDisplay ?? null) : null)

    /** How fast the walk is going, which the backend doesn't measure during a
     *  scan: the session computes it from the same ticks this row renders. */
    const scanRates = $derived(session.current?.scan ?? null)

    const pauseResumeLabel = $derived(
        isPaused ? tString('queue.row.resume') : tString('queue.row.pause'),
    )
    const pauseResumeAria = $derived(
        isPaused ? tString('queue.row.resumeAria') : tString('queue.row.pauseAria'),
    )

    // Source / destination basenames for a compact summary; the full paths sit in
    // the tooltip. Delete / trash have no destination.
    function basename(path: string | null): string {
        if (!path) return ''
        const trimmed = path.replace(/\/+$/, '')
        const idx = trimmed.lastIndexOf('/')
        return idx >= 0 ? trimmed.slice(idx + 1) : trimmed
    }
    const sourceName = $derived(basename(snapshot.source))
    const destName = $derived(basename(snapshot.destination))

    /** Every control here goes through the session, which decides pause-versus-
     *  resume from the snapshot status and refuses a command it has already
     *  sent. Nothing throws, so a click is a plain `void`. The session is null
     *  only for the frame between mount and the first effect, which is before
     *  anyone can click. */
    const commands = $derived(session.current)
</script>

<li class="queue-row" class:selected data-operation-id={snapshot.operationId} data-status={status}>
    <div class="select-cell">
        <!-- The checkbox exists for "Cancel selected", so a settled failure has
             nothing to offer it. -->
        {#if !isFailed}
            <Checkbox checked={selected} onCheckedChange={onToggleSelect} ariaLabel={tString('queue.row.selectAria')} />
        {/if}
    </div>

    <span class="type-cell" aria-hidden="true">
        <Icon name={typeIcon} size={16} />
    </span>

    <div class="summary-row">
        <span class="op-label">{label}</span>
        {#if snapshot.source}
            <span class="path" use:tooltip={{ text: snapshot.source, overflowOnly: true }}>{sourceName}</span>
        {/if}
        {#if snapshot.destination}
            <span class="arrow" aria-hidden="true">&#x2192;</span>
            <span class="path dest" use:tooltip={{ text: snapshot.destination, overflowOnly: true }}>{destName}</span>
        {/if}
    </div>

    <span
        class="status-cell"
        class:running={isRunning}
        class:paused={isPaused}
        class:queued={isQueued}
        class:failed={isFailed}
    >
        {#if isRunning}
            <Spinner size="sm" />
        {:else if isFailed}
            <Icon name="triangle-alert" size={14} />
        {/if}
        <span class="status-text">{statusLabel}</span>
    </span>

    <div class="actions-cell">
        {#if (isRunning || isPaused) && !isScanning}
            <!-- Pause parks between files, so a still-counting operation has
                 nothing to park; the backend declines the flip. -->
            <Button
                variant="secondary"
                size="mini"
                onclick={() => void commands?.togglePause()}
                aria-label={pauseResumeAria}
            >
                <span class="btn-inner">
                    <Icon name={isPaused ? 'play' : 'pause'} size={13} />
                    {pauseResumeLabel}
                </span>
            </Button>
        {/if}
        {#if isActionable}
            <Button
                variant="secondary"
                size="mini"
                onclick={() => void commands?.cancel()}
                aria-label={tString('queue.row.cancelAria')}
            >
                <span class="btn-inner">
                    <Icon name="x" size={13} />
                    {tString('queue.row.cancel')}
                </span>
            </Button>
        {/if}
        {#if isFailed}
            <Button variant="secondary" size="mini" onclick={onDismiss} aria-label={tString('queue.row.dismissAria')}>
                <span class="btn-inner">
                    <Icon name="x" size={13} />
                    {tString('queue.row.dismiss')}
                </span>
            </Button>
        {/if}
        {#if canRollback}
            <!-- Danger, like the progress dialog's: the same click deletes the
                 same files, so it can't read as gentler here. -->
            <span use:tooltip={tString('fileOperations.transferProgress.rollbackTooltip')}>
                <Button variant="danger" size="mini" onclick={() => void commands?.rollback()}>
                    <span class="btn-inner">
                        <Icon name="rotate-ccw" size={13} />
                        {tString('fileOperations.transferProgress.conflictRollback')}
                    </span>
                </Button>
            </span>
        {/if}
    </div>

    <!-- Second line for a failure: why it stopped and what to do about it, in
         full. The queue is the surface that promises completeness, so nothing
         is truncated here (the toast is the one that has to abbreviate).
         The pipeline's own title is left out on purpose: it would read
         "Couldn't copy" right beside the status cell's "Couldn't finish". -->
    {#if reason}
        <div class="reason-cell">
            <!-- eslint-disable-next-line svelte/no-at-html-tags -- markup from the typed error via `failureReasonFor`: escaped names/paths plus size tiers, no user input. Same boundary as `FallbackErrorContent`. -->
            <p class="reason-message selectable">{@html reason.message}</p>
            <p class="reason-suggestion selectable">{reason.suggestion}</p>
        </div>
    {/if}

    <!-- Second line while the operation is still counting: the same scan-phase
         line the progress dialog shows, so the two surfaces can't disagree
         about what a scanning operation looks like. -->
    {#if showScanLine && progress}
        <div class="readout-cell">
            <ScanPhaseBody
                density="compact"
                sourceFolderPath={snapshot.source ?? ''}
                scanFilesFound={progress.filesDone}
                scanDirsFound={progress.dirsDone ?? 0}
                scanBytesFound={progress.bytesDone}
                scanFilesPerSec={scanRates?.filesPerSecond ?? null}
                scanBytesPerSec={scanRates?.bytesPerSecond ?? null}
                scanCurrentDir={progress.currentDir ?? null}
                currentFile={progress.currentFile}
            />
        </div>
    {/if}

    <!-- Second line, spanning everything right of the icon gutter: the same
         dual-bar readout the copy dialog shows, in its compact density. -->
    {#if showReadout && progress}
        <div class="readout-cell">
            <TransferProgressReadout
                density="compact"
                bytesDone={progress.bytesDone}
                bytesTotal={progress.bytesTotal}
                filesDone={progress.filesDone}
                filesTotal={progress.filesTotal}
                bytesPerSecond={byteRate}
                filesPerSecond={fileRate}
                {etaSeconds}
                {stall}
                countKind={snapshot.operationType === 'trash' ? 'items' : 'files'}
            />
        </div>
    {/if}
</li>

<style>
    .queue-row {
        display: grid;
        /* One row of chrome (select, type, summary, status, actions) with the
           readout on a second line under the summary. The bars need the full
           width far more than they need to sit beside the buttons. */
        grid-template-columns: auto auto minmax(0, 1fr) auto auto;
        align-items: center;
        gap: var(--spacing-xs) var(--spacing-sm);
        padding: var(--spacing-sm) var(--spacing-md);
        border-radius: var(--radius-md);
        border: 1px solid transparent;
    }

    .queue-row.selected {
        background: var(--color-accent-subtle);
        border-color: var(--color-border-subtle);
    }

    .select-cell {
        display: flex;
        align-items: center;
    }

    .type-cell {
        display: flex;
        align-items: center;
        color: var(--color-text-secondary);
    }

    .summary-row {
        display: flex;
        align-items: baseline;
        gap: var(--spacing-xs);
        min-width: 0;
        font-size: var(--font-size-sm);
    }

    .readout-cell {
        grid-column: 3 / -1;
        min-width: 0;
    }

    /* The failure's reason takes the readout's line: a settled row has no bars
       to draw, and the prose needs the same full width. An interpolated path
       can be arbitrarily long, so it wraps mid-token rather than pushing the
       row sideways. */
    .reason-cell {
        grid-column: 3 / -1;
        min-width: 0;
        overflow-wrap: anywhere;
    }

    .reason-message {
        margin: 0;
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
    }

    .reason-suggestion {
        margin: var(--spacing-xxs) 0 0;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    /* The reason is the one thing on a row worth copying out of it. */
    .selectable {
        user-select: text;
        -webkit-user-select: text;
        cursor: text;
    }

    .op-label {
        font-weight: 500;
        color: var(--color-text-primary);
        flex-shrink: 0;
    }

    .path {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        color: var(--color-text-secondary);
        min-width: 0;
    }

    .path.dest {
        color: var(--color-accent-text);
    }

    .arrow {
        flex-shrink: 0;
        color: var(--color-text-tertiary);
    }

    .status-cell {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        white-space: nowrap;
    }

    .status-cell.running {
        color: var(--color-accent-text);
    }

    .status-cell.paused {
        color: var(--color-text-secondary);
    }

    /* Queued reads as "waiting", a notch quieter than running/paused. */
    .status-cell.queued {
        color: var(--color-text-tertiary);
    }

    /* A failure is the one row state that earns a colour, and severity follows
       the THING, not the surface: this row names the failure and prints its
       reason underneath, so it's error red, like the toast that does the same.
       The corner chip stays amber because it names nothing. */
    .status-cell.failed {
        color: var(--color-error-text);
    }

    .actions-cell {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .btn-inner {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
    }
</style>
