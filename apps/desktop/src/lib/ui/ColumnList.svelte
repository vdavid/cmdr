<script lang="ts" generics="T">
    /**
     * ColumnList: the house list-of-rows-in-columns, for every dialog that lists files or
     * other records (Search, Selection, and the rename previews). A header and rows on ONE
     * measured `grid-template-columns`, a single cursor that hover and the parent's arrow
     * keys both move, and virtual scrolling over a fixed, token-derived row height, fed by a
     * plain array or a windowed source that pages from the backend.
     *
     * The parent owns the cursor index; this component renders, reports cursor moves
     * (`onCursorChange`: clicks, hover where asked, and a `grid`'s own cursor keys) and
     * right-clicks, works out where a cursor key lands (`cursorTarget`, over `list-cursor.ts`),
     * and scrolls a row into view on request (`scrollIndexIntoView`). DETAILS.md § ColumnList
     * has the contract.
     *
     * Named for what it is: a list (one cursor, listbox semantics by default) laid out in
     * columns. Ark UI has no table or data grid, and its `Listbox` owns selection and keys
     * itself, which is exactly what this leaves to the parent. A `treegrid` is the same grid
     * with each row's place in a tree on it (`rowAria`); the tree stays the consumer's.
     */
    import { untrack } from 'svelte'
    import { getAppLogger } from '#lib/logging/logger.js'
    import { claimKey } from '#lib/shortcuts/claim-key.js'
    import {
        OVERSCAN_ROWS,
        reachesEnd,
        revealScrollTop,
        scrollMapping,
        visibleRange,
        windowPlacement,
    } from './column-list-layout'
    import { createColumnTracks } from './column-list-tracks.svelte'
    import { createDeferredActiveDescendant } from './deferred-active-descendant.svelte'
    import { listCursorMoveOf, nextListCursor } from './list-cursor'
    import type {
        ColumnListCursorKeyOptions,
        ColumnListProps,
        ColumnListRow,
        ColumnListSource,
        ColumnListWindowedSource,
    } from './column-list-types'

    const {
        columns,
        rows,
        rowKey,
        semantics = 'listbox',
        rowAria,
        ariaLabel,
        cursorIndex = -1,
        onCursorChange,
        hoverMovesCursor = false,
        onKeydown,
        onRowClick,
        onRowContextMenu,
        onReachEnd,
        isGroupHeading,
        groupHeading,
        headerClass = '',
        rowClass = '',
        rowHeight,
        virtualized = true,
        id,
    }: ColumnListProps<T> = $props()

    const uid = $props.id()

    const log = getAppLogger('columnList')

    /** A listbox option's id (with the `id` prop), or a grid row's: what `aria-activedescendant` names. */
    function rowId(index: number): string | undefined {
        if (isGrid) return `${id ?? uid}-row-${String(index)}`
        return id === undefined || isTable ? undefined : `${id}-option-${String(index)}`
    }

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

    const isTree = $derived(semantics === 'treegrid')
    // A treegrid is a grid whose rows say where they sit in a tree.
    const isGrid = $derived(semantics === 'grid' || isTree)
    // A grid is a table that takes focus: the same rows, cells, and header.
    const isTable = $derived(semantics === 'table' || isGrid)
    // A content-sized row takes `rowHeight` as its floor rather than its height.
    const rowStyleHeight = $derived(virtualized ? rowHeight : undefined)
    const rowMinHeight = $derived(virtualized ? undefined : rowHeight)
    const arrayRows = $derived(isWindowed(rows) ? null : rows)
    const count = $derived(isWindowed(rows) ? rows.count : rows.length)
    // Past WebKit's layout-height cap, the scroll position is a scaled stand-in for the rows'.
    const mapping = $derived(scrollMapping({ count, rowHeight: rowHeightPx, viewportHeight }))
    const range = $derived(
        virtualized
            ? visibleRange({
                  count,
                  rowHeight: rowHeightPx,
                  scrollTop: scrollTop * mapping.ratio,
                  viewportHeight,
                  overscan: OVERSCAN_ROWS,
              })
            : { start: 0, end: count },
    )
    const drawn = $derived(Array.from({ length: range.end - range.start }, (_, i) => range.start + i))
    // Spacers stand in for the rows outside the window, so the scrollbar spans the whole list.
    const placement = $derived(
        virtualized
            ? windowPlacement({ count, range, rowHeight: rowHeightPx, scrollTop, mapping })
            : { padTop: 0, padBottom: 0, marginTop: 0 },
    )

    function rowAt(index: number): ColumnListRow<T> {
        return isWindowed(rows) ? rows.getRow(index) : rows[index]
    }

    function keyOf(index: number): string | number {
        const row = arrayRows?.[index]
        return row !== undefined && rowKey ? rowKey(row) : index
    }

    /** One drawn block: where it sits and the key it's drawn under, fixed when the block is. */
    interface DrawnItem {
        index: number
        /** The `{#each}` key: the row's own, unless an earlier drawn row already took it. */
        key: string | number
        /** The row's own key (`keyOf`), which is what says whether the block still holds its row. */
        rowKey: string | number
    }

    /**
     * The drawn blocks and the first row key two of them share, if any.
     *
     * Svelte throws on a repeated `{#each}` key, and a production build's throw names
     * neither the list nor the key: the dialog around the list closes and the report can't
     * say why (ERR-XVTB2, Search's results holding one path twice). So a repeat is drawn
     * under a key of its own, which costs that one block its reuse across a re-sort and
     * nothing else, and the effect below says which list and which key.
     */
    const drawnPass = $derived.by<{ items: DrawnItem[]; repeatedKey: string | number | undefined }>(() => {
        // eslint-disable-next-line svelte/prefer-svelte-reactivity -- local to one pass of this derived; nothing reads it after
        const taken = new Set<string | number>()
        let repeatedKey: string | number | undefined
        const items = drawn.map((index): DrawnItem => {
            const own = keyOf(index)
            if (!taken.has(own)) {
                taken.add(own)
                return { index, key: own, rowKey: own }
            }
            repeatedKey ??= own
            // NUL can't be in a path or an id, so this can't land on another row's key.
            return { index, key: `\u0000repeat:${String(index)}`, rowKey: own }
        })
        return { items, repeatedKey }
    })
    const drawnItems = $derived(drawnPass.items)

    /** The last repeated key reported, so a list that keeps its repeat logs it once, not per scroll. */
    let reportedRepeat: string | number | undefined
    $effect(() => {
        const repeated = drawnPass.repeatedKey
        if (repeated === undefined || repeated === reportedRepeat) return
        reportedRepeat = repeated
        log.error('The {list} list got two rows with one key and drew both: {input}', {
            list: ariaLabel,
            input: repeated,
        })
    })

    /**
     * The row a block is drawn for right now, or `undefined` when its index holds nothing (a
     * windowed row not loaded) or holds ANOTHER row (the block is leaving, and a neighbour
     * shifted into its index).
     */
    function liveRowOf(item: DrawnItem): ColumnListRow<T> {
        const row = rowAt(item.index)
        if (row === undefined) return undefined
        return keyOf(item.index) === item.rowKey ? row : undefined
    }

    /**
     * The row each block last drew, by key, for the one moment a block outlives it.
     *
     * A cell may read its row while it tears down (Ark's checkbox reads its props on the way
     * out), and by then the row can be gone from its index or replaced by a neighbour. A block is
     * keyed by its row but found it by INDEX, so that read came back `undefined` or wrong, and
     * the throw took the whole window down to its crash panel (the AI suggestions and bulk
     * rename reviews both did). Cells read through `heldRowOf`, which answers the block's own
     * last row then. Not state: nothing renders from it. Pruned to the drawn keys after each
     * pass, so it holds a viewport's worth at any list size.
     */
    // eslint-disable-next-line svelte/prefer-svelte-reactivity -- bookkeeping for teardown reads; nothing renders from it
    const heldRows = new Map<string | number, T>()

    /**
     * `live` is typed as present because only a branch that drew a row asks; it goes
     * `undefined` only while that branch is on its way out, and then the held row answers.
     */
    function heldRowOf(item: DrawnItem, live: T): T {
        if (live !== undefined) {
            heldRows.set(item.key, live)
            return live
        }
        return heldRows.get(item.key) ?? live
    }

    $effect(() => {
        const keys = new Set(drawnItems.map((item) => item.key))
        untrack(() => {
            for (const key of heldRows.keys()) if (!keys.has(key)) heldRows.delete(key)
        })
    })

    const tracks = createColumnTracks<T>(() => ({
        columns,
        rows: arrayRows,
        viewport: viewportEl,
        containerWidth,
        drawnRows: drawn.length,
    }))

    // Tells a consumer that grows the list that its end is in view. Fires on every move
    // while it holds; the consumer keeps one page in flight.
    $effect(() => {
        const atEnd = reachesEnd({
            count,
            rowHeight: rowHeightPx,
            scrollTop: scrollTop * mapping.ratio,
            viewportHeight,
        })
        if (!atEnd || !onReachEnd) return
        untrack(() => {
            onReachEnd()
        })
    })

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

    /**
     * The id a combobox may name as its `aria-activedescendant` for row `index`: only a drawn
     * data row's, since a virtual list unmounts the rest and an id naming nothing reads as
     * nothing. Reactive: it changes as the window moves.
     */
    export function activeDescendantId(index: number): string | undefined {
        if (index < range.start || index >= range.end) return undefined
        const row = rowAt(index)
        if (row === undefined || isGroupHeading?.(row)) return undefined
        return rowId(index)
    }

    /** How many rows fit on screen, at least one: what a page key moves by (less one). */
    export function visibleRowCount(): number {
        const viewport = viewportEl
        if (!viewport || viewport.clientHeight <= 0) return 1
        if (virtualized && rowHeightPx > 0) return Math.max(1, Math.floor(viewport.clientHeight / rowHeightPx))
        // Content-sized rows differ in height: count the ones fully in view.
        const box = viewport.getBoundingClientRect()
        const inView = [...viewport.querySelectorAll<HTMLElement>('.column-list-row[data-index]')].filter((el) => {
            const rect = el.getBoundingClientRect()
            return rect.height > 0 && rect.top >= box.top && rect.bottom <= box.bottom
        })
        return Math.max(1, inView.length)
    }

    /**
     * Where a cursor key takes the cursor from row `from` (`list-cursor.ts`): a data row,
     * stepping over group headings (a row still loading can take it), or -1 when no row can.
     * `null` for any other key, which stays the caller's.
     */
    export function cursorTarget(
        event: KeyboardEvent,
        from: number,
        options: ColumnListCursorKeyOptions = {},
    ): number | null {
        const move = listCursorMoveOf(event, options)
        if (move === null) return null
        return nextListCursor(move, {
            count,
            from,
            visibleRows: visibleRowCount(),
            wrap: options.wrap,
            isSkipped: (index) => {
                const row = rowAt(index)
                return row !== undefined && Boolean(isGroupHeading?.(row))
            },
        })
    }

    // A grid names its cursor row a frame after it takes focus, or VoiceOver reads only the grid.
    const activeDescendant = createDeferredActiveDescendant()
    const cursorRowId = $derived(isGrid ? activeDescendantId(cursorIndex) : undefined)

    let rootEl: HTMLDivElement | undefined = $state()

    /** A focusable thing inside a cell (a checkbox's hidden input or its label), or null. */
    function cellControlOf(target: EventTarget | null): Element | null {
        if (!(target instanceof Element) || !rootEl) return null
        const control = target.closest('input, button, select, textarea, label, a[href], [tabindex]')
        return control && control !== rootEl && rootEl.contains(control) ? control : null
    }

    /**
     * A grid keeps focus on itself, never on a control in a cell: a focused cell control
     * drops the grid's `aria-activedescendant`, and a focused checkbox would toggle on the
     * same Space the consumer answers, so it'd flip twice. Pressing a control doesn't move
     * focus (the click still lands: the row takes the cursor, the control acts), and a
     * browser that focuses one anyway (a label's click focuses its input in some engines)
     * hands focus straight back.
     */
    function onGridMousedown(event: MouseEvent): void {
        if (!cellControlOf(event.target)) return
        event.preventDefault()
        if (rootEl && document.activeElement !== rootEl) rootEl.focus({ preventScroll: true })
    }

    function onGridFocusin(event: FocusEvent): void {
        if (cellControlOf(event.target)) rootEl?.focus({ preventScroll: true })
    }

    /** A grid's own cursor keys. Anything else (⇧↑, Enter) goes to `onKeydown`, and bubbles on. */
    function onGridKeydown(event: KeyboardEvent): void {
        const next = cursorTarget(event, cursorIndex)
        if (next === null) {
            onKeydown?.(event)
            return
        }
        claimKey(event)
        if (next < 0) return
        onCursorChange?.(next)
        scrollIndexIntoView(next)
    }

    /** Brings row `index` fully into view. The parent calls it after moving the cursor by key. */
    export function scrollIndexIntoView(index: number): void {
        const viewport = viewportEl
        if (!viewport || index < 0 || index >= count) return
        if (virtualized && rowHeightPx > 0 && viewport.clientHeight > 0) {
            const next = revealScrollTop({
                index,
                rowHeight: rowHeightPx,
                scrollTop: viewport.scrollTop,
                viewportHeight: viewport.clientHeight,
                ratio: mapping.ratio,
            })
            if (next === undefined) return
            viewport.scrollTop = next
            scrollTop = next
            return
        }
        // Content-sized rows, or no layout yet (first frame, or jsdom): every drawn row is a
        // real element.
        viewport.querySelector(`[data-index="${String(index)}"]`)?.scrollIntoView({ block: 'nearest' })
    }
