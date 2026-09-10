<script lang="ts">
    /**
     * The viewer's mid-screen "fetching this file" state, shown in place of content while
     * an open has pulled its file into a preview temp for more than a second.
     * Presentational: `createViewerPull` decides when it shows and what it draws.
     */
    import Button from '$lib/ui/Button.svelte'
    import ProgressBar from '$lib/ui/ProgressBar.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { formatByteSize } from '$lib/units'

    interface Props {
        fileName: string
        bytesDone: number
        bytesTotal: number | null
        /** `null` when the size is unknown: a spinner and a running count instead of a bar. */
        fraction: number | null
        /** No new bytes for a while, so the bar stops shimmering. */
        stalled: boolean
        onCancel: () => void
    }

    const { fileName, bytesDone, bytesTotal, fraction, stalled, onCancel }: Props = $props()

    const title = $derived(tString('viewer.pull.title', { fileName }))
</script>

<div class="pull-panel">
    <p class="pull-title">{title}</p>
    {#if fraction !== null && bytesTotal !== null}
        <div class="pull-bar">
            <ProgressBar value={fraction} ariaLabel={title} animated={!stalled} />
        </div>
        <p class="pull-bytes">
            {tString('viewer.pull.progress', {
                doneText: formatByteSize(bytesDone),
                totalText: formatByteSize(bytesTotal),
            })}
        </p>
    {:else}
        <Spinner size="md" />
        <p class="pull-bytes">
            {tString('viewer.pull.progressUnknownTotal', { doneText: formatByteSize(bytesDone) })}
        </p>
    {/if}
    <Button variant="secondary" onclick={onCancel}>{tString('viewer.error.cancel')}</Button>
</div>

<style>
    .pull-panel {
        display: flex;
        flex: 1;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-md);
        padding: var(--spacing-xl);
        text-align: center;
    }

    .pull-title {
        max-width: 100%;
        margin: 0;
        font-size: var(--font-size-md);
        color: var(--color-text-primary);
        overflow-wrap: anywhere;
    }

    .pull-bar {
        display: flex;
        width: min(320px, 80%);
    }

    .pull-bytes {
        margin: 0;
        font-size: var(--font-size-sm);
        font-variant-numeric: tabular-nums;
        color: var(--color-text-secondary);
    }
</style>
