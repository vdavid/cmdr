<script lang="ts">
    /**
     * The level badge at a toast's leading edge: a filled circle (a rounded triangle for
     * `warn`) in the level's color, with a glyph over it. Flat fills only; the colors are the
     * `--color-toast-*-icon` tokens, so each scheme tunes its own.
     *
     * Decorative (`aria-hidden`): the toast's role (`status` or `alert`) and its words carry the
     * level for a screen reader. The 28px size is fixed, ❌ not a prop: the toast's insets and
     * its minimum text height are built around it.
     */
    import type { ToastLevel } from './toast-store.svelte'

    interface Props {
        level: ToastLevel
    }

    const { level }: Props = $props()
</script>

<svg
    class="toast-level-icon"
    class:info={level === 'info'}
    class:success={level === 'success'}
    class:warn={level === 'warn'}
    class:error={level === 'error'}
    width="28"
    height="28"
    viewBox="0 0 24 24"
    aria-hidden="true"
>
    {#if level === 'warn'}
        <!-- The stroke, in the fill color, is what rounds the corners. -->
        <path class="shape" d="M12 3.6 L22 20.8 H2 Z" stroke-width="3.6" stroke-linejoin="round" />
        <rect class="glyph" x="10.85" y="8.8" width="2.3" height="6.4" rx="1.15" />
        <circle class="glyph" cx="12" cy="18.2" r="1.35" />
    {:else}
        <circle class="shape" cx="12" cy="12" r="12" />
        {#if level === 'info'}
            <circle class="glyph" cx="12" cy="7.2" r="1.6" />
            <rect class="glyph" x="10.8" y="10.2" width="2.4" height="7.8" rx="1.2" />
        {:else if level === 'error'}
            <rect class="glyph" x="10.8" y="5.6" width="2.4" height="8.8" rx="1.2" />
            <circle class="glyph" cx="12" cy="17.8" r="1.6" />
        {:else if level === 'success'}
            <path
                class="glyph-stroke"
                d="M7.3 12.4 L10.5 15.6 L16.8 9.2"
                stroke-width="2.4"
                stroke-linecap="round"
                stroke-linejoin="round"
            />
        {:else}
            <circle class="glyph" cx="7.5" cy="12" r="1.5" />
            <circle class="glyph" cx="12" cy="12" r="1.5" />
            <circle class="glyph" cx="16.5" cy="12" r="1.5" />
        {/if}
    {/if}
</svg>

<style>
    .toast-level-icon {
        display: block;
        flex-shrink: 0;
    }

    .shape {
        fill: var(--color-toast-default-icon);
    }

    .glyph {
        fill: var(--color-toast-icon-glyph);
    }

    .glyph-stroke {
        fill: none;
        stroke: var(--color-toast-icon-glyph);
    }

    .info .shape {
        fill: var(--color-toast-info-icon);
    }

    .success .shape {
        fill: var(--color-toast-success-icon);
    }

    .warn .shape {
        fill: var(--color-toast-warn-icon);
        stroke: var(--color-toast-warn-icon);
    }

    .warn .glyph {
        fill: var(--color-toast-warn-icon-glyph);
    }

    .error .shape {
        fill: var(--color-toast-error-icon);
    }
</style>