</script>

<!-- A grid is the focused element itself, so its keys and its `aria-activedescendant` live
     on this root; a listbox or table never takes focus here. -->
<div
    class="column-list"
    class:is-content-sized={!virtualized}
    bind:this={rootEl}
    role={isTree ? 'treegrid' : isGrid ? 'grid' : isTable ? 'table' : undefined}
    aria-label={isTable ? ariaLabel : undefined}
    aria-rowcount={isTable ? count + 1 : undefined}
    tabindex={isGrid ? 0 : undefined}
    aria-activedescendant={isGrid ? activeDescendant.of(cursorRowId) : undefined}
    onkeydown={isGrid ? onGridKeydown : undefined}
    onmousedown={isGrid ? onGridMousedown : undefined}
    onfocusin={isGrid ? onGridFocusin : undefined}
    onfocus={isGrid ? () => { activeDescendant.tookFocus(cursorRowId) } : undefined}
    onblur={isGrid ? () => { activeDescendant.reset() } : undefined}
>
    <div
        class="column-list-row column-list-probe"
        aria-hidden="true"
        style:height={rowHeight}
        bind:this={probeEl}
    ></div>
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
        id={isTable ? undefined : id}
        role={isTable ? 'rowgroup' : 'listbox'}
        aria-label={isTable ? undefined : ariaLabel}
        onscroll={() => {
            scrollTop = viewportEl?.scrollTop ?? 0
        }}
    >
        <div
            style:padding-top="{placement.padTop}px"
            style:padding-bottom="{placement.padBottom}px"
            style:margin-top={placement.marginTop < 0 ? `${String(placement.marginTop)}px` : undefined}
        >
            <!-- Each block reads its row through its own drawn item, by key: a block on its way out
                 keeps the row it drew (`heldRowOf`), never a neighbour that shifted into its index. -->
            {#each drawnItems as item (item.key)}
                {@const index = item.index}
                {@const live = liveRowOf(item)}
                {#if live === undefined}
                    <!-- A windowed row that hasn't arrived yet: holds its place, says nothing. -->
                    <div
                        class="column-list-row is-placeholder {rowClass}"
                        aria-hidden="true"
                        style:height={rowHeight}
                    ></div>
                {:else if isGroupHeading?.(live)}
                    {@const row = heldRowOf(item, live)}
                    <!-- In a listbox a heading can't be a child (options and groups only), so it
                         stays visual there and each option's own label must carry the context. -->
                    <div
                        class="column-list-row is-group"
                        role={isTable ? 'row' : undefined}
                        aria-rowindex={isTable ? index + 2 : undefined}
                        aria-hidden={isTable ? undefined : 'true'}
                        style:height={rowStyleHeight}
                        style:min-height={rowMinHeight}
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
                    {@const row = heldRowOf(item, live)}
                    {@const isUnderCursor = index === cursorIndex}
                    {@const tree = isTree ? rowAria?.(row) : undefined}
                    <!-- A treegrid's rows say where they sit in the tree, and carry no
                         `aria-selected`: the active descendant marks the cursor, and a row's
                         checkbox its state, so "selected" would contradict an unchecked box. -->
                    <div
                        class="column-list-row is-data {rowClass}"
                        class:animate-track={tracks.animateTracks}
                        class:is-under-cursor={isUnderCursor}
                        style="grid-template-columns: {tracks.gridTemplate};"
                        style:height={rowStyleHeight}
                        style:min-height={rowMinHeight}
                        role={isTable ? 'row' : 'option'}
                        id={rowId(index)}
                        tabindex={isTable ? undefined : -1}
                        aria-selected={isTree || (isTable && !isGrid) ? undefined : isUnderCursor}
                        aria-level={tree?.level}
                        aria-expanded={tree?.expanded}
                        aria-describedby={tree?.describedBy}
                        aria-setsize={isTree ? tree?.setSize : isTable ? undefined : count}
                        aria-posinset={isTree ? tree?.posInSet : isTable ? undefined : index + 1}
                        aria-rowindex={isTable ? index + 2 : undefined}
                        data-index={index}
                        onclick={() => {
                            onCursorChange?.(index)
                            onRowClick?.(index)
                        }}
                        oncontextmenu={(event) => {
                            if (!onRowContextMenu) return
                            event.preventDefault()
                            onRowContextMenu({ index, row, event })
                        }}
                        onmouseenter={hoverMovesCursor ? () => onCursorChange?.(index) : undefined}
                    >
                        {#each columns as column (column.id)}
                            <span
                                class="column-list-cell {column.class ?? ''}"
                                class:is-emphasis={column.emphasis}
                                class:is-end={column.align === 'end'}
                                class:is-unclipped={column.clip === false}
                                class:tone-secondary={column.tone === 'secondary'}
                                class:tone-tertiary={column.tone === 'tertiary'}
                                role={isGrid ? 'gridcell' : isTable ? 'cell' : undefined}
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

    /* The `flex: 1 1 auto` child: it absorbs every bit of room the header leaves. No scroll
       anchoring: a list past the height cap moves its drawn rows within the content as it
       scrolls (`windowPlacement`), and an engine that anchors would scroll back against it. */
    .column-list-viewport {
        flex: 1 1 auto;
        min-height: 0;
        overflow-y: auto;
        overflow-anchor: none;
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

    /* Content-sized rows (`virtualized={false}`): the row height is a floor, and a row grows
       with its content. Rows of varying height lose the rhythm a fixed height gives, so a
       hairline separates them. */
    .column-list.is-content-sized .column-list-row {
        height: auto;
        min-height: calc(var(--font-size-md) * var(--font-line-height-normal) + 2 * var(--spacing-xxs));
    }

    .column-list.is-content-sized .column-list-row.is-data + .column-list-row {
        border-top: 1px solid var(--color-border-subtle);
    }

    /* Single cursor: clicks, the keys, and hover where asked all write the cursor index, so
       there's no separate hover background. */
    .column-list-row.is-under-cursor {
        background: var(--color-accent-subtle);
    }

    /* A grid holds focus itself. The ring sits inside its edge: a consumer's well around the
       list clips what overhangs (Multi-rename's rounded preview). */
    .column-list[role='grid'],
    .column-list[role='treegrid'] {
        outline: none;
    }

    .column-list[role='grid']:focus-visible,
    .column-list[role='treegrid']:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
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
