<script lang="ts" generics="T">
    /**
     * ColumnList: the house list-of-rows-in-columns, for every dialog that lists files or
     * other records (Search, Selection, and the rename previews). A header and rows on ONE
     * measured `grid-template-columns`, a single cursor that hover and the parent's arrow
     * keys both move, and virtual scrolling over a fixed, token-derived row height, fed by a
     * plain array or a windowed source that pages from the backend.
     *
     * The parent owns the keyboard and the cursor index; this component renders, reports
     * hover, clicks, and right-clicks, and scrolls a row into view on request
     * (`scrollIndexIntoView`). DETAILS.md § ColumnList has the contract.
     *
     * Named for what it is: a list (one cursor, listbox semantics by default) laid out in
     * columns. Ark UI has no table or data grid, and its `Listbox` owns selection and keys
     * itself, which is exactly what this leaves to the parent.
     */
    import { untrack, type Snippet } from 'svelte'
    import { OVERSCAN_ROWS, revealScrollTop, visibleRange } from './column-list-layout'
    import { createColumnTracks } from './column-list-tracks.svelte'
    import type {
        ColumnListCellContext,
        ColumnListColumn,
        ColumnListRowEvent,
        ColumnListSource,
        ColumnListWindowedSource,
    } from './column-list-types'

    interface Props {
        columns: ColumnListColumn<T>[]
        /** An array (measured columns walk it), or a windowed source that pages on demand. */
        rows: ColumnListSource<T>
        /** Keys an array source's rows so a re-sorted list moves rows instead of rebuilding them. */
        rowKey?: (row: T) => string | number
        /**
         * `listbox` (default): rows are options and the cursor row is `aria-selected`, for
         * lists you move through and act on. `table`: rows and cells, for previews and rows
         * holding their own controls (an option can't contain a checkbox or a text field).
         */
        semantics?: 'listbox' | 'table'
        ariaLabel: string
        /** The row under the cursor, or -1. */
        cursorIndex?: number
        /** The pointer entered a data row. Writing it to `cursorIndex` gives the single cursor. */
        onHover?: (index: number) => void
        onRowClick?: (index: number) => void
        /** Right-click on a data row. The list calls `preventDefault` when this is set. */
        onRowContextMenu?: (payload: ColumnListRowEvent<T>) => void
        /** Marks a row as a group heading (a folder name above its files). */
        isGroupHeading?: (row: T) => boolean
        /** Renders a group heading across every column. */
        groupHeading?: Snippet<[ColumnListCellContext<T>]>
        /** Consumer hook classes on the header and each row (test selectors, contrast audits). */
        headerClass?: string
        rowClass?: string
        /** A CSS length overriding the default row height, for taller rows (thumbnails). */
        rowHeight?: string
    }

    const {
        columns,
        rows,
        rowKey,
        semantics = 'listbox',
        ariaLabel,
        cursorIndex = -1,
        onHover,
        onRowClick,
        onRowContextMenu,
        isGroupHeading,
        groupHeading,
        headerClass = '',
        rowClass = '',
        rowHeight,
    }: Props = $props()

    function isWindowed(source: ColumnListSource<T>): source is ColumnListWindowedSource<T> {
        return !Array.isArray(source)
    }

    let viewportEl: HTMLDivElement | undefined = $state()
    let probeEl: HTMLDivElement | undefined = $state()
    let scrollTop = $state(0)
    let viewportHeight = $state(0)
    let containerWidth = $state(0)
    /** One row's height in pixels, read off the probe (which carries the row height token). */
    let rowHeightPx = $state(0)

    const isTable = $derived(semantics === 'table')
    const arrayRows = $derived(isWindowed(rows) ? null : rows)
    const count = $derived(isWindowed(rows) ? rows.count : rows.length)
    const range = $derived(
        visibleRange({ count, rowHeight: rowHeightPx, scrollTop, viewportHeight, overscan: OVERSCAN_ROWS }),
    )
    const drawn = $derived(Array.from({ length: range.end - range.start }, (_, i) => range.start + i))
    // Spacers stand in for the rows outside the window, so the scrollbar spans the whole list.
    const padTop = $derived(rowHeightPx > 0 ? range.start * rowHeightPx : 0)
    const padBottom = $derived(rowHeightPx > 0 ? (count - range.end) * rowHeightPx : 0)

    function rowAt(index: number): T | undefined {
        return isWindowed(rows) ? rows.getRow(index) : rows[index]
    }

    function keyOf(index: number): string | number {
        const row = arrayRows?.[index]
        return row !== undefined && rowKey ? rowKey(row) : index
    }

    const tracks = createColumnTracks<T>(() => ({
        columns,
        rows: arrayRows,
        viewport: viewportEl,
        containerWidth,
        drawnRows: drawn.length,
    }))

    // Tells a windowed source which rows are on screen, so it can fetch them.
    $effect(() => {
        const { start, end } = range
        if (!isWindowed(rows)) return
        const source = rows
        untrack(() => source.onRangeChange?.({ start, end }))
    })

    $effect(() => {
        const viewport = viewportEl
        const probe = probeEl
        if (!viewport || !probe) return
        const read = (): void => {
            viewportHeight = viewport.clientHeight
            containerWidth = viewport.clientWidth
            rowHeightPx = probe.getBoundingClientRect().height
        }
        read()
        if (typeof ResizeObserver === 'undefined') return
        // The probe resizes with the text size, so a text-size change re-windows on its own.
        const observer = new ResizeObserver(read)
        observer.observe(viewport)
        observer.observe(probe)
        return () => {
            observer.disconnect()
        }
    })

    /** Brings row `index` fully into view. The parent calls it after moving the cursor by key. */
    export function scrollIndexIntoView(index: number): void {
        const viewport = viewportEl
        if (!viewport || index < 0 || index >= count) return
        if (rowHeightPx > 0 && viewport.clientHeight > 0) {
            const next = revealScrollTop({
                index,
                rowHeight: rowHeightPx,
                scrollTop: viewport.scrollTop,
                viewportHeight: viewport.clientHeight,
            })
            if (next === undefined) return
            viewport.scrollTop = next
            scrollTop = next
            return
        }
        // No layout yet (first frame, or jsdom): every drawn row is a real element.
        viewport.querySelector(`[data-index="${String(index)}"]`)?.scrollIntoView({ block: 'nearest' })
    }
