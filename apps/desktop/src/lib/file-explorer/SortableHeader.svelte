<script lang="ts">
    import type { SortColumn, SortOrder } from './types'

    interface Props {
        column: SortColumn
        label: string
        currentSortColumn: SortColumn
        currentSortOrder: SortOrder
        onClick: (column: SortColumn) => void
        /** Alignment: 'left' (default), 'right' for numeric columns */
        align?: 'left' | 'right'
    }

    const { column, label, currentSortColumn, currentSortOrder, onClick, align = 'left' }: Props = $props()

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

<button
    class="sortable-header"
    class:is-active={isActive}
    class:align-right={align === 'right'}
    onclick={handleClick}
    onkeydown={handleKeyDown}
    type="button"
>
    <span class="label">{label}</span>
    <span class="sort-indicator" class:invisible={!isActive} aria-hidden="true">
        {isActive ? (currentSortOrder === 'ascending' ? '▲' : '▼') : '▲'}
    </span>
</button>

<style>
    .sortable-header {
        display: flex;
        align-items: center;
        gap: 4px;
        padding: 0 var(--spacing-xs);
        background: transparent;
        border: none;
        cursor: pointer;
        font: inherit;
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        white-space: nowrap;
        text-align: left;
        height: 100%;
    }

    .sortable-header:hover {
        color: var(--color-text-primary);
        background: var(--color-bg-hover);
    }

    .sortable-header.is-active {
        color: var(--color-accent);
        font-weight: 500;
    }

    .sortable-header.align-right {
        justify-content: flex-end;
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
        opacity: 0;
    }
</style>
