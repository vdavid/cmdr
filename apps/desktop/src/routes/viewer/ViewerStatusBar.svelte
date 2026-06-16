<!--
    Bottom status bar for the viewer: file name, line / byte counts, the backend
    badge (in memory / indexed / streaming), an optional word-wrap badge, and the
    keyboard shortcut hint.

    Presentational only. The page owns the underlying session state and passes it
    in as props.
-->

<script lang="ts">
    import { tooltip } from '$lib/tooltip/tooltip'
    import Size from '$lib/ui/Size.svelte'
    import type { MediaDimensions, ViewerContentKind } from '$lib/ipc/bindings'
    import { isMediaKind, mediaKindLabel, formatMediaDimensions } from './media-view'

    interface Props {
        /** File name shown at the start of the bar. */
        fileName: string
        /** Content kind. Media kinds (image / PDF) show kind + dimensions instead of line / backend info. */
        kind: ViewerContentKind
        /** Image pixel dimensions, when known (raster only). Shown only in media mode. */
        mediaDimensions: MediaDimensions | null
        /** Total line count, or `null` when not yet known (streaming, no index). */
        totalLines: number | null
        /** File size in bytes. */
        totalBytes: number
        /** Effective backend mode driving the badge. */
        currentMode: 'fullLoad' | 'byteSeek' | 'lineIndex'
        /** Whether a background index build is currently running. */
        isIndexing: boolean
        /** Whether word wrap is on (adds a "wrap" badge). */
        wordWrap: boolean
        /** Indexing timeout in seconds, surfaced in the streaming-badge tooltips. */
        indexingTimeoutSecs: number
    }

    const {
        fileName,
        kind,
        mediaDimensions,
        totalLines,
        totalBytes,
        currentMode,
        isIndexing,
        wordWrap,
        indexingTimeoutSecs,
    }: Props = $props()

    const isMedia = $derived(isMediaKind(kind))
    const dimensionsText = $derived(formatMediaDimensions(mediaDimensions))
</script>

<div class="status-bar" aria-label="File information">
    <span>{fileName}</span>
    {#if isMedia}
        <span class="backend-badge">{mediaKindLabel(kind)}</span>
        {#if dimensionsText}
            <span>{dimensionsText}</span>
        {/if}
        <span><Size bytes={totalBytes} /></span>
    {:else}
        {#if totalLines !== null}
            <span>{totalLines} {totalLines === 1 ? 'line' : 'lines'}</span>
        {/if}
        <span><Size bytes={totalBytes} /></span>
        {#if currentMode === 'fullLoad'}
        <span
            class="backend-badge"
            use:tooltip={'You have the file entirely in memory. You can quickly scroll to any line.'}
            >in memory</span
        >
    {:else if currentMode === 'lineIndex'}
        <span
            class="backend-badge"
            use:tooltip={'You have the file indexed, so the line numbers are accurate, and you can quickly scroll to any point.'}
            >indexed</span
        >
    {:else if isIndexing}
        <span
            class="backend-badge"
            use:tooltip={`This is a large file in streaming mode. We're building an index in background (max ${String(indexingTimeoutSecs)} sec)... Line numbers are currently approximate.`}
            >streaming, indexing...</span
        >
        {:else}
            <span
                class="backend-badge"
                use:tooltip={`This is a large file in streaming mode. Indexing would've taken longer than ${String(indexingTimeoutSecs)} sec, so we didn't do it. The line numbers are estimates.`}
                >streaming</span
            >
        {/if}
        {#if wordWrap}
            <span class="backend-badge" use:tooltip={{ text: 'Lines wrap at the window edge', shortcut: 'W' }}>wrap</span
            >
        {/if}
    {/if}
    {#if kind === 'image'}
        <span class="shortcut-hint">Click 100% / fit &middot; Scroll zoom &middot; Drag pan</span>
    {:else if kind === 'text'}
        <span class="shortcut-hint">W wrap &middot; F tail &middot; ⌘F search</span>
    {/if}
</div>

<style>
    .status-bar {
        display: flex;
        align-items: center;
        gap: var(--spacing-lg);
        padding: var(--spacing-xs) var(--spacing-sm);
        background: var(--color-bg-secondary);
        border-top: 1px solid var(--color-border-strong);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        flex-shrink: 0;
        /* Opt back in to native selection here so users can copy the file name or line
         * count. The global reset is `user-select: none`, and `.file-content` keeps
         * that for its custom selection model; the status bar is plain chrome. */
        user-select: text;
        -webkit-user-select: text;
    }

    .backend-badge {
        padding: 1px var(--spacing-xs);
        border-radius: var(--radius-sm);
        background: var(--color-bg-tertiary);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
    }

    .shortcut-hint {
        margin-left: auto;
        color: var(--color-text-tertiary);
    }
</style>
