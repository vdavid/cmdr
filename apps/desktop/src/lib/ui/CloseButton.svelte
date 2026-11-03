<!--
  The house round × button: a 22px circle with a 24px hit box, tertiary at rest and tinted on
  hover. Toasts and inline notices (the Ask Cmdr "getting long" nudge) close with it. It never
  positions itself: the caller places it through `class`, styled with `:global(...)`.
-->
<script lang="ts">
    import type { Snippet } from 'svelte'
    import Icon from './Icon.svelte'
    import { tooltip } from '../tooltip/tooltip.js'

    interface Props {
        /** What closing does, for screen readers ("Dismiss notification"). Required: the × alone says nothing. */
        ariaLabel: string
        onclick: () => void
        /** Optional hover hint; the aria-label stays either way. */
        tooltipText?: string
        /** Extra class for the caller's positioning and selector contracts (`toast-close`). */
        class?: string
        /** Decoration drawn under the ×, like the toast's countdown ring. */
        children?: Snippet
    }

    const { ariaLabel, onclick, tooltipText, class: extraClass = '', children }: Props = $props()
</script>

<button
    type="button"
    class="close-button hit-target {extraClass}"
    {onclick}
    use:tooltip={tooltipText}
    aria-label={ariaLabel}
>
    {@render children?.()}
    <Icon name="x" size={10} />
</button>

<style>
    .close-button {
        flex: none;
        display: flex;
        align-items: center;
        justify-content: center;
        width: 22px;
        height: 22px;
        padding: 0;
        background: none;
        border: none;
        border-radius: var(--radius-full);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        line-height: var(--font-line-height-flat);
        transition:
            background var(--transition-fast),
            color var(--transition-fast);
    }

    .close-button:hover {
        background: var(--color-tint-hover);
        color: var(--color-text-primary);
    }
</style>
