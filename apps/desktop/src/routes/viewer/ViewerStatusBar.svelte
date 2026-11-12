<!--
    Bottom status bar for the viewer: file name, line / byte counts, the backend
    badge (in memory / indexed / streaming), an optional word-wrap badge, and the
    keyboard shortcut hint.

    Presentational only. The page owns the underlying session state and passes it
    in as props.
-->

<script lang="ts">
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import Size from '#lib/ui/Size.svelte'
    import type { MediaDimensions, ViewerContentKind } from '#lib/ipc/bindings.js'
    import { isMediaKind, mediaKindLabel, formatMediaDimensions } from './media-view'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { ViewerDisplayMode } from './viewer-view-mode'
    import type { Snippet } from 'svelte'
    import Trans from '#lib/intl/Trans.svelte'
    import ShortcutChip from '#lib/ui/ShortcutChip.svelte'
    import { commandHasShortcut, shortcutAsText } from '#lib/shortcuts/reactive-shortcuts.svelte.js'

    interface Props {
        /** File name shown at the start of the bar. */
        fileName: string
        /** Content kind. Media kinds (image / PDF) show kind + dimensions instead of line / backend info. */
        kind: ViewerContentKind
        mode: ViewerDisplayMode
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
        mode,
        mediaDimensions,
        totalLines,
        totalBytes,
        currentMode,
        isIndexing,
        wordWrap,
        indexingTimeoutSecs,
    }: Props = $props()

    const isMedia = $derived(mode === 'media' && isMediaKind(kind))
    const dimensionsText = $derived(formatMediaDimensions(mediaDimensions))
    // The user's keys, live: wrap and tail are remappable, search is fixed (`Viewer window` in
    // the shortcuts registry), and a key turned off drops its part of the hint.
    const wrapBound = $derived(commandHasShortcut('viewer.toggleWordWrap'))
    const tailBound = $derived(commandHasShortcut('viewer.toggleTail'))
    const searchBound = $derived(commandHasShortcut('viewer.search'))
</script>

<!-- Non-clickable: a chip's deep link opens Settings, which this restricted window can't create. -->
{#snippet wrapChip(_children: Snippet)}<ShortcutChip commandId="viewer.toggleWordWrap" size="sm" clickable={false} />{/snippet}
{#snippet tailChip(_children: Snippet)}<ShortcutChip commandId="viewer.toggleTail" size="sm" clickable={false} />{/snippet}
{#snippet searchChip(_children: Snippet)}<ShortcutChip commandId="viewer.search" size="sm" clickable={false} />{/snippet}

<div class="status-bar" aria-label={tString('viewer.statusBar.ariaLabel')}>
    <span>{fileName}</span>
    {#if isMedia}
        <span class="backend-badge">{mediaKindLabel(kind)}</span>
        {#if dimensionsText}
            <span>{dimensionsText}</span>
        {/if}
        <span><Size bytes={totalBytes} /></span>
    {:else if mode === 'binary' || mode === 'hex'}
        <span><Size bytes={totalBytes} /></span>
        <span class="backend-badge">{tString(mode === 'binary' ? 'viewer.toolbar.viewMode.binary' : 'viewer.toolbar.viewMode.hex')}</span>
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
            <span class="backend-badge" use:tooltip={{ text: tString('viewer.statusBar.badge.wrapTooltip'), shortcut: shortcutAsText('viewer.toggleWordWrap', 'tooltip') }}>{tString('viewer.statusBar.badge.wrap')}</span
            >
        {/if}
    {/if}
    {#if isMedia && kind === 'image'}
        <span class="shortcut-hint">{tString('viewer.statusBar.hint.image')}</span>
    {:else if mode === 'text'}
        <!-- Each part is its own message, so a language orders the key and the word its own way. -->
        <span class="shortcut-hint">
            {#if wrapBound}<span class="hint-part"><Trans key="viewer.statusBar.hint.wrap" snippets={{ key: wrapChip }} /></span>{/if}
            {#if tailBound}<span class="hint-part"><Trans key="viewer.statusBar.hint.tail" snippets={{ key: tailChip }} /></span>{/if}
            {#if searchBound}<span class="hint-part"><Trans key="viewer.statusBar.hint.search" snippets={{ key: searchChip }} /></span>{/if}
        </span>
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

    /* The middle dot between parts is decoration: the second `content` hides it from screen readers
       where WebKit knows the alt-text form, and the first keeps it on an older one. */
    .hint-part + .hint-part::before {
        content: ' · ';
        content: ' · ' / '';
        white-space: pre;
    }
</style>
