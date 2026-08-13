<script lang="ts">
    import { tooltip } from '$lib/tooltip/tooltip'
    import { getFolderName } from './transfer-dialog-utils'

    interface Props {
        sourcePath: string
        destinationPath: string
        /** Which pane the files are heading for: 'left' reads destination ← source,
         *  'right' reads source → destination. Omitted when there is no pane to be
         *  relative to (a dialog that adopted an operation started elsewhere knows
         *  the two paths and nothing about panes), where the arrow simply means
         *  "from → to" — the same thing the queue row shows. */
        direction?: 'left' | 'right'
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
    {#if direction !== 'left'}
        <span class="folder-name source" use:tooltip={{ text: sourcePath, overflowOnly: true }}>{sourceName}</span>
        <span class="arrow">&#x2192;</span>
        <span class="folder-name destination" use:tooltip={{ text: destinationPath, overflowOnly: true }}
            >{destinationName}</span
        >
    {:else}
        <span class="folder-name destination" use:tooltip={{ text: destinationPath, overflowOnly: true }}
            >{destinationName}</span
        >
        <span class="arrow">&#x2190;</span>
        <span class="folder-name source" use:tooltip={{ text: sourcePath, overflowOnly: true }}>{sourceName}</span>
    {/if}
</div>

<style>
    .direction-indicator {
        display: flex;
        align-items: center;
        justify-content: flex-start;
        gap: var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    /* Content-width so the source → destination group reads left-aligned; each name
       shrinks with an ellipsis when the row is tight, rather than padding out to fill. */
    .folder-name {
        flex: 0 1 auto;
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        font-weight: 500;
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
        font-size: var(--font-size-md);
        color: var(--color-text-tertiary);
    }
</style>
