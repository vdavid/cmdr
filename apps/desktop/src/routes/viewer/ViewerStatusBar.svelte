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
    import { tString } from '$lib/intl/messages.svelte'

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

<div class="status-bar" aria-label={tString('viewer.statusBar.ariaLabel')}>
    <span>{fileName}</span>
    {#if isMedia}
        <span class="backend-badge">{mediaKindLabel(kind)}</span>
        {#if dimensionsText}
            <span>{dimensionsText}</span>
        {/if}
        <span><Size bytes={totalBytes} /></span>
    {:else}
        {#if totalLines !== null}
            <span>{tString('viewer.statusBar.lineCount', { count: totalLines })}</span>
        {/if}
        <span><Size bytes={totalBytes} /></span>
        {#if currentMode === 'fullLoad'}
        <span
            class="backend-badge"
            use:tooltip={tString('viewer.statusBar.badge.inMemoryTooltip')}
            >{tString('viewer.statusBar.badge.inMemory')}</span
        >
    {:else if currentMode === 'lineIndex'}
        <span
            class="backend-badge"
            use:tooltip={tString('viewer.statusBar.badge.indexedTooltip')}
            >{tString('viewer.statusBar.badge.indexed')}</span
        >
    {:else if isIndexing}
        <span
            class="backend-badge"
            use:tooltip={tString('viewer.statusBar.badge.streamingIndexingTooltip', { seconds: indexingTimeoutSecs })}
            >{tString('viewer.statusBar.badge.streamingIndexing')}</span
        >
        {:else}
            <span
                class="backend-badge"
                use:tooltip={tString('viewer.statusBar.badge.streamingTooltip', { seconds: indexingTimeoutSecs })}
                >{tString('viewer.statusBar.badge.streaming')}</span
            >
        {/if}
        {#if wordWrap}
            <span class="backend-badge" use:tooltip={{ text: tString('viewer.statusBar.badge.wrapTooltip'), shortcut: 'W' }}>{tString('viewer.statusBar.badge.wrap')}</span
            >
        {/if}
    {/if}
    {#if kind === 'image'}
        <span class="shortcut-hint">{tString('viewer.statusBar.hint.image')}</span>
    {:else if kind === 'text'}
        <span class="shortcut-hint">{tString('viewer.statusBar.hint.text')}</span>
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
