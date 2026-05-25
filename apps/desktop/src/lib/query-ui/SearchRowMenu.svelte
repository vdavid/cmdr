<script lang="ts">
    /**
     * SearchRowMenu: The `…` icon button rendered on a search-result row.
     *
     * Visible on EVERY row at all times. (An earlier hover-only / cursor-only treatment
     * kept it hidden until proximity, but discoverability matters more than visual quiet
     * here.) The column header reads "Actions" so the button column has a clear name in
     * the grid.
     *
     * Clicking the button opens the same native context menu as right-clicking the
     * row (Open, Reveal in Finder / Open in file manager, Copy path, Copy name —
     * all routed through the existing native `showFileContextMenu` factory). The
     * actual native popup is shown by the parent (`SearchResults`'s `onRowMenu`
     * handler), which lets this component stay stateless and lets the parent decide
     * which selection paths to send.
     */
    import IconMoreHorizontal from '~icons/lucide/more-horizontal'
    import { tooltip } from '$lib/tooltip/tooltip'

    interface Props {
        /** Opens the context menu. Called on click (left or keyboard). */
        onOpen: () => void
    }

    const { onOpen }: Props = $props()
</script>

<button
    type="button"
    class="row-menu-btn"
    aria-label="More actions"
    tabindex="-1"
    use:tooltip={'More actions'}
    onclick={(e) => {
        e.stopPropagation()
        onOpen()
    }}
>
    <IconMoreHorizontal width="16" height="16" />
</button>

<style>
    /* Always-on. The previous `opacity: 0` baseline + cursor / hover overrides are gone —
       the button is a permanent affordance, matching the "Actions" column header. */
    .row-menu-btn {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 22px;
        height: 22px;
        padding: 0;
        background: transparent;
        border: 0;
        border-radius: var(--radius-sm);
        color: var(--color-text-tertiary);
        transition:
            background var(--transition-base),
            color var(--transition-base);
    }

    .row-menu-btn:hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    .row-menu-btn:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
    }
</style>
