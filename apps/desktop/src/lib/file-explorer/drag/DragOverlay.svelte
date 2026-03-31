<script lang="ts">
    import {
        getOverlayVisible,
        getOverlayX,
        getOverlayY,
        getOverlayFileInfos,
        getOverlayTotalCount,
        getOverlayTargetName,
        getOverlayOperation,
        getOverlayCanDrop,
        buildOverlayNameLines,
        formatActionLine,
    } from './drag-overlay.svelte.js'

    const cursorOffset = 16

    const visible = $derived(getOverlayVisible())
    const x = $derived(getOverlayX())
    const y = $derived(getOverlayY())
    const fileInfos = $derived(getOverlayFileInfos())
    const totalCount = $derived(getOverlayTotalCount())
    const targetName = $derived(getOverlayTargetName())
    const operation = $derived(getOverlayOperation())
    const canDrop = $derived(getOverlayCanDrop())

    const nameLines = $derived(buildOverlayNameLines(fileInfos, totalCount))
    const actionLine = $derived(formatActionLine(operation, targetName, canDrop))
</script>

{#if visible}
    <div
        class="drag-overlay"
        class:cannot-drop={!canDrop}
        style="left: {String(x + cursorOffset)}px; top: {String(y + cursorOffset)}px;"
        aria-hidden="true"
    >
        <div class="name-list">
            {#each nameLines as line, i (i)}
                <div class="name-line" class:is-summary={line.isSummary}>
                    {#if !line.isSummary}
                        {#if line.iconUrl}
                            <img class="name-icon" src={line.iconUrl} alt="" width="12" height="12" />
                        {:else}
                            <span class="name-icon-emoji">{line.isDirectory ? '\uD83D\uDCC1' : '\uD83D\uDCC4'}</span>
                        {/if}
                    {/if}
                    <span class="name-text">{line.text}</span>
                </div>
            {/each}
        </div>
        <div class="action-line" class:is-warning={!canDrop}>
            {actionLine}
        </div>
    </div>
{/if}

<style>
    .drag-overlay {
        position: fixed;
        z-index: var(--z-notification);
        pointer-events: none;
        max-width: 320px;
        padding: var(--spacing-md) var(--spacing-md);
        border-radius: var(--radius-lg);
        background: color-mix(in srgb, var(--color-drag-overlay-bg), transparent 10%);
        color: color-mix(in srgb, white, transparent 8%);
        font-family: var(--font-system), sans-serif;
        font-size: var(--font-size-xs);
        line-height: 1.5;
        backdrop-filter: blur(8px);
        /* Fade edges via CSS mask-image */
        mask-image: linear-gradient(to bottom, transparent 0%, black 8px, black calc(100% - 8px), transparent 100%);
        /* Fade transition */
        opacity: 1;
        transition: opacity 0.15s ease-out;
    }

    .drag-overlay.cannot-drop {
        opacity: 0.5;
    }

    .name-list {
        max-height: 340px;
        overflow: hidden;
    }

    .name-line {
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
        /* stylelint-disable-next-line declaration-property-value-disallowed-list */
        padding: 1px 0;
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .name-icon {
        width: 12px;
        height: 12px;
        object-fit: contain;
        flex-shrink: 0;
    }

    .name-icon-emoji {
        font-size: var(--font-size-xs);
        width: 12px;
        text-align: center;
        flex-shrink: 0;
    }

    .name-text {
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .is-summary {
        color: color-mix(in srgb, white, transparent 45%);
        font-style: italic;
    }

    .action-line {
        margin-top: var(--spacing-sm);
        padding-top: var(--spacing-sm);
        border-top: 1px solid color-mix(in srgb, white, transparent 85%);
        font-weight: 500;
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- forced-dark bg */
        color: var(--color-accent);
    }

    .action-line.is-warning {
        color: color-mix(in srgb, white, transparent 60%);
    }
</style>
