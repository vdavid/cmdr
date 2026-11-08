<script lang="ts">
    /**
     * The selected operation's item tree: a `ColumnList` treegrid over the tree source's
     * windows (name with indent, disclosure, and checkbox; size; status), the header's
     * select-all box, the cursor row's story under the tree, and the states a tree can be
     * in before it has rows (loading, an old operation backfilling, the journal's indexes
     * building, an operation still running).
     *
     * Renders only. The tree, its tri-state, and its counts are the backend's
     * (`operation-tree-source.svelte.ts`); the words are `operation-tree-labels.ts`.
     */
    import ColumnList from '#lib/ui/ColumnList.svelte'
    import {
        columnListProps,
        type ColumnListApi,
        type ColumnListColumn,
        type ColumnListRowAria,
    } from '#lib/ui/column-list-types.js'
    import Checkbox from '#lib/ui/Checkbox.svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import Size from '#lib/ui/Size.svelte'
    import Spinner from '#lib/ui/Spinner.svelte'
    import ProgressBar from '#lib/ui/ProgressBar.svelte'
    import { useShortenMiddle } from '#lib/utils/shorten-middle-action.js'
    import { claimKey } from '#lib/shortcuts/claim-key.js'
    import { eventMatchesCommand } from '#lib/shortcuts/index.js'
    import type { CommandId } from '#lib/commands/index.js'
    import { nextTreeCursor, treeCursorKeyOf } from '#lib/ui/tree-cursor.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import type { OpKind, OperationTreeRow } from '#lib/ipc/bindings.js'
    import { treeRowKeyId, type OperationTreeSource } from './operation-tree-source.svelte'
    import { folderSizeText, treeBlankNotice, treeRowStatus, treeRowTips } from './operation-tree-labels'

    interface Props {
        source: OperationTreeSource
        /** The operation's kind, which words what a rollback did to a row. */
        kind: OpKind
        /** ← on a top-level row: focus goes back to the operation list. */
        onLeave?: () => void
    }

    const { source, kind, onLeave }: Props = $props()

    const uid = $props.id()
    let list: ColumnListApi | undefined = $state()

    function tipId(row: OperationTreeRow): string {
        return `${uid}-tip-${treeRowKeyId(row.key)}`
    }

    function rowAria(row: OperationTreeRow): ColumnListRowAria {
        return {
            level: row.level,
            expanded: row.expandable ? row.expanded : undefined,
            setSize: row.setSize,
            posInSet: row.posInSet,
            describedBy: treeRowTips(row, kind).length > 0 ? tipId(row) : undefined,
        }
    }

    /** Move the cursor to row `index` and bring it into view. */
    function moveCursor(index: number): void {
        source.setCursor(index)
        list?.scrollIndexIntoView(index)
    }

    /**
     * The tree's registry keys (`Main window/Operation log`): Space and Insert check the
     * cursor row (Insert then steps down, as in the panes), ⌘A and ⌘⇧A check everything or
     * nothing. Each is claimed even where it does nothing, so Space never scrolls the page.
     * ⌘A and ⌘⇧A also arrive by the menu road (`OperationLogDialog.svelte`'s claims).
     */
    const TREE_COMMANDS: { id: CommandId; run: () => void }[] = [
        {
            id: 'operationLog.toggleItem',
            run: () => {
                const row = source.rows.getRow(source.cursorIndex)
                if (row !== undefined) source.toggleChecked(row)
            },
        },
        {
            id: 'operationLog.toggleItemAndDown',
            run: () => {
                const from = source.cursorIndex
                const row = source.rows.getRow(from)
                if (row !== undefined) source.toggleChecked(row)
                if (from + 1 < source.rows.count) moveCursor(from + 1)
            },
        },
        { id: 'operationLog.selectAll', run: () => source.setAll(true) },
        { id: 'operationLog.deselectAll', run: () => source.setAll(false) },
    ]

    /** ← and → through `tree-cursor.ts`, then the registry keys; the grid already answered ↑ ↓ and the page keys. */
    function onKeydown(event: KeyboardEvent): void {
        const key = treeCursorKeyOf(event)
        if (key === null) {
            if (event.isComposing) return
            const command = TREE_COMMANDS.find(({ id }) => eventMatchesCommand(event, id))
            if (command === undefined) return
            claimKey(event)
            command.run()
            return
        }
        claimKey(event)
        const from = source.cursorIndex
        const row = source.rows.getRow(from)
        const action = nextTreeCursor(key, {
            from,
            row: row === undefined ? undefined : rowAria(row),
            count: source.rows.count,
            parentIndex: source.parentIndexOf(from),
        })
        if (action === null || row === undefined) return
        switch (action.kind) {
            case 'expand':
            case 'collapse':
                source.toggleExpanded(row)
                return
            case 'move':
                moveCursor(action.index)
                return
            case 'leave':
                onLeave?.()
                return
        }
    }

    const cursorRow = $derived(source.rows.getRow(source.cursorIndex))
    const cursorTips = $derived(cursorRow === undefined ? [] : treeRowTips(cursorRow, kind))
    const allBox = $derived(source.allCheckbox)
    const blank = $derived(source.blank)

    const columns: ColumnListColumn<OperationTreeRow>[] = $derived([
        {
            id: 'name',
            label: tString('operationLog.tree.columnName'),
            width: { kind: 'share', minPx: 160 },
            clip: false,
            header: nameHeader,
            cell: nameCell,
        },
        {
            id: 'size',
            label: tString('operationLog.tree.columnSize'),
            width: { kind: 'fixed', px: 90 },
            align: 'end',
            tone: 'secondary',
            cell: sizeCell,
        },
        {
            id: 'status',
            label: tString('operationLog.tree.columnStatus'),
            width: { kind: 'fixed', px: 150 },
            clip: false,
            cell: statusCell,
        },
    ])
