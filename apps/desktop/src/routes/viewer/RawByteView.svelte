<script lang="ts">
    import { onMount } from 'svelte'
    import { viewerGetBytes } from '$lib/tauri-commands'
    import { tString } from '$lib/intl/messages.svelte'
    import {
        bytesPerRawRow,
        clampRawTopRow,
        firstRawRow,
        isRawScrollScaled,
        MAX_RAW_SCROLL_HEIGHT,
        RAW_ROW_HEIGHT,
        rawChunkRows,
        rawKeyTopRow,
        rawRowCount,
        scrollTopForRawRow,
        wheelRowStep,
        type RawChunk,
        type RawViewMode,
    } from './raw-byte-view'

    interface Props {
        sessionId: string
        fileName: string
        totalBytes: number
        mode: RawViewMode
        initialOffset: number
        onOffsetChange: (offset: number) => void
    }

    const { sessionId, fileName, totalBytes, mode, initialOffset, onOffsetChange }: Props = $props()
    let container: HTMLDivElement
    let viewportHeight = $state(600)
    let scrollTop = $state(0)
    /** The first visible row: the view's one position, which the scrollbar follows when scaled. */
    let topRow = $state(0)
    /**
     * The last chunk that arrived. It stays on screen at its own position until the next one
     * lands, so a scroll shows the rows it already has instead of blanking for a round trip.
     */
    let fetched = $state<RawChunk | null>(null)
    let readError = $state(false)
    /** The `scrollTop` this view last wrote, so its own echo event doesn't move `topRow`. */
    let ownScrollTop: number | null = null
    let wheelCarry = 0

    const bytesPerRow = $derived(bytesPerRawRow(mode))
    const totalRows = $derived(rawRowCount(totalBytes, mode))
    const scaled = $derived(isRawScrollScaled(totalRows))
    const viewportRows = $derived(Math.max(1, Math.floor(viewportHeight / RAW_ROW_HEIGHT)))
    const startRow = $derived(Math.max(0, topRow - 16))
    const count = $derived(Math.min(totalRows - startRow, Math.ceil(viewportHeight / RAW_ROW_HEIGHT) + 32))
    const spacerHeight = $derived(Math.min(totalRows * RAW_ROW_HEIGHT, MAX_RAW_SCROLL_HEIGHT))
    // Scaled, the rows ride on the real `scrollTop` with `topRow` pinned to the viewport's top
    // edge; rows above it may sit at a negative offset, clipped by the scroller.
    const rowsTop = $derived.by(() => {
        if (!fetched) return 0
        return scaled
            ? scrollTop + (fetched.startRow - topRow) * RAW_ROW_HEIGHT
            : fetched.startRow * RAW_ROW_HEIGHT
    })
    const rows = $derived(fetched ? rawChunkRows({ chunk: fetched, mode, totalBytes }) : [])

    $effect(() => {
        const offset = startRow * bytesPerRow
        const byteCount = count * bytesPerRow
        let cancelled = false
        readError = false
        if (byteCount > 0) {
            void viewerGetBytes(sessionId, offset, byteCount)
                .then((bytes) => {
                    if (!cancelled) fetched = { startRow, bytes }
                })
                .catch(() => {
                    if (!cancelled) readError = true
                })
        }
        return () => { cancelled = true }
    })

    onMount(() => {
        const resize = new ResizeObserver(() => { viewportHeight = container.clientHeight })
        resize.observe(container)
        viewportHeight = container.clientHeight
        topRow = clampRawTopRow(Math.floor(initialOffset / bytesPerRow), viewportRows, totalRows)
        syncScrollbar()
        container.focus()
        return () => { resize.disconnect(); }
    })

    function setTopRow(row: number): void {
        topRow = row
        onOffsetChange(row * bytesPerRow)
    }

    /** Moves the (scaled) thumb to `topRow`, remembering the write so its echo is ignored. */
    function syncScrollbar(): void {
        const target = scrollTopForRawRow(topRow, viewportHeight, totalRows)
        container.scrollTop = target
        ownScrollTop = target
        scrollTop = container.scrollTop
    }

    function handleScroll(): void {
        scrollTop = container.scrollTop
        if (ownScrollTop !== null && Math.abs(scrollTop - ownScrollTop) < 1) {
            ownScrollTop = null
            return
        }
        ownScrollTop = null
        setTopRow(firstRawRow(scrollTop, viewportHeight, totalRows))
    }

    // Scaled, one scrollbar pixel spans many rows, so the wheel and keys step whole rows
    // themselves. Unscaled, the browser's own scrolling is already row-precise and smooth.
    function handleWheel(e: WheelEvent): void {
        if (!scaled || e.deltaY === 0) return
        e.preventDefault()
        const step = wheelRowStep({ deltaY: e.deltaY, deltaMode: e.deltaMode, viewportRows, carry: wheelCarry })
        wheelCarry = step.carry
        if (step.rows === 0) return
        setTopRow(clampRawTopRow(topRow + step.rows, viewportRows, totalRows))
        syncScrollbar()
    }

    function handleKeyDown(e: KeyboardEvent): void {
        if (!scaled || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return
        const next = rawKeyTopRow({ key: e.key, topRow, viewportRows, totalRows })
        if (next === null) return
        e.preventDefault()
        setTopRow(next)
        syncScrollbar()
    }
</script>

<div
    class="raw-byte-view"
    role="document"
    tabindex="0"
    aria-label={tString('viewer.content.ariaLabel', { fileName })}
    bind:this={container}
    onscroll={handleScroll}
    onwheel={handleWheel}
    onkeydown={handleKeyDown}
>
    <div class="raw-spacer" style="height: {spacerHeight}px">
        <div class="raw-rows" style="top: {rowsTop}px">
            {#each rows as row (row.offset)}
                <div class="raw-row">
                    <span class="raw-offset">{row.offset}</span>
                    {#if mode === 'hex'}<span class="raw-hex">{row.hex}</span>{/if}
                    <span class="raw-characters">{row.characters}</span>
                </div>
            {/each}
        </div>
    </div>
    {#if readError}<div class="raw-error" role="alert">{tString('viewer.error.readFailed')}</div>{/if}
</div>

<style>
    .raw-byte-view {
        flex: 1;
        min-height: 0;
        overflow: auto;
        background: var(--color-bg-primary);
        font-family: var(--font-mono), monospace;
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        user-select: text;
        -webkit-user-select: text;
    }

    .raw-byte-view:focus {
        outline: none;
    }

    .raw-spacer {
        position: relative;
    }

    .raw-rows {
        position: absolute;
        left: 0;
        width: max-content;
        min-width: 100%;
    }

    .raw-row {
        display: flex;
        gap: var(--spacing-md);
        height: 20px;
        line-height: 20px;
        white-space: pre;
    }

    .raw-offset {
        position: sticky;
        left: 0;
        padding: 0 var(--spacing-sm);
        color: var(--color-text-tertiary);
        background: var(--color-bg-primary);
    }

    .raw-hex,
    .raw-characters {
        white-space: pre;
    }

    .raw-error {
        position: sticky;
        top: 0;
        padding: var(--spacing-sm);
        color: var(--color-text-primary);
        background: var(--color-bg-secondary);
    }
</style>
