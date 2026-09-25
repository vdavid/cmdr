<script lang="ts">
    import { onMount } from 'svelte'
    import { viewerGetBytes } from '$lib/tauri-commands'
    import { tString } from '$lib/intl/messages.svelte'
    import {
        bytesPerRawRow,
        firstRawRow,
        formatRawRow,
        MAX_RAW_SCROLL_HEIGHT,
        RAW_ROW_HEIGHT,
        rawRowCount,
        scrollTopForRawRow,
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
    let fetched = $state<{ startRow: number; bytes: number[] } | null>(null)
    let readError = $state(false)

    const bytesPerRow = $derived(bytesPerRawRow(mode))
    const totalRows = $derived(rawRowCount(totalBytes, mode))
    const firstVisible = $derived(firstRawRow(scrollTop, viewportHeight, totalRows))
    const startRow = $derived(Math.max(0, firstVisible - 16))
    const count = $derived(Math.min(totalRows - startRow, Math.ceil(viewportHeight / RAW_ROW_HEIGHT) + 32))
    const spacerHeight = $derived(Math.min(totalRows * RAW_ROW_HEIGHT, MAX_RAW_SCROLL_HEIGHT))
    const rowsTop = $derived(
        totalRows * RAW_ROW_HEIGHT <= MAX_RAW_SCROLL_HEIGHT
            ? startRow * RAW_ROW_HEIGHT
            : Math.max(0, scrollTop + (startRow - firstVisible) * RAW_ROW_HEIGHT),
    )
    const rows = $derived.by(() => {
        if (fetched?.startRow !== startRow) return []
        const result = []
        for (let i = 0; i < count; i++) {
            const bytes = fetched.bytes.slice(i * bytesPerRow, (i + 1) * bytesPerRow)
            if (bytes.length === 0) break
            result.push(formatRawRow(bytes, (startRow + i) * bytesPerRow, totalBytes, mode))
        }
        return result
    })

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
        const row = Math.floor(initialOffset / bytesPerRow)
        container.scrollTop = scrollTopForRawRow(row, viewportHeight, totalRows)
        scrollTop = container.scrollTop
        container.focus()
        return () => resize.disconnect()
    })

    function handleScroll(): void {
        scrollTop = container.scrollTop
        onOffsetChange(firstRawRow(scrollTop, viewportHeight, totalRows) * bytesPerRow)
    }
</script>

<div
    class="raw-byte-view"
    role="document"
    tabindex="0"
    aria-label={tString('viewer.content.ariaLabel', { fileName })}
    bind:this={container}
    onscroll={handleScroll}
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
