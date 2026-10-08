<script lang="ts">
    import { tString } from '$lib/intl/messages.svelte'
    import Icon from '$lib/ui/Icon.svelte'

    /**
     * Tooltip-like overlay that surfaces the user's in-flight type-to-jump buffer
     * in the bottom-right of the pane. Pure presentational: all state lives in
     * `type-to-jump-state.svelte.ts` and is fed in via props.
     *
     * The stale state (italic + reduced opacity) signals that the buffer reset
     * fired but the indicator hasn't hidden yet. The next keystroke will start a
     * fresh buffer.
     *
     * `kind="filter"` shows the quick filter's pattern instead ("Filter: …"),
     * which stays up for as long as the filter narrows the list, with a × that
     * clears it by mouse (`onClear`). The × takes no focus: the keyboard's way out
     * is Esc, and a click must leave focus on the pane it filters.
     */

    interface Props {
        buffer: string
        visible: boolean
        stale: boolean
        kind?: 'jump' | 'filter'
        /** Clears the filter; shows the × when set (filter only). */
        onClear?: () => void
    }

    const { buffer, visible, stale, kind = 'jump', onClear }: Props = $props()
</script>

{#if visible}
    <div class="type-to-jump-indicator" class:is-stale={stale}>
        <span
            role="status"
            aria-live="polite"
            aria-label={kind === 'filter'
                ? tString('fileExplorer.quickFilter.ariaLabel', { pattern: buffer })
                : tString('fileExplorer.typeToJump.ariaLabel', { buffer })}
        >
            {kind === 'filter'
                ? tString('fileExplorer.quickFilter.prefix')
                : tString('fileExplorer.typeToJump.prefix')}<span class="buffer">{buffer}</span>
        </span>
        {#if kind === 'filter' && onClear}
            <button
                type="button"
                class="clear"
                tabindex="-1"
                aria-label={tString('fileExplorer.quickFilter.clear')}
                onmousedown={(e: MouseEvent) => {
                    e.preventDefault()
                }}
                onclick={onClear}
            >
                <Icon name="circle-x" size={12} />
            </button>
        {/if}
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
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        transition:
            color var(--transition-base),
            font-style var(--transition-base);
    }

    /* Stale: the buffer reset fired but the indicator hasn't hidden yet. Quiet
       text (the shared `--color-text-quiet` token) + italic on the whole
       overlay; the `.buffer` span's own accent color is overridden the same
       way, since it doesn't inherit through its explicit `color:` below. */
    .type-to-jump-indicator.is-stale {
        font-style: italic;
        color: var(--color-text-quiet);
    }

    .buffer {
        font-family: var(--font-mono);
        color: var(--color-accent-text);
        transition: color var(--transition-base);
    }

    /* The overlay lets clicks through to the list; only the × takes them. */
    .clear {
        pointer-events: auto;
        display: flex;
        padding: 0;
        border: none;
        background: none;
        color: var(--color-text-quiet);
        cursor: default;
    }

    .clear:hover {
        color: var(--color-text-primary);
    }

    .type-to-jump-indicator.is-stale .buffer {
        color: var(--color-text-quiet);
    }

    @media (prefers-reduced-motion: reduce) {
        .type-to-jump-indicator,
        .buffer {
            transition: none;
        }
    }
</style>
