<script lang="ts">
    /**
     * Single filter chip. Two display modes:
     *
     *   Default ("any" state): shows just the label, e.g. "Size", "Modified", "Search in".
     *   Configured: shows the label-value summary plus an `×` clear affordance, e.g.
     *               "Size > 100 MB ×". Clicking × clears; pressing Backspace on a focused
     *               configured chip clears too.
     *
     * The chip is a single button that opens a popover. The `×` is a nested clickable span (not
     * a nested `<button>`) because nested buttons are invalid HTML; instead, we stop propagation
     * on the clear click so it never reaches the chip's own onActivate.
     *
     * The parent owns the open/close state of the associated popover; this component just fires
     * `onActivate` when Enter/Space/click happens.
     */
    import type { Snippet } from 'svelte'

    interface Props {
        /** Bindable ref to the chip button (so the parent can focus it after Esc, etc.). */
        chipElement?: HTMLButtonElement
        /** Static label shown when no value is configured ("Size", "Modified", "Search in"). */
        label: string
        /** Summary shown when configured. When set, the chip switches to its filled style. */
        value?: string
        /** Whether the chip is in its "configured" state. Drives style and the × affordance. */
        configured: boolean
        /** True when the popover this chip controls is open. Drives the active-style ring. */
        isOpen: boolean
        /** Whether the chip is disabled (e.g. search index unavailable). */
        disabled?: boolean
        /** Highlighted because AI just populated the underlying filter. */
        highlighted?: boolean
        /** Fired on click, Enter, or Space. */
        onActivate: () => void
        /** Fired when the user clears the configured value (× click or Backspace on focus). */
        onClear: () => void
        /** Optional aria-label override. Defaults to label + value when configured. */
        ariaLabel?: string
        /** Optional leading slot, e.g. a tiny icon. */
        leading?: Snippet
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let {
        chipElement = $bindable(),
        label,
        value = '',
        configured,
        isOpen,
        disabled = false,
        highlighted = false,
        onActivate,
        onClear,
        ariaLabel,
        leading,
    }: Props = $props()
    /* eslint-enable prefer-const */

    const computedAriaLabel = $derived(ariaLabel ?? (configured ? `${label}: ${value}` : label))

    function handleKeyDown(e: KeyboardEvent): void {
        if (disabled) return
        if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault()
            onActivate()
            return
        }
        // Backspace on a focused configured chip clears it. Matches the contract documented in
        // the search redesign plan §3.2. We don't intercept Backspace when not configured (it
        // could fire from a chip that just had focus; no harm letting it bubble harmlessly).
        if (e.key === 'Backspace' && configured) {
            e.preventDefault()
            onClear()
        }
    }

    /**
     * Clears the filter when the user mousedowns the × marker. We listen on `mousedown` rather
     * than `click` so the event fires before the chip's `onclick` (which would otherwise re-open
     * the popover). `stopPropagation` prevents the chip-level click from firing at all.
     */
    function handleClearClick(e: MouseEvent): void {
        e.stopPropagation()
        e.preventDefault()
        if (disabled) return
        onClear()
    }
</script>

<button
    bind:this={chipElement}
    type="button"
    class="filter-chip"
    class:is-configured={configured}
    class:is-open={isOpen}
    class:is-highlighted={highlighted}
    aria-haspopup="dialog"
    aria-expanded={isOpen}
    aria-label={computedAriaLabel}
    {disabled}
    onclick={() => {
        if (!disabled) onActivate()
    }}
    onkeydown={handleKeyDown}
>
    {#if leading}<span class="chip-leading">{@render leading()}</span>{/if}
    <span class="chip-label">
        {#if configured}{label}: {value}{:else}{label}{/if}
    </span>
    {#if configured}
        <!--
          Decorative clear marker (no role, no tabindex). The keyboard path is Backspace on the
          chip itself; the × is a mouse-only affordance. Nested interactive controls (a button
          inside a button) trip "nested-interactive" in axe and confuse assistive tech, so the
          chip stays a single button. The mousedown handler stops propagation so the chip's own
          activate doesn't also fire.
        -->
        <span
            class="chip-clear"
            aria-hidden="true"
            onmousedown={handleClearClick}
        >×</span>
    {/if}
</button>

<style>
    .filter-chip {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-xxs) var(--spacing-sm);
        font-size: var(--font-size-md);
        font-weight: 500;
        line-height: 1;
        color: var(--color-text-secondary);
        background: transparent;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        white-space: nowrap;
        transition:
            background var(--transition-base),
            border-color var(--transition-base),
            color var(--transition-base);
    }

    .filter-chip:not(:disabled):hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    .filter-chip.is-configured {
        background: var(--color-accent-subtle);
        border-color: var(--color-accent);
        color: var(--color-text-primary);
    }

    .filter-chip.is-open {
        /* When the chip's popover is open, the chip itself reads as the "active" target. We use
           the same tinted treatment as configured so open + default-state still feels selected. */
        background: var(--color-accent-subtle);
        border-color: var(--color-accent);
        color: var(--color-text-primary);
    }

    .filter-chip:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .filter-chip.is-highlighted {
        background: var(--color-accent-subtle);
        border-radius: var(--radius-sm);
        transition: background 1.5s ease-out;
    }

    .chip-leading {
        display: inline-flex;
        align-items: center;
    }

    .chip-label {
        line-height: 1;
    }

    .chip-clear {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 14px;
        height: 14px;
        border-radius: var(--radius-full);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-md);
        line-height: 1;
    }

    .chip-clear:hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }
</style>
