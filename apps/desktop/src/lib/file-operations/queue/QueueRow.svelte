<script lang="ts">
    import Button from '$lib/ui/Button.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import ProgressBar from '$lib/ui/ProgressBar.svelte'
    import Size from '$lib/ui/Size.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { formatDuration } from '$lib/tauri-commands'
    import { tooltip } from '$lib/tooltip/tooltip'
    import type { OperationRow } from './operations-store.svelte'
    import { operationTypeIcon } from './operation-icon'

    interface Props {
        row: OperationRow
        selected: boolean
        onToggleSelect: () => void
        onPauseResume: () => void
        onCancel: () => void
    }

    const { row, selected, onToggleSelect, onPauseResume, onCancel }: Props = $props()

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

    const label = $derived(tString('queue.row.label', { type: snapshot.operationType }))
    const statusLabel = $derived(tString('queue.row.status', { status }))

    /** Progress fraction (0..1) from the live event, by bytes when known, else
     *  by file count. Null when there's no progress yet (queued / scanning). */
    const fraction = $derived.by(() => {
        if (!progress) return null
        if (progress.bytesTotal > 0) return progress.bytesDone / progress.bytesTotal
        if (progress.filesTotal > 0) return progress.filesDone / progress.filesTotal
        return null
    })

    const etaText = $derived.by(() => {
        if (!isRunning || progress?.etaSeconds == null) return null
        return tString('queue.row.etaRemaining', { duration: formatDuration(progress.etaSeconds) })
    })

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
    <label class="select-cell">
        <input
            type="checkbox"
            checked={selected}
            onchange={onToggleSelect}
            aria-label={tString('queue.row.selectAria')}
        />
    </label>

    <span class="type-cell" aria-hidden="true">
        <Icon name={typeIcon} size={16} />
    </span>

    <div class="main-cell">
        <div class="summary-row">
            <span class="op-label">{label}</span>
            {#if snapshot.source}
                <span class="path" use:tooltip={{ text: snapshot.source, overflowOnly: true }}>{sourceName}</span>
            {/if}
            {#if snapshot.destination}
                <span class="arrow" aria-hidden="true">&#x2192;</span>
                <span class="path dest" use:tooltip={{ text: snapshot.destination, overflowOnly: true }}
                    >{destName}</span
                >
            {/if}
        </div>

        <div class="progress-row">
            {#if fraction !== null && (isRunning || isPaused)}
                <ProgressBar
                    value={fraction}
                    size="sm"
                    ariaLabel={tString('queue.row.label', { type: snapshot.operationType })}
                />
                {#if progress && progress.bytesTotal > 0}
                    <span class="bytes"><Size bytes={progress.bytesDone} /> / <Size bytes={progress.bytesTotal} /></span>
                {/if}
            {:else if isQueued}
                <span class="queued-hint">
                    <Icon name="hourglass" size={12} />
                </span>
            {/if}
            {#if etaText}
                <span class="eta">{etaText}</span>
            {/if}
        </div>
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
    </div>
</li>

<style>
    .queue-row {
        display: grid;
        grid-template-columns: auto auto minmax(0, 1fr) auto auto;
        align-items: center;
        gap: var(--spacing-sm);
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

    .main-cell {
        min-width: 0;
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
    }

    .summary-row {
        display: flex;
        align-items: baseline;
        gap: var(--spacing-xs);
        min-width: 0;
        font-size: var(--font-size-sm);
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

    .progress-row {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        min-height: 14px;
    }

    .bytes,
    .eta {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        flex-shrink: 0;
        font-variant-numeric: tabular-nums;
    }

    .queued-hint {
        display: flex;
        align-items: center;
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
