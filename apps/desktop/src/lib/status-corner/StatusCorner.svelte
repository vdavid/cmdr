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
    //
    // The FDA badge is the least transient member of all (a missing grant stays
    // missing until someone goes and gives it), so it takes the one slot left of
    // the hourglass and everything that comes and goes renders further left.
    import type { Snippet } from 'svelte'
    import WakeIndicator from '#lib/ask-cmdr/WakeIndicator.svelte'
    import IndexingStatusIndicator from '#lib/indexing/IndexingStatusIndicator.svelte'
    import FdaBadge from '#lib/onboarding/FdaBadge.svelte'
    import SuggestedOpsIndicator from '#lib/suggested-ops/SuggestedOpsIndicator.svelte'
    import OperationChip from './OperationChip.svelte'
    import { publishCornerBottom } from './toast-clearance'

    interface Props {
        /** Rendered left of the indexing hourglass. */
        children?: Snippet
        /**
         * Opens onboarding at its Full Disk Access step, for `FdaBadge`.
         *
         * The one prop the corner forwards to a member, and it's here because the wizard's
         * visibility is `routes/(main)/+page.svelte` `$state`: only the page can mount it.
         * Required, so a corner mounted without it is a compile error rather than a badge
         * that quietly does nothing when clicked.
         */
        onOpenOnboarding: () => void
    }

    const { children, onOpenOnboarding }: Props = $props()

    // The top toast stack starts below this row's real bottom, so a toast never covers a member (`toast-clearance.ts`).
    let corner = $state<HTMLDivElement>()
    $effect(() => (corner ? publishCornerBottom(corner) : undefined))
</script>

<div class="status-corner" bind:this={corner}>
    {@render children?.()}
    <OperationChip />
    <WakeIndicator />
    <SuggestedOpsIndicator />
    <FdaBadge {onOpenOnboarding} />
    <IndexingStatusIndicator />
</div>

<style>
    .status-corner {
        /* No positioned ancestor here (`.main-content` is static), so these
           offsets resolve against the initial containing block — which is
           exactly where the hourglass has always sat. Don't "fix" that by
           making an ancestor relative; it would move the corner. */
        position: absolute;
        /* 6px, two above the inset `right` uses: the row's 14px glyphs have to land on the
           optical centre of the 29px titlebar (`--titlebar-height`), and the plain corner inset
           sits them below it. Written as the token minus `--spacing-xxs` rather than a raw `6px`
           so the nudge stays visibly relative to that inset. */
        top: calc(var(--spacing-sm) - var(--spacing-xxs));
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
