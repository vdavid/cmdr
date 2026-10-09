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
        /**
         * For a link that toggles a disclosure below it: the pair tells a screen reader
         * that the link opens something and whether it's open. Button form only.
         */
        'aria-expanded'?: boolean
        'aria-controls'?: string
        /**
         * For a link that switches a view on and off in place (a filter, say): it reads as a
         * toggle button, pressed or not. Button form only.
         */
        'aria-pressed'?: boolean
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
        'aria-expanded': ariaExpanded,
        'aria-controls': ariaControls,
        'aria-pressed': ariaPressed,
        children,
    }: Props = $props()
</script>

{#if href}
    <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- external URL in a Tauri app, not a SvelteKit route; resolve() does not apply -->
    <a class="link-button" {href} {target} {rel} {onclick} aria-label={ariaLabel}>
        {@render children()}
    </a>
{:else}
    <button
        class="link-button"
        {type}
        {disabled}
        {onclick}
        aria-label={ariaLabel}
        aria-expanded={ariaExpanded}
        aria-controls={ariaControls}
        aria-pressed={ariaPressed}
    >
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
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- links opt back into pointer; Cmdr sets cursor: default globally (see above) */
        cursor: pointer;
    }

    .link-button:hover {
        /* Keep the a11y-safe accent-text color on hover; the lighter
           --color-accent-hover doesn't meet 4.5:1 on white. Underline is enough
           affordance, already present in the resting state. */
        text-decoration: underline;
    }

    /* A pressed toggle drops the underline and sits on an accent wash: it's the view in use,
       no longer a place to go. The wash bleeds into the margin so the text doesn't move. */
    .link-button[aria-pressed='true'] {
        text-decoration: none;
        background: var(--color-accent-subtle);
        border-radius: var(--radius-xs);
        padding: 0 var(--spacing-xxs);
        margin: 0 calc(-1 * var(--spacing-xxs));
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
