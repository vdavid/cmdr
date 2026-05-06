<script lang="ts">
    import type { Snippet } from 'svelte'

    interface Props {
        type?: 'button' | 'submit'
        disabled?: boolean
        onclick?: (e: MouseEvent) => void
        'aria-label'?: string
        children: Snippet
    }

    const { type = 'button', disabled = false, onclick, 'aria-label': ariaLabel, children }: Props = $props()
</script>

<button class="link-button" {type} {disabled} {onclick} aria-label={ariaLabel}>
    {@render children()}
</button>

<style>
    .link-button {
        font: inherit;
        color: var(--color-accent-text);
        text-decoration: underline;
        background: none;
        border: none;
        padding: 0;
        /* Cmdr sets `cursor: default` globally on `html` and `a` for native feel.
           Links opt back in here — the only sanctioned `cursor: pointer` in the app. */
        /* stylelint-disable-next-line declaration-property-value-disallowed-list */
        cursor: pointer;
    }

    .link-button:hover {
        /* Keep the a11y-safe accent-text color on hover; the lighter
           --color-accent-hover doesn't meet 4.5:1 on white. Underline is enough
           affordance — already present in the resting state. */
        text-decoration: underline;
    }

    .link-button:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
        box-shadow: var(--shadow-focus-contrast);
    }

    .link-button:disabled {
        opacity: 0.4;
        cursor: not-allowed;
        pointer-events: none;
    }
</style>
