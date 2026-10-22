<script lang="ts">
    /**
     * The lines under a finished operation's full bars, in the progress dialog's
     * done state: how much, how long, and how fast on average; what it skipped
     * because it already existed; and what the OS refused to move to the trash.
     * The wording rules live in `transfer-done-summary.ts`.
     */
    import type { WriteCompleteEvent } from '#lib/ipc/bindings.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import Trans from '#lib/intl/Trans.svelte'
    import Size from '#lib/ui/Size.svelte'
    import { formatDuration, seconds } from '#lib/units/index.js'
    import { composeTrashRefusedToast } from './transfer-complete-toast'
    import { doneFigures, doneSkippedLine } from './transfer-done-summary'

    interface Props {
        event: WriteCompleteEvent
        /** How long the operation ran, or `null` when nobody said when it started. */
        elapsedMs: number | null
    }

    const { event, elapsedMs }: Props = $props()

    const figures = $derived(doneFigures(event, elapsedMs))
    const skipped = $derived(doneSkippedLine(event))
    /** The same sentence the trash's completion toast uses for what stayed behind. */
    const refused = $derived(composeTrashRefusedToast(event.refused))
</script>

{#if figures !== null || skipped !== null || refused !== null}
    <div class="done-summary" role="status">
        {#if figures?.kind === 'rate'}
            <p>
                <Trans
                    key="fileOperations.transferProgress.doneSummaryRate"
                    params={{ duration: formatDuration(seconds(figures.seconds)) }}
                    snippets={{ size: sizeSnippet, rate: rateSnippet }}
                />
            </p>
        {:else if figures?.kind === 'size'}
            <p><Trans key="fileOperations.transferProgress.doneSummarySize" snippets={{ size: sizeSnippet }} /></p>
        {:else if figures?.kind === 'time'}
            <p>
                {tString('fileOperations.transferProgress.doneSummaryTime', {
                    duration: formatDuration(seconds(figures.seconds)),
                })}
            </p>
        {/if}
        {#if skipped !== null}
            <p>{skipped}</p>
        {/if}
        {#if refused !== null}
            <p class="refused">{refused}</p>
        {/if}
    </div>
{/if}

{#snippet sizeSnippet(children: import('svelte').Snippet)}<Size
        bytes={figures !== null && figures.kind !== 'time' ? figures.bytes : 0}
    />{@render children()}{/snippet}
{#snippet rateSnippet(children: import('svelte').Snippet)}<Size
        bytes={figures?.kind === 'rate' ? figures.bytesPerSecond : 0}
        rounded
    />{@render children()}{/snippet}

<style>
    .done-summary {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        margin-bottom: var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .done-summary p {
        margin: 0;
    }

    /* The OS kept some items where they were: worth noticing, so it reads in the
       warning colour, like the toast it repeats. */
    .refused {
        color: var(--color-warning-text);
    }
</style>