</script>

{#snippet nameHeader()}
    <span class="name-header">
        {#if allBox !== 'none'}
            <!-- Out of the Tab order: the grid keeps focus; ⌘A and ⌘⇧A do the same from the keyboard. -->
            <Checkbox
                checked={allBox === 'on'}
                indeterminate={allBox === 'mixed'}
                tabindex={-1}
                ariaLabel={tString('operationLog.tree.selectAll')}
                onCheckedChange={() => {
                    source.toggleAll()
                }}
            />
        {/if}
        <span>{tString('operationLog.tree.columnName')}</span>
    </span>
{/snippet}

{#snippet nameCell({ row }: { row: OperationTreeRow })}
    {@const box = source.checkboxOf(row)}
    {@const isFolder = row.entryType === 'dir'}
    <span class="name" style:padding-left="calc({row.level - 1} * var(--spacing-lg))">
        <!-- A pointer target only: the row (the grid's active descendant) takes → and ←. -->
        <span
            class="disclosure"
            class:is-open={row.expanded}
            aria-hidden="true"
            onclick={() => {
                source.toggleExpanded(row)
            }}
        >
            {#if row.expandable}<Icon name="chevron-right" size={12} />{/if}
        </span>
        {#if box !== 'none'}
            <!-- Out of the Tab order: the grid keeps focus, and a press still checks it. A row
                 inside a folder unit mirrors its folder, greyed (disabled): nothing of its own to pick. -->
            <span class="check">
                <Checkbox
                    checked={box === 'on'}
                    indeterminate={box === 'mixed'}
                    disabled={row.role === 'contents'}
                    tabindex={-1}
                    ariaLabel={row.role === 'contents'
                        ? tString('operationLog.tree.comesBackWithFolder')
                        : tString('operationLog.tree.include', { name: row.label })}
                    onCheckedChange={() => {
                        source.toggleChecked(row)
                    }}
                />
            </span>
        {/if}
        <span class="kind-icon" aria-hidden="true">
            <Icon name={isFolder ? (row.expanded ? 'folder-open' : 'folder') : 'file'} size={14} />
        </span>
        <span
            class="label"
            class:is-contents={row.role === 'contents'}
            class:is-settled={row.role === 'settled'}
            use:useShortenMiddle={{ text: row.label, preferBreakAt: '/', tooltip: row.fullPath }}
        ></span>
    </span>
{/snippet}

{#snippet sizeCell({ row }: { row: OperationTreeRow })}
    {#if row.entryType === 'dir'}{row.tally === null ? '' : folderSizeText(row.tally)}{:else}<Size
            bytes={row.size}
        />{/if}
{/snippet}

{#snippet statusCell({ row }: { row: OperationTreeRow })}
    {@const status = treeRowStatus(row, kind)}
    {@const tips = treeRowTips(row, kind)}
    <span class="status" data-tone={status?.tone}>
        {#if status?.tone === 'rolledBack'}<Icon name="rotate-ccw" size={12} aria-hidden="true" />{/if}
        {#if status?.tone === 'skipped'}<Icon name="circle-alert" size={12} aria-hidden="true" />{/if}
        {#if status}<span class="status-word">{status.word}</span>{/if}
        <!-- The row's accessible description: what the cursor row's line below shows. -->
        {#if tips.length > 0}<span class="sr-only" id={tipId(row)}>{tips.join(' ')}</span>{/if}
    </span>
{/snippet}

<div class="tree">
    {#if blank === null}
        <ColumnList
            bind:this={list}
            {...columnListProps({
                columns,
                rows: source.rows,
                semantics: 'treegrid',
                rowAria,
                ariaLabel: tString('operationLog.tree.label'),
                cursorIndex: source.cursorIndex,
                onCursorChange: (index) => {
                    source.setCursor(index)
                },
                onKeydown,
                rowClass: 'tree-row',
            })}
        />
        <!-- The cursor row's story, for the eye. A screen reader hears the same words as the
             row's description, so this line stays out of the accessibility tree. -->
        <p class="tree-tip" aria-hidden="true">{cursorTips.join(' ')}</p>
    {:else}
        <div class="tree-blank">
            {#if blank.kind === 'backfilling' && source.backfillSlow && blank.total > 0}
                <p>{tString(treeBlankNotice(blank))}</p>
                <ProgressBar
                    value={blank.done / blank.total}
                    ariaLabel={tString(treeBlankNotice(blank))}
                    ariaValuetext={tString('operationLog.tree.backfillProgress', {
                        doneText: formatInteger(blank.done),
                        totalText: formatInteger(blank.total),
                    })}
                />
            {:else if blank.kind === 'backfilling'}
                <!-- Decorative: the line beside it says what's happening. -->
                <Spinner size="md" />
                <p role="status">{tString(treeBlankNotice(blank))}</p>
            {:else if blank.kind === 'loading'}
                <!-- A quick load shows nothing at all; a slow one earns a spinner. -->
                {#if source.slow}<Spinner size="md" label={tString(treeBlankNotice(blank))} />{/if}
            {:else}
                <p role="status">{tString(treeBlankNotice(blank))}</p>
            {/if}
        </div>
    {/if}
    {#if source.togglesCapped}
        <p class="tree-capped" role="status">{tString('operationLog.tree.togglesCapped')}</p>
    {/if}
</div>

<style>
    .tree {
        display: flex;
        flex-direction: column;
        flex: 1 1 auto;
        min-height: 0;
    }

    .name-header {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        /* Lines the header's box up with the rows' boxes, past the disclosure slot. */
        padding-left: calc(var(--spacing-md) + var(--spacing-xs));
    }

    .name {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        min-width: 0;
        width: 100%;
    }

    .disclosure {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        flex-shrink: 0;
        width: var(--spacing-md);
        height: var(--spacing-md);
        color: var(--color-text-tertiary);
        transition: transform var(--transition-base);
    }

    .disclosure.is-open {
        transform: rotate(90deg);
    }

    @media (prefers-reduced-motion: reduce) {
        .disclosure {
            transition: none;
        }
    }

    .check {
        display: inline-flex;
        flex-shrink: 0;
    }

    .kind-icon {
        display: inline-flex;
        flex-shrink: 0;
        color: var(--color-text-tertiary);
    }

    /* A `0` basis: `useShortenMiddle` rewrites the text on resize, so a width that followed
       the text would resize it again in the same round. */
    .label {
        flex: 1 1 0;
        min-width: 0;
        overflow: hidden;
        white-space: nowrap;
    }

    .label.is-contents,
    .label.is-settled {
        color: var(--color-text-secondary);
    }

    .status {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        min-width: 0;
        font-size: var(--font-size-sm);
    }

    .status-word {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .status[data-tone='rolledBack'] {
        color: var(--color-accent-text);
    }

    .status[data-tone='skipped'] {
        color: var(--color-warning-text);
    }

    .status[data-tone='unfinished'],
    .status[data-tone='rollup'] {
        color: var(--color-text-secondary);
    }

    /* One line, always: the cursor moving onto a row with a story doesn't push the bar down. */
    .tree-tip {
        margin: 0;
        padding: var(--spacing-xs) var(--spacing-md);
        min-height: calc(var(--font-size-sm) * var(--font-line-height-normal));
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        border-top: 1px solid var(--color-border-subtle);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .tree-blank {
        flex: 1 1 auto;
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-md);
        padding: var(--spacing-xl);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        text-align: center;
    }

    .tree-blank p {
        margin: 0;
    }

    .tree-capped {
        margin: 0;
        padding: var(--spacing-xs) var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-warning-text);
    }
</style>
