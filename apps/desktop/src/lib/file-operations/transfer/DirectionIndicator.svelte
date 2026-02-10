<script lang="ts">
    import { getFolderName } from './transfer-dialog-utils'

    interface Props {
        sourcePath: string
        destinationPath: string
        /** Direction the arrow points: 'left' means copying to left pane, 'right' means copying to right pane */
        direction: 'left' | 'right'
    }

    const { sourcePath, destinationPath, direction }: Props = $props()

    const sourceName = $derived(getFolderName(sourcePath))
    const destinationName = $derived(getFolderName(destinationPath))
</script>

<div class="direction-indicator">
    {#if direction === 'right'}
        <span class="folder-name left-side source" title={sourcePath}>{sourceName}</span>
        <span class="arrow">&#x2192;</span>
        <span class="folder-name right-side destination" title={destinationPath}>{destinationName}</span>
    {:else}
        <span class="folder-name left-side destination" title={destinationPath}>{destinationName}</span>
        <span class="arrow">&#x2190;</span>
        <span class="folder-name right-side source" title={sourcePath}>{sourceName}</span>
    {/if}
</div>

<style>
    .direction-indicator {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: 12px;
        padding: 8px 16px;
        font-size: 13px;
        color: var(--color-text-secondary);
    }

    .folder-name {
        flex: 1;
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        font-weight: 500;
    }

    /* Position-based alignment to keep arrow centered */
    .folder-name.left-side {
        text-align: right;
    }

    .folder-name.right-side {
        text-align: left;
    }

    /* Color based on source/destination */
    .folder-name.source {
        color: var(--color-text-primary);
    }

    .folder-name.destination {
        color: var(--color-accent);
    }

    .arrow {
        flex-shrink: 0;
        font-size: 18px;
        color: var(--color-text-muted);
    }
</style>
