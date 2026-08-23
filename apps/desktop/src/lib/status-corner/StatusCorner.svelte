<script lang="ts">
    // The main window's top-right status corner: a right-aligned row of ambient
    // status affordances. It owns the corner PLACEMENT (the absolute offsets and
    // the stacking level), so every indicator inside it is a plain inline box
    // that only has to describe itself.
    //
    // The hourglass is the trailing, always-last member (it's the oldest and the
    // most ambient); anything passed as `children` renders to its left.
    //
    // The suggestions indicator is a NAMED member rather than a `children` one,
    // because David placed it between the queue chip and the hourglass and
    // `children` renders left of both. Members stay ordered here, in the corner
    // that owns placement, so the hourglass-stays-last rule is visible in one
    // file rather than spread across the callers.
    //
    // The wake indicator sits immediately LEFT of the suggestions badge, so the
    // two AI glyphs read as one group. That side, not the other: the row is
    // right-aligned and grows leftward, so a member that comes and goes with
    // each wake can't be allowed to shove the persistent badge sideways every
    // time the agent has a look at something.
    import type { Snippet } from 'svelte'
    import WakeIndicator from '$lib/ask-cmdr/WakeIndicator.svelte'
    import IndexingStatusIndicator from '$lib/indexing/IndexingStatusIndicator.svelte'
    import SuggestedOpsIndicator from '$lib/suggested-ops/SuggestedOpsIndicator.svelte'
    import OperationChip from './OperationChip.svelte'

    interface Props {
        /** Rendered left of the indexing hourglass. */
        children?: Snippet
    }

    const { children }: Props = $props()
</script>

<div class="status-corner">
    {@render children?.()}
    <OperationChip />
    <WakeIndicator />
    <SuggestedOpsIndicator />
    <IndexingStatusIndicator />
</div>

<style>
    .status-corner {
        /* No positioned ancestor here (`.main-content` is static), so these
           offsets resolve against the initial containing block — which is
           exactly where the hourglass has always sat. Don't "fix" that by
           making an ancestor relative; it would move the corner. */
        position: absolute;
        top: var(--spacing-sm);
        right: var(--spacing-sm);
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        z-index: var(--z-sticky);
        /* The row is mounted whether or not it has anything to show, so an empty
           (or gap-sized) box must never eat clicks meant for the pane below.
           Children opt back in. */
        pointer-events: none;
    }

    .status-corner > :global(*) {
        pointer-events: auto;
    }
</style>
