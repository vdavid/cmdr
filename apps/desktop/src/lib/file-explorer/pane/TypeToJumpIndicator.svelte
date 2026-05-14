<script lang="ts">
    /**
     * Tooltip-like overlay that surfaces the user's in-flight type-to-jump buffer
     * in the bottom-right of the pane. Pure presentational: all state lives in
     * `type-to-jump-state.svelte.ts` and is fed in via props.
     *
     * The stale state (italic + reduced opacity) signals that the buffer reset
     * fired but the indicator hasn't hidden yet. The next keystroke will start a
     * fresh buffer.
     */

    interface Props {
        buffer: string
        visible: boolean
        stale: boolean
    }

    const { buffer, visible, stale }: Props = $props()
</script>

{#if visible}
    <div
        class="type-to-jump-indicator"
        class:is-stale={stale}
        role="status"
        aria-live="polite"
        aria-label="Jump to {buffer}"
    >
        Jump: <span class="buffer">{buffer}</span>
    </div>
{/if}

<style>
    .type-to-jump-indicator {
        position: absolute;
        right: var(--spacing-sm);
        bottom: var(--spacing-sm);
        z-index: var(--z-overlay);
        pointer-events: none;
        padding: var(--spacing-xxs) var(--spacing-sm);
        background-color: var(--color-bg-secondary);
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-sm);
        box-shadow: var(--shadow-md);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        font-family: var(--font-system);
        white-space: nowrap;
        transition:
            opacity var(--transition-base),
            font-style var(--transition-base);
        opacity: 1;
    }

    .type-to-jump-indicator.is-stale {
        font-style: italic;
        opacity: 0.5;
    }

    .buffer {
        font-family: var(--font-mono);
        color: var(--color-accent-text);
    }

    @media (prefers-reduced-motion: reduce) {
        .type-to-jump-indicator {
            transition: none;
        }
    }
</style>
