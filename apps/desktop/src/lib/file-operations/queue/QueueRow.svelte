<script lang="ts">
    import Button from '$lib/ui/Button.svelte'
    import Checkbox from '$lib/ui/Checkbox.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import type { OperationRow } from './operations-store.svelte'
    import { operationTypeIcon } from './operation-icon'
    import { transferReadout } from '../progress-readout'
    import TransferProgressReadout from '../TransferProgressReadout.svelte'
    import { stallNoticeFor } from '../transfer/transfer-stall'

    interface Props {
        row: OperationRow
        selected: boolean
        onToggleSelect: () => void
        onPauseResume: () => void
        onCancel: () => void
        /** Cancel AND delete what this op has already written. Offered only on
         *  rows the backend says can be reversed (`supportsRollback`). */
        onRollback: () => void
    }

    const { row, selected, onToggleSelect, onPauseResume, onCancel, onRollback }: Props = $props()

    const snapshot = $derived(row.snapshot)
    const progress = $derived(row.progress)
    const status = $derived(snapshot.status)

    /** A paused op stays in the write-op-state map and reports `is_running:true`,
     *  so the bar-is-moving truth is the SNAPSHOT status, never the progress
     *  event. Only a `running` op shows the live spinner / animated bar. */
    const isRunning = $derived(status === 'running')
    const isPaused = $derived(status === 'paused')
    const isQueued = $derived(status === 'queued')
    const isActionable = $derived(status === 'running' || status === 'paused' || status === 'queued')

    const typeIcon = $derived(operationTypeIcon(snapshot.operationType))

    /** The backend switched this op to undoing what it wrote. The lifecycle
     *  status stays `running` throughout (rollback is an INTENT, not a
     *  lifecycle state), so the live progress phase is the only signal. */
    const isRollingBack = $derived(progress?.phase === 'rolling_back')

    /** Rollback is offered on exactly the operations the backend can reverse
     *  (`supportsRollback`), and only while there's still something running to
     *  reverse. Same affordance the progress dialog shows, same wording. */
    const canRollback = $derived(snapshot.supportsRollback && (isRunning || isPaused) && !isRollingBack)

    const label = $derived(tString('queue.row.label', { type: snapshot.operationType }))
    const statusLabel = $derived(
        isRollingBack
            ? tString('fileOperations.transferProgress.titleRollingBack')
            : tString('queue.row.status', { status }),
    )

    /** The dual-bar readout shows once there's something to fill either bar.
     *  Instant ops (rename, create folder/file) emit no `write-progress` at all,
     *  so their rows stay a single line. */
    const showReadout = $derived(
        (isRunning || isPaused) && progress !== null && (progress.bytesTotal > 0 || progress.filesTotal > 0),
    )

    /** Non-null once the BACKEND reports this transfer has stopped moving for a
     *  reason that isn't deliberate. Same classifier the copy dialog uses, so
     *  the two windows can't disagree about whether something is stuck. */
    const stall = $derived(stallNoticeFor(progress?.activity))

    /** Speed and ETA describe a transfer that's moving, so a paused row shows
     *  neither. The ETA is the SMOOTHED value from the store, never
     *  `progress.etaSeconds`: the copy dialog renders the same smoothed number,
     *  so the two windows agree. */
    const byteRate = $derived(isRunning && progress ? transferReadout(progress).bytesPerSecond : null)
    const fileRate = $derived(isRunning ? (progress?.filesPerSecond ?? null) : null)
    const etaSeconds = $derived(isRunning ? row.etaSecondsDisplay : null)

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
</script>

<li class="queue-row" class:selected data-operation-id={snapshot.operationId} data-status={status}>
    <div class="select-cell">
        <Checkbox checked={selected} onCheckedChange={onToggleSelect} ariaLabel={tString('queue.row.selectAria')} />
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

    <span class="status-cell" class:running={isRunning} class:paused={isPaused} class:queued={isQueued}>
        {#if isRunning}
            <Spinner size="sm" />
        {/if}
        <span class="status-text">{statusLabel}</span>
    </span>

    <div class="actions-cell">
        {#if status === 'running' || status === 'paused'}
            <Button variant="secondary" size="mini" onclick={onPauseResume} aria-label={pauseResumeAria}>
                <span class="btn-inner">
                    <Icon name={isPaused ? 'play' : 'pause'} size={13} />
                    {pauseResumeLabel}
                </span>
            </Button>
        {/if}
        {#if isActionable}
            <Button variant="secondary" size="mini" onclick={onCancel} aria-label={tString('queue.row.cancelAria')}>
                <span class="btn-inner">
                    <Icon name="x" size={13} />
                    {tString('queue.row.cancel')}
                </span>
            </Button>
        {/if}
        {#if canRollback}
            <!-- Danger, like the progress dialog's: the same click deletes the
                 same files, so it can't read as gentler here. -->
            <span use:tooltip={tString('fileOperations.transferProgress.rollbackTooltip')}>
                <Button variant="danger" size="mini" onclick={onRollback}>
                    <span class="btn-inner">
                        <Icon name="rotate-ccw" size={13} />
                        {tString('fileOperations.transferProgress.conflictRollback')}
                    </span>
                </Button>
            </span>
        {/if}
    </div>

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
