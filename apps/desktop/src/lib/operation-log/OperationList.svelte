<script lang="ts">
    /**
     * The Operation log's left pane: recent operations, newest first, as a `grid`
     * (`ColumnList`), whose cursor IS the selection the detail pane shows. Each row
     * carries the summary, who and when, a glyph when the operation didn't run to its
     * end, and its rollback state in that state's color. "Load more" sits under the
     * list, in the Tab order between the list and the tree. ↑ ↓ Home End PgUp PgDn are the
     * grid's own; → and Enter (`operationLog.goToItems`) go into the picked one's items.
     */
    import ColumnList from '#lib/ui/ColumnList.svelte'
    import { columnListProps, type ColumnListColumn } from '#lib/ui/column-list-types.js'
    import Button from '#lib/ui/Button.svelte'
    import StatusGlyph from '#lib/ui/StatusGlyph.svelte'
    import type { IconName } from '#lib/ui/icons/icon-map.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { claimKey } from '#lib/shortcuts/claim-key.js'
    import { eventMatchesCommand } from '#lib/shortcuts/index.js'
    import { formatDateTime } from '#lib/settings/reactive-settings.svelte.js'
    import type { ExecutionStatus } from '#lib/ipc/bindings.js'
    import type { OperationRow } from '#lib/tauri-commands/index.js'
    import { executionStatusLabel, initiatorLabel, operationSummary, rollbackStateLabel } from './operation-log-labels'

    interface Props {
        entries: OperationRow[]
        cursorIndex: number
        onCursorChange: (index: number) => void
        hasMore: boolean
        loadingMore: boolean
        onLoadMore: () => void
        /** → or Enter: focus goes into the picked operation's items. */
        onGoToItems?: () => void
    }

    const { entries, cursorIndex, onCursorChange, hasMore, loadingMore, onLoadMore, onGoToItems }: Props = $props()

    /** The grid hands over every key that isn't a cursor key. */
    function onKeydown(event: KeyboardEvent): void {
        if (event.isComposing || !eventMatchesCommand(event, 'operationLog.goToItems')) return
        claimKey(event)
        onGoToItems?.()
    }

    /** The glyph an operation that didn't run to its end carries; a finished one needs none. */
    function statusGlyph(status: ExecutionStatus): IconName | null {
        switch (status) {
            case 'done':
                return null
            case 'queued':
            case 'running':
                return 'clock'
            case 'canceled':
                return 'circle-slash'
            case 'failed':
                return 'circle-alert'
        }
    }

    const columns: ColumnListColumn<OperationRow>[] = $derived([
        {
            id: 'operation',
            label: tString('operationLog.dialog.operations'),
            width: { kind: 'share', minPx: 160 },
            clip: false,
            cell: operationCell,
        },
    ])
</script>

{#snippet operationCell({ row }: { row: OperationRow })}
    {@const glyph = statusGlyph(row.executionStatus)}
    <span class="op-item" data-op-id={row.opId}>
        <span class="op-summary">{operationSummary(row)}</span>
        <span class="op-line">
            <span class="op-meta">
                <span>{initiatorLabel(row.initiator)}</span>
                <span aria-hidden="true">·</span>
                <span>{formatDateTime(row.endedAt ?? row.startedAt)}</span>
            </span>
            {#if glyph !== null}<StatusGlyph name={glyph} label={executionStatusLabel(row.executionStatus)} />{/if}
            <span class="op-state" data-state={row.rollbackState}>{rollbackStateLabel(row.rollbackState)}</span>
        </span>
    </span>
{/snippet}

<div class="op-list">
    <ColumnList
        {...columnListProps({
            columns,
            rows: entries,
            rowKey: (row) => row.opId,
            semantics: 'grid',
            ariaLabel: tString('operationLog.dialog.operations'),
            cursorIndex,
            onCursorChange,
            onKeydown,
            rowHeight: 'calc(var(--font-size-sm) * 1.4 + var(--font-size-xs) * 1.4 + 2 * var(--spacing-sm))',
            rowClass: 'op-row',
        })}
    />
    {#if hasMore}
        <div class="load-more">
            <Button variant="secondary" size="mini" disabled={loadingMore} onclick={onLoadMore}>
                {tString('operationLog.dialog.loadMore')}
            </Button>
        </div>
    {/if}
</div>

<style>
    .op-list {
        display: flex;
        flex-direction: column;
        flex: 1 1 auto;
        min-height: 0;
    }

    .op-item {
        display: flex;
        flex-direction: column;
        justify-content: center;
        gap: var(--spacing-xxs);
        min-width: 0;
        width: 100%;
    }

    .op-summary {
        font-size: var(--font-size-sm);
        font-weight: 600;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .op-line {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        min-width: 0;
    }

    /* Wraps between its parts (who, the dot, when), never inside one, so a long initiator
       ("Agent, avec vos modifications") gives way before the state does. */
    .op-meta {
        display: inline-flex;
        flex-wrap: wrap;
        align-items: center;
        gap: var(--spacing-xs);
        min-width: 0;
        overflow: hidden;
    }

    .op-state {
        margin-left: auto;
        flex-shrink: 0;
        white-space: nowrap;
    }

    .op-state[data-state='rollbackable'] {
        color: var(--color-accent-text);
    }

    .op-state[data-state='partiallyRolledBack'],
    .op-state[data-state='rollingBack'] {
        color: var(--color-warning-text);
    }

    .op-state[data-state='notRollbackable'],
    .op-state[data-state='rolledBack'] {
        color: var(--color-text-tertiary);
    }

    .load-more {
        display: flex;
        justify-content: center;
        padding: var(--spacing-sm) 0;
    }
</style>
