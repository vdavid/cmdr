<script lang="ts">
    import type { SortColumn, SortOrder } from '../types'
    import { commands, SORT_BY_COMMANDS } from '#lib/commands/index.js'
    import { shortcutAsText } from '#lib/shortcuts/reactive-shortcuts.svelte.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import StatusGlyph from '#lib/ui/StatusGlyph.svelte'

    interface Props {
        column: SortColumn
        label: string
        /**
         * The column this pane's rows are currently in, or `null` when they are in
         * no column's order at all: the search-results pane's RANKED state, where
         * the engine's ordering is the answer. Every header stays clickable then;
         * none is active and none draws a caret.
         */
        currentSortColumn: SortColumn | null
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
         * What clicking the ACTIVE column does next, when that is something other
         * than sorting by it. The search-results pane cycles a third click back to
         * the engine's ranked order, so its active header says "Sort by relevance"
         * rather than promising a sort it won't perform. Absent on every pane whose
         * header only ever toggles a direction.
         */
        clearsSortLabel?: string
        /**
         * Why the rows follow another column than this header's (a date the location doesn't
         * record falls back to Modified). Draws an info glyph after the label and leads the
         * tooltip, above the command name. The glyph is `aria-hidden` with no tooltip of its own:
         * the button's `aria-label` replaces its content's name anyway, and two tooltips would
         * stack. The measurer mirrors its width (`HEADER_FALLBACK_GLYPH`).
         */
        fallbackNote?: string
    }

    const {
        column,
        label,
        currentSortColumn,
        currentSortOrder,
        onClick,
        align = 'left',
        isFocused = true,
        clearsSortLabel,
        fallbackNote,
    }: Props = $props()

    const commandId = $derived(SORT_BY_COMMANDS[column])
    const commandName = $derived(commands.find((c) => c.id === commandId)?.name ?? '')
    // Reactive: re-reads when the user rebinds the shortcut. The tooltip action
    // live-updates its content, so a focus flip or rebind mid-hover shows too.
    const shortcut = $derived(isFocused ? shortcutAsText(commandId, 'tooltip') : undefined)

    const isActive = $derived(column === currentSortColumn)
    /**
     * The button's spoken name carries the sort state ("Size, sorted descending"), since
     * `aria-sort` needs a `columnheader` and this is a plain button, then the fallback note
     * when there is one, since the glyph that shows it is `aria-hidden`. It starts with the
     * visible label, so voice control finds it by what's on screen (WCAG 2.5.3).
     */
    const sortState = $derived(
        tString('fileExplorer.columns.sortState', { column: label, order: isActive ? currentSortOrder : 'none' }),
    )
    const ariaLabel = $derived(
        fallbackNote === undefined
            ? sortState
            : tString('fileExplorer.columns.sortStateFallback', { state: sortState, fallback: fallbackNote }),
    )
    /**
     * The tooltip names what the NEXT click does. That is normally this column's
     * sort command; on the active column of a pane that cycles back to an unsorted
     * state, it's `clearsSortLabel`. The shortcut stays either way, because the key
     * runs the same cycle the click does.
     */
    const nextClickText = $derived(isActive && clearsSortLabel !== undefined ? clearsSortLabel : commandName)
    const tooltipText = $derived(fallbackNote === undefined ? nextClickText : `${fallbackNote}\n\n${nextClickText}`)

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
    aria-label={ariaLabel}
    use:tooltip={{ text: tooltipText, shortcut }}
>
    <span class="label">{label}</span>
    {#if fallbackNote !== undefined}
        <span class="fallback-note" aria-hidden="true"><StatusGlyph name="info" label={fallbackNote} tooltip={null} /></span>
    {/if}
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

    .fallback-note {
        display: inline-flex;
        flex-shrink: 0;
    }

    .sort-indicator {
        font-size: 8px;
        flex-shrink: 0;
    }

    .sort-indicator.invisible {
        display: none;
    }
</style>
