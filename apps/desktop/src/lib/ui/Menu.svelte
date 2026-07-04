<script lang="ts" module>
    // `MenuItem` lives in `./menu-types.ts` (not here) so non-Svelte consumers resolve
    // it as a real type; re-exported for callers that import it alongside the component.
    export type { MenuItem } from './menu-types'
</script>

<script lang="ts">
    import type { MenuItem } from './menu-types'
    /**
     * A controlled action menu built on Ark UI's `Menu`. The house dropdown menu:
     * opened programmatically at a point (the keyboard-invoked Enter popup anchors it
     * at the cursor row) or from a caller-owned trigger, with Ark owning the keyboard
     * contract (arrow keys, Enter/Space to select, Escape to dismiss, typeahead) and
     * focus management.
     *
     * This wraps Ark 1:1 (named after Ark's `Menu`, per the repo convention). It's a
     * presentational, items-driven shell: the caller controls `open`, supplies the
     * `items`, and reacts to `onSelect`. Positioning is either `anchorPoint` (a
     * viewport point — the context-menu shape) or Ark's default trigger anchoring.
     *
     * Frosted-glass surface with the shared glass tokens (like `Select`), so it drops
     * its blur under reduced transparency. Design tokens only; AA contrast on the
     * accent-highlighted row via `--color-accent-fg`.
     */
    import { Menu } from '@ark-ui/svelte/menu'
    import { Portal } from '@ark-ui/svelte/portal'
    import Icon from './Icon.svelte'

    interface Props {
        /** Controlled open state. */
        open: boolean
        /** Fires when the menu wants to open or close (Escape, outside click, select). */
        onOpenChange: (open: boolean) => void
        items: MenuItem[]
        /** Fires with the selected item's `value`. */
        onSelect: (value: string) => void
        ariaLabel: string
        /**
         * A viewport point to anchor the menu at (the context-menu shape — the Enter
         * popup passes the cursor row's position). Omit to use trigger anchoring.
         */
        anchorPoint?: { x: number; y: number } | null
        /** The row highlighted when the menu opens (the configured/default action). */
        defaultHighlightedValue?: string | null
        /**
         * Teleport the open menu to `document.body` so it escapes ancestor
         * `overflow`/`mask`/stacking contexts. Leave `false` in the viewer window.
         */
        portal?: boolean
    }

    const {
        open,
        onOpenChange,
        items,
        onSelect,
        ariaLabel,
        anchorPoint = null,
        defaultHighlightedValue = null,
        portal = false,
    }: Props = $props()

    const positioning = {
        placement: 'bottom-start' as const,
        gutter: 2,
    }

    // Opened programmatically (controlled `open` + `anchorPoint`, no trigger to hand
    // focus over), so focus the content ourselves once Ark has rendered it —
    // otherwise keyboard navigation (arrows / Enter / Escape) never reaches the menu
    // and it's mouse-only. rAF waits for the content to mount and position after open.
    // Query by class rather than `bind:ref` (Ark's ref typing trips the lint's
    // flow analysis); only one menu is open at a time, so the selector is unambiguous.
    $effect(() => {
        if (!open) return
        const raf = requestAnimationFrame(() => {
            document.querySelector<HTMLElement>('.menu-content')?.focus()
        })
        return () => {
            cancelAnimationFrame(raf)
        }
    })
</script>

<Menu.Root
    {open}
    onOpenChange={(details) => {
        onOpenChange(details.open)
    }}
    onSelect={(details) => {
        onSelect(details.value)
    }}
    {defaultHighlightedValue}
    anchorPoint={anchorPoint ?? undefined}
    aria-label={ariaLabel}
    {positioning}
>
    <Portal disabled={!portal}>
        <Menu.Positioner>
            <Menu.Content class="menu-content">
                {#each items as item (item.value)}
                    <Menu.Item value={item.value} class="menu-item" disabled={item.disabled}>
                        {#if item.icon}
                            <Icon name={item.icon} size={15} aria-hidden="true" />
                        {/if}
                        <span class="menu-item-label">{item.label}</span>
                    </Menu.Item>
                {/each}
            </Menu.Content>
        </Menu.Positioner>
    </Portal>
</Menu.Root>

<style>
    /* Frosted-glass surface, shared tokens with `Select` / tooltips so every glass
       surface reads as one material; the blur drops under reduced transparency
       (the token flips opaque). */
    :global(.menu-content) {
        display: flex;
        flex-direction: column;
        gap: 1px;
        min-width: 200px;
        padding: var(--spacing-xs);
        background: var(--color-bg-glass);
        -webkit-backdrop-filter: saturate(180%) blur(20px);
        backdrop-filter: saturate(180%) blur(20px);
        border: 0.5px solid var(--color-border-glass);
        border-radius: var(--radius-lg);
        box-shadow: var(--shadow-lg);
        z-index: var(--z-dropdown);
        outline: none;
    }

    :global(html.reduce-transparency .menu-content) {
        -webkit-backdrop-filter: none;
        backdrop-filter: none;
    }

    :global(.menu-content:focus),
    :global(.menu-content:focus-visible) {
        outline: none;
    }

    :global(.menu-content .menu-item) {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) var(--spacing-sm);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        cursor: default;
        outline: none;
        white-space: nowrap;
    }

    /* Highlighted row (keyboard / pointer cursor): accent fill, like macOS. */
    :global(.menu-content .menu-item[data-highlighted]) {
        background: var(--color-accent);
        color: var(--color-accent-fg);
    }

    :global(.menu-content .menu-item[data-disabled]) {
        opacity: 0.5;
    }

    :global(.menu-content .menu-item-label) {
        flex: 1;
        min-width: 0;
    }
</style>
