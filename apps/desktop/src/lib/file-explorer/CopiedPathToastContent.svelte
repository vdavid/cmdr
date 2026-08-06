<script lang="ts">
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        /** The absolute path that just landed on the clipboard. */
        path: string
    }

    const { path }: Props = $props()
</script>

<div class="content">
    <span class="message">{tString('fileExplorer.clipboard.copiedPath')}</span>
    <span class="path">{path}</span>
</div>

<style>
    .content {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
    }

    .message {
        color: var(--color-text-primary);
    }

    .path {
        font-family: var(--font-mono);
        color: var(--color-text-secondary);
        /* A long path wraps instead of spilling out of the toast. `anywhere` rather
           than `break-word` so a break can land mid-segment too: one 200-character
           folder name carries no break opportunity of its own. */
        overflow-wrap: anywhere;
    }
</style>
