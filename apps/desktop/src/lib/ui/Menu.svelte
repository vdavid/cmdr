<script lang="ts" module>
    // `MenuItem` lives in `./menu-types.ts` (not here) so non-Svelte consumers resolve
    // it as a real type; re-exported for callers that import it alongside the component.
    export type { MenuItem } from './menu-types'
</script>

<script lang="ts">
    import type { MenuItem } from './menu-types'
    /**
     * A presentational action menu: a small popup rendered at a viewport point, the
     * house dropdown menu (the keyboard-invoked Enter popup anchors it at the cursor
     * row). Mounted only while shown — the CALLER controls visibility with an `{#if}`
     * around this component.
     *
     * Deliberately NOT built on Ark/zag's `Menu` machine: that machine is trigger-
     * driven and doesn't reliably open (mounted-already-open) or close (controlled
     * `open=false`) when driven programmatically at a point, which this use needs. So
     * this owns its rendering, positioning, pointer selection, and outside-click
     * dismissal directly. KEYBOARD navigation is the caller's job (it keeps focus and
     * routes keys), reflected here via the controlled `highlightedValue`; pointer
     * hover reports back through `onHighlightChange`.
     *
     * Portaled to `document.body` so `position: fixed` isn't captured by an ancestor
     * transform, and to escape ancestor `overflow`/`mask`. Frosted-glass surface with
     * the shared glass tokens (like `Select`); design tokens only; AA contrast on the
     * highlighted row.
     */
    import { Portal } from '@ark-ui/svelte/portal'
    import Icon from './Icon.svelte'

    interface Props {
        items: MenuItem[]
        /** Fires with the selected item's `value` on POINTER selection. */
        onSelect: (value: string) => void
        /** Fires when the menu wants to close (an outside pointer-down). */
        onClose: () => void
        ariaLabel: string
        /** The viewport point to anchor the menu's top-left near. */
        anchorPoint?: { x: number; y: number } | null
        /** The controlled highlighted row (the caller owns keyboard nav). */
        highlightedValue?: string | null
        /** Fires when the highlight changes (pointer hover), so the caller can sync. */
        onHighlightChange?: (value: string | null) => void
    }

    const {
        items,
        onSelect,
        onClose,
        ariaLabel,
        anchorPoint = null,
        highlightedValue = null,
        onHighlightChange,
    }: Props = $props()

    // A generous offset keeps the menu clear of the pointer; clamp to the viewport so
    // it never renders off-screen when anchored near an edge.
    const MENU_MAX_WIDTH = 260
    const left = $derived(anchorPoint ? Math.min(anchorPoint.x, window.innerWidth - MENU_MAX_WIDTH - 8) : 0)
    const top = $derived(anchorPoint ? Math.min(anchorPoint.y, window.innerHeight - items.length * 36 - 16) : 0)
</script>

<Portal>
    <!-- Transparent full-viewport catcher: a pointer-down anywhere outside the menu
         dismisses it. The menu sits above it, so item clicks land on the item. -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
        class="menu-backdrop"
        onpointerdown={() => {
            onClose()
        }}
    ></div>
    <div class="menu-content" role="menu" aria-label={ariaLabel} style="left: {left}px; top: {top}px">
        {#each items as item (item.value)}
            <button
                type="button"
                role="menuitem"
                class="menu-item"
                class:is-highlighted={item.value === highlightedValue}
                disabled={item.disabled}
                onpointerenter={() => {
                    onHighlightChange?.(item.value)
                }}
                onclick={() => {
                    onSelect(item.value)
                }}
            >
                {#if item.icon}
                    <Icon name={item.icon} size={15} aria-hidden="true" />
                {/if}
                <span class="menu-item-label">{item.label}</span>
            </button>
        {/each}
    </div>
</Portal>

<style>
    .menu-backdrop {
        position: fixed;
        inset: 0;
        z-index: var(--z-dropdown);
    }

    /* Frosted-glass surface, shared tokens with `Select` / tooltips so every glass
       surface reads as one material; the blur drops under reduced transparency
       (the token flips opaque). */
    .menu-content {
        position: fixed;
        display: flex;
        flex-direction: column;
        gap: 1px;
        min-width: 200px;
        max-width: 260px;
        padding: var(--spacing-xs);
        background: var(--color-bg-glass);
        -webkit-backdrop-filter: saturate(180%) blur(20px);
        backdrop-filter: saturate(180%) blur(20px);
        border: 0.5px solid var(--color-border-glass);
        border-radius: var(--radius-lg);
        box-shadow: var(--shadow-lg);
        /* Above the backdrop so item clicks land on the item, not the catcher. */
        z-index: calc(var(--z-dropdown) + 1);
    }

    :global(html.reduce-transparency) .menu-content {
        -webkit-backdrop-filter: none;
        backdrop-filter: none;
    }

    .menu-item {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) var(--spacing-sm);
        border: none;
        border-radius: var(--radius-sm);
        background: transparent;
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        text-align: left;
        cursor: default;
        outline: none;
        white-space: nowrap;
    }

    /* Highlighted row (keyboard / pointer cursor): accent fill, like macOS. */
    .menu-item.is-highlighted {
        background: var(--color-accent);
        color: var(--color-accent-fg);
    }

    .menu-item:disabled {
        opacity: 0.5;
    }

    .menu-item-label {
        flex: 1;
        min-width: 0;
    }
</style>