</script>

<div
    class="column-list"
    role={isTable ? 'table' : undefined}
    aria-label={isTable ? ariaLabel : undefined}
    aria-rowcount={isTable ? count + 1 : undefined}
>
    <div class="column-list-row column-list-probe" aria-hidden="true" style:height={rowHeight} bind:this={probeEl}></div>
    <div
        class="column-list-header {headerClass}"
        class:animate-track={tracks.animateTracks}
        role={isTable ? 'row' : undefined}
        aria-rowindex={isTable ? 1 : undefined}
        style="grid-template-columns: {tracks.gridTemplate};"
    >
        {#each columns as column (column.id)}
            <span
                class="column-list-label"
                class:is-end={column.align === 'end'}
                role={isTable && column.label ? 'columnheader' : undefined}
                aria-hidden={column.label ? undefined : 'true'}
                data-column={column.id}
            >
                {#if column.labelHidden}
                    <span class="sr-only">{column.label}</span>
                {:else if column.header}
                    {@render column.header()}
                {:else}
                    {column.label}
                {/if}
            </span>
        {/each}
    </div>
    <div
        class="column-list-viewport"
        bind:this={viewportEl}
        role={isTable ? 'rowgroup' : 'listbox'}
        aria-label={isTable ? undefined : ariaLabel}
        onscroll={() => {
            scrollTop = viewportEl?.scrollTop ?? 0
        }}
    >
        <div style:padding-top="{padTop}px" style:padding-bottom="{padBottom}px">
            {#each drawn as index (keyOf(index))}
                {@const row = rowAt(index)}
                {#if row === undefined}
                    <!-- A windowed row that hasn't arrived yet: holds its place, says nothing. -->
                    <div class="column-list-row is-placeholder {rowClass}" aria-hidden="true" style:height={rowHeight}></div>
                {:else if isGroupHeading?.(row)}
                    <!-- In a listbox a heading can't be a child (options and groups only), so it
                         stays visual there and each option's own label must carry the context. -->
                    <div
                        class="column-list-row is-group"
                        role={isTable ? 'row' : undefined}
                        aria-rowindex={isTable ? index + 2 : undefined}
                        aria-hidden={isTable ? undefined : 'true'}
                        style:height={rowHeight}
                        data-index={index}
                    >
                        <span
                            class="column-list-group-cell"
                            role={isTable ? 'rowheader' : undefined}
                            aria-colspan={isTable ? columns.length : undefined}
                        >
                            {@render groupHeading?.({ row, index, isUnderCursor: false })}
                        </span>
                    </div>
                {:else}
                    {@const isUnderCursor = index === cursorIndex}
                    <div
                        class="column-list-row is-data {rowClass}"
                        class:animate-track={tracks.animateTracks}
                        class:is-under-cursor={isUnderCursor}
                        style="grid-template-columns: {tracks.gridTemplate};"
                        style:height={rowHeight}
                        role={isTable ? 'row' : 'option'}
                        tabindex={isTable ? undefined : -1}
                        aria-selected={isTable ? undefined : isUnderCursor}
                        aria-setsize={isTable ? undefined : count}
                        aria-posinset={isTable ? undefined : index + 1}
                        aria-rowindex={isTable ? index + 2 : undefined}
                        data-index={index}
                        onclick={() => onRowClick?.(index)}
                        oncontextmenu={(event) => {
                            if (!onRowContextMenu) return
                            event.preventDefault()
                            onRowContextMenu({ index, row, event })
                        }}
                        onmouseenter={() => onHover?.(index)}
                    >
                        {#each columns as column (column.id)}
                            <span
                                class="column-list-cell {column.class ?? ''}"
                                class:is-emphasis={column.emphasis}
                                class:is-end={column.align === 'end'}
                                class:is-unclipped={column.clip === false}
                                class:tone-secondary={column.tone === 'secondary'}
                                class:tone-tertiary={column.tone === 'tertiary'}
                                role={isTable ? 'cell' : undefined}
                                data-column={column.id}
                            >
                                {@render column.cell({ row, index, isUnderCursor })}
                            </span>
                        {/each}
                    </div>
                {/if}
            {/each}
        </div>
    </div>
</div>

<style>
    /* Header and rows get their `grid-template-columns` as one inline string, so they can't
       resolve the same tracks differently. Every track is measured or fixed, never
       `max-content`: each row is its own grid, so `max-content` would resolve per row from
       that row's data and the columns would drift out of line. */
    .column-list {
        position: relative;
        display: flex;
        flex-direction: column;
        flex: 1 1 auto;
        min-height: 0;
        line-height: var(--font-line-height-normal);
    }

    /* A hidden row, so it takes the row height the script reads in pixels; a text-size change
       resizes it, and its ResizeObserver re-windows the list. */
    .column-list-probe {
        position: absolute;
        visibility: hidden;
        pointer-events: none;
        width: 0;
    }

    .column-list-header,
    .column-list-row.is-data,
    .column-list-row.is-group {
        display: grid;
        column-gap: var(--spacing-md);
        align-items: center;
    }

    /* The tracks ease between widths; `.animate-track` is off for the first layout so
       opening a dialog doesn't animate the columns in. */
    .column-list-header.animate-track,
    .column-list-row.animate-track {
        transition: grid-template-columns var(--transition-slow);
    }

    @media (prefers-reduced-motion: reduce) {
        .column-list-header.animate-track,
        .column-list-row.animate-track {
            transition: none;
        }
    }

    .column-list-header {
        flex-shrink: 0;
        padding: var(--spacing-xs) var(--spacing-md);
        background: var(--color-bg-primary);
        /* The font-size MUST sit on the grid container, not just on the labels: `ch` tracks
           resolve against the element that owns the grid. A header left at the inherited
           root size resolved every `ch` track ~14% wider than the rows' and pushed its
           right-hand side out of line. Keep it in lockstep with `.column-list-row`. */
        font-size: var(--font-size-md);
        border-bottom: 1px solid var(--color-border-subtle);
        user-select: none;
    }

    .column-list-label {
        color: var(--color-text-tertiary);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .column-list-label.is-end {
        text-align: right;
    }

    /* The `flex: 1 1 auto` child: it absorbs every bit of room the header leaves. */
    .column-list-viewport {
        flex: 1 1 auto;
        min-height: 0;
        overflow-y: auto;
        background: var(--color-bg-primary);
    }

    /* The row height every window computation divides by. Fixed, so a virtual list knows
       where row N sits without laying it out: the dialog type's one line of text plus the
       row's vertical padding. A consumer with taller rows overrides it (`rowHeight`, inline
       on every row and the probe). */
    .column-list-row {
        box-sizing: border-box;
        height: calc(var(--font-size-md) * var(--font-line-height-normal) + 2 * var(--spacing-xxs));
        padding: var(--spacing-xxs) var(--spacing-md);
        font-size: var(--font-size-md);
        color: var(--color-text-primary);
    }

    /* Single cursor: hover and the parent's arrow keys both write the cursor index, so
       there's no separate hover background. */
    .column-list-row.is-under-cursor {
        background: var(--color-accent-subtle);
    }

    /* A row that hasn't arrived holds its place; nothing on it answers the pointer. */
    .column-list-row.is-placeholder {
        pointer-events: none;
    }

    .column-list-cell {
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .column-list-cell.is-unclipped {
        overflow: visible;
    }

    .column-list-cell.is-emphasis {
        font-weight: 500;
    }

    .column-list-cell.is-end {
        text-align: right;
    }

    .column-list-cell.tone-secondary {
        color: var(--color-text-secondary);
    }

    .column-list-cell.tone-tertiary {
        color: var(--color-text-tertiary);
    }

    /* Under the cursor the muted tones read at full `--color-text-primary`: secondary and
       tertiary drop below WCAG AA on the lightest accent tints of the cursor background
       (`scripts/check-a11y-contrast/query_dialog_states.go`). Full contrast on the active
       row is also the expected "this row is focused" read. */
    .column-list-row.is-under-cursor .column-list-cell {
        color: var(--color-text-primary);
    }

    .column-list-group-cell {
        grid-column: 1 / -1;
        min-width: 0;
        overflow: hidden;
        white-space: nowrap;
        color: var(--color-text-secondary);
        font-weight: 500;
    }
</style>
