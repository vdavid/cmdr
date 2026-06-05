<script lang="ts">
    import { tooltip } from '$lib/tooltip/tooltip'
    import { getFolderName } from './transfer-dialog-utils'

    interface Props {
        sourcePath: string
        destinationPath: string
        /** Direction the arrow points: 'left' means copying to left pane, 'right' means copying to right pane */
        direction: 'left' | 'right'
        /** Override for the source label. Used when the path basename isn't a
         *  user-meaningful name (an MTP storage root, where the basename is a raw
         *  storage id). Falls back to the path basename when undefined. */
        sourceLabel?: string
        /** Override for the destination label. Same fallback as `sourceLabel`. */
        destinationLabel?: string
    }

    const { sourcePath, destinationPath, direction, sourceLabel, destinationLabel }: Props = $props()

    const sourceName = $derived(sourceLabel ?? getFolderName(sourcePath))
    const destinationName = $derived(destinationLabel ?? getFolderName(destinationPath))
</script>

<div class="direction-indicator">
    {#if direction === 'right'}
        <span class="folder-name left-side source" use:tooltip={{ text: sourcePath, overflowOnly: true }}
            >{sourceName}</span
        >
        <span class="arrow">&#x2192;</span>
        <span class="folder-name right-side destination" use:tooltip={{ text: destinationPath, overflowOnly: true }}
            >{destinationName}</span
        >
    {:else}
        <span class="folder-name left-side destination" use:tooltip={{ text: destinationPath, overflowOnly: true }}
            >{destinationName}</span
        >
        <span class="arrow">&#x2190;</span>
        <span class="folder-name right-side source" use:tooltip={{ text: sourcePath, overflowOnly: true }}
            >{sourceName}</span
        >
    {/if}
</div>

<style>
    .direction-indicator {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-md);
        padding: var(--spacing-sm) var(--spacing-lg);
        font-size: var(--font-size-md);
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
        color: var(--color-accent-text);
    }

    .arrow {
        flex-shrink: 0;
        font-size: var(--font-size-xl);
        color: var(--color-text-tertiary);
    }
</style>
