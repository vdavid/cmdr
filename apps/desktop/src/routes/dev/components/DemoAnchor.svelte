<script lang="ts">
    /** The plain trigger button the floater demos (Popover, Menu, FilterPopover) open from. */
    import type { Snippet } from 'svelte'

    interface Props {
        /** The rendered `<button>`, for demos that anchor a floater to it. */
        el?: HTMLButtonElement
        onclick: (event: MouseEvent) => void
        children: Snippet
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let { el = $bindable(), onclick, children }: Props = $props()
    /* eslint-enable prefer-const */
</script>

<button bind:this={el} type="button" class="demo-anchor" {onclick}>
    {@render children()}
</button>

<style>
    .demo-anchor {
        padding: var(--spacing-xs) var(--spacing-md);
        font-size: var(--font-size-sm);
        background: transparent;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
    }
</style>
