<script lang="ts">
    import SortableHeader from '../selection/SortableHeader.svelte'
    import type { SortColumn, SortOrder } from '../types'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        /** The row's grid tracks, mirrored from the data rows so columns line up. */
        gridTemplate: string
        isFocused: boolean
        sortBy: SortColumn
        sortOrder: SortOrder
        /**
         * When on, the Name column carries the full filename and there's no Ext
         * column: the two sort triggers share the Name track instead.
         */
        showExtensionInName: boolean
        /** Whether the optional Git status column takes a track. */
        gitColumnVisible: boolean
        /** Suppresses the column-width transition for one paint after a nav. */
        skipTransition: boolean
        onSortChange?: (column: SortColumn) => void
        /** Measured height, read back by the virtual-scroll math in `FullList`. */
        height?: number
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let {
        gridTemplate,
        isFocused,
        sortBy,
        sortOrder,
        showExtensionInName,
        gitColumnVisible,
        skipTransition,
        onSortChange,
        height = $bindable(0),
    }: Props = $props()
    /* eslint-enable prefer-const */

    const sort = $derived(onSortChange ?? (() => {}))
</script>

<!-- Role/aria intentionally omitted: the header sits inside the listbox, and
     `role="toolbar"` would violate aria-required-children. The sort buttons
     inside remain individually focusable. -->
<div
    class="header-row"
    class:no-transition={skipTransition}
    style="grid-template-columns: {gridTemplate};"
    bind:clientHeight={height}
>
    <span class="header-icon"></span>
    {#if showExtensionInName}
        <!-- Extension rides in the Name column, so there's no Ext column and no
             Ext header of its own. Sort-by-extension stays clickable by splitting
             the single Name-column header into two triggers: "Name" fills the
             space on the left, "Ext" shrinks to its label on the right. Both live
             INSIDE the `1fr` Name track, so the Ext trigger costs the pane no
             width — the measurer still reserves nothing for it. The data cells
             below stay as the full filename in the Name column. -->
        <span class="header-name-ext">
            <SortableHeader
                column="name"
                {isFocused}
                label={tString('fileExplorer.columns.name')}
                currentSortColumn={sortBy}
                currentSortOrder={sortOrder}
                onClick={sort}
            />
            <SortableHeader
                column="extension"
                {isFocused}
                label={tString('fileExplorer.columns.ext')}
                align="right"
                currentSortColumn={sortBy}
                currentSortOrder={sortOrder}
                onClick={sort}
            />
        </span>
    {:else}
        <SortableHeader
            column="name"
            {isFocused}
            label={tString('fileExplorer.columns.name')}
            currentSortColumn={sortBy}
            currentSortOrder={sortOrder}
            onClick={sort}
        />
    {/if}
    {#if gitColumnVisible}
        <span class="header-git" title={tString('fileExplorer.columns.gitTitle')}>{tString('fileExplorer.columns.git')}</span>
    {/if}
    {#if !showExtensionInName}
        <SortableHeader
            column="extension"
            {isFocused}
            label={tString('fileExplorer.columns.ext')}
            currentSortColumn={sortBy}
            currentSortOrder={sortOrder}
            onClick={sort}
        />
    {/if}
    <SortableHeader
        column="size"
        {isFocused}
        label={tString('fileExplorer.columns.size')}
        align="right"
        currentSortColumn={sortBy}
        currentSortOrder={sortOrder}
        onClick={sort}
    />
    <SortableHeader
        column="modified"
        {isFocused}
        label={tString('fileExplorer.columns.modified')}
        align="right"
        currentSortColumn={sortBy}
        currentSortOrder={sortOrder}
        onClick={sort}
    />
</div>

<style>
    .header-row {
        display: grid;
        /* grid-template-columns set via inline style for shrink-wrapped column widths */
        gap: var(--spacing-sm);
        /* Horizontal = the rows' own padding (`--spacing-sm`) PLUS `.listbox-region`'s
           gutter (`--spacing-xs`), so the grid columns land in the same place as the
           rows'. The bottom padding separates the labels from the first row; the height
           grows by the same amount so they keep their position instead of shifting up
           inside an unchanged band. */
        padding: var(--spacing-xxs) var(--spacing-md) var(--spacing-xs);
        background: var(--color-bg-secondary);
        height: calc(22px * var(--font-scale) + var(--spacing-xs));
        flex-shrink: 0;
        /* Sticky inside the scroll container: the header always shares the row
           content width (auto-shrinking when a vertical scrollbar appears) so
           columns line up with the data rows beneath. The `top: 0` pin keeps
           the header in view during vertical scroll. */
        position: sticky;
        top: 0;
        z-index: 1;
        transition: grid-template-columns 300ms ease;
    }

    .header-row.no-transition {
        transition: none;
    }

    @media (prefers-reduced-motion: reduce) {
        .header-row {
            transition: none;
        }
    }

    .header-icon {
        width: var(--spacing-icon-size);
    }

    /* Combined Name + Ext header for the `showExtensionInName` layout: occupies
       the single Name column (`1fr`) and lays its two sort triggers in a row,
       Name filling the space, Ext shrinking to its label on the right. The
       inner `SortableHeader` buttons keep their own hover/active styling, so the
       Ext trigger reads exactly like a real column header while costing the pane
       no column width. */
    .header-name-ext {
        display: flex;
        align-items: center;
        min-width: 0;
        gap: var(--spacing-sm);
    }

    /* The Name trigger takes all spare width; Ext stays just wide enough for
       its label + caret. `min-width: 0` lets the Name button's label ellipsize
       instead of pushing Ext off the edge in a very narrow pane. */
    .header-name-ext > :global(.sortable-header:first-child) {
        flex: 1 1 auto;
        min-width: 0;
    }

    .header-name-ext > :global(.sortable-header:last-child) {
        flex: 0 0 auto;
    }

    .header-git {
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        text-align: center;
        align-self: center;
        white-space: nowrap;
        cursor: default;
    }
</style>
