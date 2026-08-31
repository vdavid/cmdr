<script lang="ts">
    /**
     * The toast after a cancelled transfer's reversal: what came back, and what
     * stayed where it is.
     *
     * A component rather than a plain string because the leftovers are a LIST —
     * one line per typed reason, each naming a file when it applies to one. A
     * single sentence would have to pick one reason and drop the rest, and the
     * dropped one is exactly what the user is standing in front of.
     *
     * Every line arrives already localized (`cancel-rollback-toast.ts`); this
     * file decides only how they stack.
     */
    import type { CancelRollbackReadout } from './cancel-rollback-toast'

    interface Props {
        /** The composed lines. `headline` is absent when the reversal undid
         *  nothing, so the toast opens on the explanation. */
        readout: CancelRollbackReadout
    }

    const { readout }: Props = $props()
</script>

<div class="toast-body">
    {#if readout.headline !== null}
        <span class="headline">{readout.headline}</span>
    {/if}
    {#if readout.leftBehind !== null}
        <span class="left-behind">{readout.leftBehind}</span>
    {/if}
    {#if readout.reasons.length > 0}
        <ul class="reasons">
            {#each readout.reasons as reason (reason)}
                <li>{reason}</li>
            {/each}
        </ul>
    {/if}
</div>

<style>
    .toast-body {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
    }

    .headline {
        color: var(--color-text-primary);
    }

    /* The expectation-setting line and the reasons under it are the explanation,
       not the news, so they sit a step back from the headline. */
    .left-behind,
    .reasons {
        color: var(--color-text-secondary);
    }

    .reasons {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        margin: 0;
        padding-left: var(--spacing-md);
    }

    /* A leftover's own name can be long and unbreakable, and it must not push the
       toast off the edge of the window. */
    .reasons li {
        overflow-wrap: anywhere;
    }
</style>
