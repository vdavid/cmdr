<script lang="ts">
    import SortableHeader from '../selection/SortableHeader.svelte'
    import type { SortColumn, SortOrder } from '../types'
    import { dateHeaderSortColumn, headerLabelKey, type DateColumn } from '../displayed-date'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import type { MenuAnchor } from '#lib/tauri-commands/index.js'
    import DateSortChevron from './DateSortChevron.svelte'

    interface Props {
        /** The row's grid tracks, mirrored from the data rows so columns line up. */
        gridTemplate: string
        isFocused: boolean
        /** The column the rows are in, or `null` for a pane in no column's order. */
        sortBy: SortColumn | null
        sortOrder: SortOrder
        /** The date the date column shows (`displayedDateColumn` of its rows' sort), which names its header. */
        dateColumn: DateColumn
        /** Why the rows follow Modified though a date sort was requested, or `undefined` when they don't. */
        dateFallbackNote: string | undefined
        /**
         * When on, the Name column carries the full filename and there's no Ext
         * column: the two sort triggers share the Name track instead.
         */
        showExtensionInName: boolean
        /** Whether the date column takes a track (a narrow pane drops it; `full-list-column-fit.ts`). */
        showDateColumn: boolean
        /** Whether the optional Git status column takes a track. */
        gitColumnVisible: boolean
        /** Suppresses the column-width transition for one paint after a nav. */
        skipTransition: boolean
        /**
         * Width the rows' scroll container loses to its vertical scrollbar (0 with
         * macOS overlay scrollbars). The header lives outside that container, so it
         * adds this to its own right padding to stay column-aligned with the rows.
         */
        scrollbarWidth: number
        /** Passed to the ACTIVE column's header. See `SortableHeader`'s prop. */
        clearsSortLabel?: string
        onSortChange?: (column: SortColumn) => void
        /**
         * Opens the Sort by popup (the three dates): at `anchor` from the date chevron, at the
         * pointer (`null`) on a right-click anywhere on this row. Resolves when it closes.
         */
        onSortMenu?: (anchor: MenuAnchor | null) => Promise<void>
    }

    const {
        gridTemplate,
        isFocused,
        sortBy,
        sortOrder,
        dateColumn,
        dateFallbackNote,
        showExtensionInName,
        showDateColumn,
        gitColumnVisible,
        skipTransition,
        scrollbarWidth,
        clearsSortLabel,
        onSortChange,
        onSortMenu,
    }: Props = $props()

    const sort = $derived(onSortChange ?? (() => {}))
    const openSortMenu = (anchor: MenuAnchor | null): Promise<void> => onSortMenu?.(anchor) ?? Promise.resolve()

    function handleContextMenu(e: MouseEvent): void {
        e.preventDefault()
        void openSortMenu(null)
    }
</script>

<!-- The same sort toolbar as Brief view's: a row of sort buttons that ←/→ walk, with ↑ to
     the location bar (`pane/pane-chrome.ts`). The Git label between them is plain text. A
     right-click anywhere on it opens the date Sort by popup at the pointer. -->
<!-- svelte-ignore a11y_interactive_supports_focus -->
<div
    class="header-row"
    oncontextmenu={handleContextMenu}
    role="toolbar"
    aria-label={tString('fileExplorer.list.sortColumnsAriaLabel')}
    data-pane-chrome
    class:no-transition={skipTransition}
    style:grid-template-columns={gridTemplate}
    style:--spacing-scrollbar-width="{scrollbarWidth}px"
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
                {clearsSortLabel}
                label={tString('fileExplorer.columns.name')}
                currentSortColumn={sortBy}
                currentSortOrder={sortOrder}
                onClick={sort}
            />
            <SortableHeader
                column="extension"
                {isFocused}
                {clearsSortLabel}
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
            {clearsSortLabel}
            label={tString('fileExplorer.columns.name')}
            currentSortColumn={sortBy}
            currentSortOrder={sortOrder}
            onClick={sort}
        />
    {/if}
    {#if gitColumnVisible}
        <span class="header-git" use:tooltip={tString('fileExplorer.columns.gitTitle')}>{tString('fileExplorer.columns.git')}</span>
    {/if}
    {#if !showExtensionInName}
        <SortableHeader
            column="extension"
            {isFocused}
            {clearsSortLabel}
            label={tString('fileExplorer.columns.ext')}
            currentSortColumn={sortBy}
            currentSortOrder={sortOrder}
            onClick={sort}
        />
    {/if}
    <SortableHeader
        column="size"
        {isFocused}
        {clearsSortLabel}
        label={tString('fileExplorer.columns.size')}
        align="right"
        currentSortColumn={sortBy}
        currentSortOrder={sortOrder}
        onClick={sort}
    />
    <!-- One date column: it sorts by (and lights up for) the requested date, and its label
         names the date its cells show, which is Modified in a fallback. The Sort by chevron
         shares its track, left of the caret, and comes first in DOM order so ←/→ walk the
         row left to right. A narrow pane drops it with its column; the date sorts stay in
         the row's right-click Sort by popup. -->
    {#if showDateColumn}
        <span class="header-date">
            <DateSortChevron onOpen={openSortMenu} />
            <SortableHeader
                column={dateHeaderSortColumn(sortBy)}
                {isFocused}
                {clearsSortLabel}
                label={tString(headerLabelKey(dateColumn))}
                fallbackNote={dateFallbackNote}
                align="right"
                currentSortColumn={sortBy}
                currentSortOrder={sortOrder}
                onClick={sort}
            />
        </span>
    {/if}
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
        /* This row renders OUTSIDE the rows' scroll container, so a classic scrollbar
           narrows the rows but not the header. `--spacing-scrollbar-width` carries the
           live measurement (see the `scrollbarWidth` prop) and is 0 for macOS overlay
           scrollbars; without it every column drifts right under "Always show scroll
           bars". The background still runs edge to edge, over the scrollbar's column,
           the way a native list header does. */
        padding-right: calc(var(--spacing-md) + var(--spacing-scrollbar-width));
        background: var(--color-bg-secondary);
        height: calc(22px * var(--font-scale) + var(--spacing-xs));
        flex-shrink: 0;
        /* The rows get their clipping from the scroll container's `overflow-x: hidden`;
           this row is outside it, so it clips itself. Without this, dragging the splitter
           narrower than the fixed column tracks spills the labels past the pane edge. */
        overflow-x: hidden;
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

    /* The date track: the Sort by chevron, then the right-aligned date header. The gap
       equals the two buttons' negative margins, so their hover backgrounds meet without
       overlapping. The measurer mirrors chevron + gap (`HEADER_DATE_SORT_CHEVRON`). */
    .header-date {
        display: flex;
        align-items: center;
        justify-content: flex-end;
        min-width: 0;
        gap: var(--spacing-sm);
    }

    .header-date > :global(.sortable-header) {
        flex: 0 1 auto;
        min-width: 0;
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
