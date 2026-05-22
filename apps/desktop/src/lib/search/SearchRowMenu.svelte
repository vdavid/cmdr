<script lang="ts">
    /**
     * SearchRowMenu: The `…` icon button rendered on a search-result row.
     *
     * Per search-redesign-plan §3.9:
     *   - Visible on the cursor row at all times.
     *   - Rendered (but `opacity: 0`) on non-cursor rows, transitioning in on row hover.
     *   - Clicking the button opens the same native context menu as right-clicking the
     *     row (Open, Reveal in Finder / Open in file manager, Copy path, Copy name —
     *     all routed through the existing native `showFileContextMenu` factory).
     *
     * The actual native popup is shown by the parent (`SearchResults`'s `onMenu`
     * handler), which lets this component stay stateless and lets the parent decide
     * which selection paths to send.
     */
    import IconMoreHorizontal from '~icons/lucide/more-horizontal'
    import { tooltip } from '$lib/tooltip/tooltip'

    interface Props {
        /** True when this row is the keyboard cursor row. Drives always-visible vs. hover-only. */
        isCursorRow: boolean
        /** Opens the context menu. Called on click (left or keyboard). */
        onOpen: () => void
    }

    const { isCursorRow, onOpen }: Props = $props()
</script>

<button
    type="button"
    class="row-menu-btn"
    class:is-cursor={isCursorRow}
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
        opacity: 0;
        transition:
            opacity var(--transition-base),
            background var(--transition-base),
            color var(--transition-base);
    }

    /* The cursor row's button is always visible; non-cursor rows reveal it on row hover.
       Hover state is forwarded by the parent via the row's :hover selector. */
    .row-menu-btn.is-cursor {
        opacity: 1;
    }

    .row-menu-btn:hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    .row-menu-btn:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
        opacity: 1;
    }
</style>
