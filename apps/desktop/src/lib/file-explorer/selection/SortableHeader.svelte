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
    }

    const {
        column,
        label,
        currentSortColumn,
        currentSortOrder,
        onClick,
        align = 'left',
        isFocused = true,
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

    .sortable-header:hover {
        color: var(--color-text-primary);
        background: var(--color-bg-tertiary);
    }

    .sortable-header.is-active {
        color: var(--color-accent-text);
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
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- below type scale, sort arrows */
        font-size: 8px;
        flex-shrink: 0;
    }

    .sort-indicator.invisible {
        display: none;
    }
</style>
