<!--
  Link-styled element. Renders <button> for in-app actions (default) or <a> when
  `href` is set. Owns the only sanctioned `cursor: pointer` in the app; Cmdr
  globally sets `cursor: default` on `html` and `<a>` for native macOS feel.

  When using `href`: the URL is decorative (for screen readers, right-click "Copy
  link"). Always intercept the click via `onclick` and route through
  `openExternalUrl()`. Tauri blocks raw `<a>` navigation. The eslint disable for
  `svelte/no-navigation-without-resolve` is intentional here: that rule wants
  SvelteKit's `resolve()`, which doesn't apply to externally-intercepted URLs.
-->
<script lang="ts">
    import type { Snippet } from 'svelte'

    interface Props {
        href?: string
        target?: string
        rel?: string
        type?: 'button' | 'submit'
        disabled?: boolean
        onclick?: (e: MouseEvent) => void
        'aria-label'?: string
        children: Snippet
    }

    const {
        href,
        target,
        rel,
        type = 'button',
        disabled = false,
        onclick,
        'aria-label': ariaLabel,
        children,
    }: Props = $props()
</script>

{#if href}
    <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -->
    <a class="link-button" {href} {target} {rel} {onclick} aria-label={ariaLabel}>
        {@render children()}
    </a>
{:else}
    <button class="link-button" {type} {disabled} {onclick} aria-label={ariaLabel}>
        {@render children()}
    </button>
{/if}

<style>
    .link-button {
        font: inherit;
        color: var(--color-accent-text);
        text-decoration: underline;
        background: none;
        border: none;
        padding: 0;
        /* Cmdr sets `cursor: default` globally on `html` and `a` for native feel.
           Links opt back in here: the only sanctioned `cursor: pointer` in the app. */
        /* stylelint-disable-next-line declaration-property-value-disallowed-list */
        cursor: pointer;
    }

    .link-button:hover {
        /* Keep the a11y-safe accent-text color on hover; the lighter
           --color-accent-hover doesn't meet 4.5:1 on white. Underline is enough
           affordance, already present in the resting state. */
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
