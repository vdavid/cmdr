<script lang="ts">
    import type { SortColumn, SortOrder } from '../types'
    import { commands, type CommandId } from '$lib/commands'
    import { getFirstShortcutReactive } from '$lib/shortcuts/reactive-shortcuts.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'

    interface Props {
        column: SortColumn
        label: string
        currentSortColumn: SortColumn
        currentSortOrder: SortOrder
        onClick: (column: SortColumn) => void
        /** Alignment: 'left' (default), 'right' for numeric columns */
        align?: 'left' | 'right'
        /** Whether the containing pane is focused. The sort shortcut acts on the focused
         * pane only, so the tooltip includes it only when pressing it would actually
         * sort this pane. Clicking sorts this pane regardless, so the tooltip text
         * itself always shows. */
        isFocused?: boolean
        /**
         * Whether this pane's rows follow its sort at all (`caps.sortsRows`). Off
         * for a pane that renders a fixed row order (the search-results snapshot),
         * where the header becomes a plain LABEL: no button, no tooltip, no active
         * highlight, and no direction caret, because every one of those would name
         * an order the rows are not in or promise a click that can't do anything.
         * The label, its alignment, and its column track are unchanged, so the
         * measured column widths still line up with the data cells below.
         */
        sortable?: boolean
    }

    const {
        column,
        label,
        currentSortColumn,
        currentSortOrder,
        onClick,
        align = 'left',
        isFocused = true,
        sortable = true,
    }: Props = $props()

    const columnToCommandIdMap: Record<SortColumn, CommandId> = {
        name: 'sort.byName',
        extension: 'sort.byExtension',
        size: 'sort.bySize',
        modified: 'sort.byModified',
        created: 'sort.byCreated',
    }

    const commandId = $derived(columnToCommandIdMap[column])
    const commandName = $derived(commands.find((c) => c.id === commandId)?.name ?? '')
    // Reactive: re-reads when the user rebinds the shortcut. The tooltip action
    // live-updates its content, so a focus flip or rebind mid-hover shows too.
    const shortcut = $derived(isFocused ? getFirstShortcutReactive(commandId) : undefined)

    const isActive = $derived(column === currentSortColumn)

    function handleClick() {
        onClick(column)
    }

    function handleKeyDown(e: KeyboardEvent) {
        if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault()
            onClick(column)
        }
    }
</script>

{#if sortable}
    <button
        class="sortable-header"
        class:is-active={isActive}
        class:align-right={align === 'right'}
        onclick={handleClick}
        onkeydown={handleKeyDown}
        type="button"
        use:tooltip={{ text: commandName, shortcut }}
    >
        <span class="label">{label}</span>
        <span class="sort-indicator" class:invisible={!isActive} aria-hidden="true">
            {isActive ? (currentSortOrder === 'ascending' ? '▲' : '▼') : '▲'}
        </span>
    </button>
{:else}
    <!-- The pane's rows don't follow its sort, so this is a column NAME and nothing
         more. Same class (the two parents' grid alignment keys off it, and the label
         has to sit exactly where the sorted one does) on a plain `<span>`: no button
         semantics, no hover affordance (that rule is scoped to `button`), no caret. -->
    <span class="sortable-header" class:align-right={align === 'right'}>
        <span class="label">{label}</span>
    </span>
{/if}

<style>
    .sortable-header {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: 0 var(--spacing-xs);
        /* Negative horizontal margin pulls the button 4px outside the column
           track on each side. Combined with the 4px internal padding, the
           label still lines up with the data cells below, while the hover
           background gets breathing room and adjacent buttons sit closer. */
        margin: 0 calc(-1 * var(--spacing-xs));
        background: transparent;
        border: none;
        border-radius: var(--radius-sm);
        font: inherit;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        white-space: nowrap;
        text-align: left;
        height: 100%;
        transition:
            background-color var(--transition-fast),
            color var(--transition-fast);
    }

    button.sortable-header:hover {
        color: var(--color-text-primary);
        background: var(--color-bg-tertiary);
    }

    .sortable-header.is-active {
        color: var(--color-accent-text);
        font-weight: 500;
    }

    /* Right-aligned (numeric) headers: `row-reverse` puts the first DOM child (the
       label) at the RIGHT edge and the caret to its LEFT, so the caret never sits
       between the label and the column's right edge where the numbers line up.
       `flex-start` is the right-hand edge under `row-reverse`. */
    .sortable-header.align-right {
        flex-direction: row-reverse;
        justify-content: flex-start;
        text-align: right;
    }

    .label {
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .sort-indicator {
        font-size: 8px;
        flex-shrink: 0;
    }

    .sort-indicator.invisible {
        display: none;
    }
</style>
