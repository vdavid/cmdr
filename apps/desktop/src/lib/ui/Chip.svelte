<script lang="ts">
    /**
     * Chip: a small pill button used across the query dialogs. Two flavors, one component:
     *
     *   - Filter chip (`variant="filter"`, default): opens a popover. Default state shows just the
     *     label ("Size", "Modified"); configured state shows "Size: > 100 MB" plus an `×` clear
     *     affordance. Carries `aria-haspopup="dialog"` + `aria-expanded`. Backspace on a focused
     *     configured chip clears it.
     *   - Recent pill (`variant="recent"`): a denser pill with a leading mode badge and a
     *     middle-truncated label. Click loads + runs the entry; right-click removes it. No popover
     *     semantics, no clear.
     *
     * The chip is a single button. The `×` is a decorative span (not a nested `<button>`, which is
     * invalid HTML and trips axe's `nested-interactive`); the keyboard clear path is Backspace.
     * `onClear`'s `mousedown` handler stops propagation so the chip's own activate doesn't fire.
     */
    import type { Snippet } from 'svelte'
    import { tooltip, type TooltipParam } from '$lib/tooltip/tooltip'

    interface Props {
        /** Bindable ref to the chip button (so the parent can focus it after Esc, etc.). */
        chipElement?: HTMLButtonElement
        /** `filter` (popover trigger) or `recent` (history pill). Drives semantics + density. */
        variant?: 'filter' | 'recent'
        /** Static label shown when no value is configured ("Size"), or the pill's primary text. */
        label: string
        /** Summary shown when configured. When set, the chip switches to its filled style. */
        value?: string
        /** Whether the chip is in its "configured" state. Drives style and the × affordance. */
        configured?: boolean
        /** True when the popover this chip controls is open. Drives the active-style ring. */
        isOpen?: boolean
        /** Whether the chip is disabled. */
        disabled?: boolean
        /** Highlighted because AI just populated the underlying filter. */
        highlighted?: boolean
        /** Fired on click, Enter, or Space. */
        onActivate: () => void
        /** Fired when the user clears the configured value (× click or Backspace on focus). */
        onClear?: () => void
        /** Fired on right-click (recent pill's "remove from history"). */
        onContextMenu?: (e: MouseEvent) => void
        /** Optional aria-label override. Defaults to label + value when configured. */
        ariaLabel?: string
        /** Optional tooltip (string or config). */
        tooltipContent?: TooltipParam
        /** Optional leading slot, e.g. a tiny icon or a mode badge. */
        leading?: Snippet
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let {
        chipElement = $bindable(),
        variant = 'filter',
        label,
        value = '',
        configured = false,
        isOpen = false,
        disabled = false,
        highlighted = false,
        onActivate,
        onClear,
        onContextMenu,
        ariaLabel,
        tooltipContent,
        leading,
    }: Props = $props()
    /* eslint-enable prefer-const */

    const computedAriaLabel = $derived(ariaLabel ?? (configured ? `${label}: ${value}` : label))
    const haspopup = $derived(variant === 'filter')

    function handleKeyDown(e: KeyboardEvent): void {
        if (disabled) return
        if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault()
            onActivate()
            return
        }
        // Backspace on a focused configured chip clears it. We don't intercept Backspace when not
        // configured (it could fire from a chip that just had focus; no harm letting it bubble).
        if (e.key === 'Backspace' && configured && onClear) {
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
        onClear?.()
    }
</script>

<button
    bind:this={chipElement}
    type="button"
    class="chip"
    class:chip-filter={variant === 'filter'}
    class:chip-recent={variant === 'recent'}
    class:is-configured={configured}
    class:is-open={isOpen}
    class:is-highlighted={highlighted}
    aria-haspopup={haspopup ? 'dialog' : undefined}
    aria-expanded={haspopup ? isOpen : undefined}
    aria-label={computedAriaLabel}
    {disabled}
    onclick={() => {
        if (!disabled) onActivate()
    }}
    oncontextmenu={onContextMenu}
    onkeydown={handleKeyDown}
    use:tooltip={tooltipContent ?? ''}
>
    {#if leading}<span class="chip-leading">{@render leading()}</span>{/if}
    <span class="chip-label">
        {#if configured}{label}: {value}{:else}{label}{/if}
    </span>
    {#if configured && onClear}
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
    .chip {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
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

    /* === Filter chip: opens a popover. Slightly larger type for the calmer chip strip. === */
    .chip-filter {
        padding: var(--spacing-xxs) var(--spacing-sm);
        font-size: var(--font-size-md);
    }

    /* === Recent pill: denser, with a truncating label and a capped width. === */
    .chip-recent {
        padding: var(--spacing-xxs) var(--spacing-sm);
        font-size: var(--font-size-sm);
        max-width: 240px;
        flex-shrink: 0;
    }

    .chip:not(:disabled):hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    /* The recent pill hovers to the accent tint (no configured/open state of its own). */
    .chip-recent:not(:disabled):hover {
        background: var(--color-accent-subtle);
        border-color: var(--color-accent);
        color: var(--color-text-primary);
    }

    .chip.is-configured,
    .chip.is-open {
        /* When the chip's popover is open OR it carries a configured value, it reads as the
           "active" target via the same tinted treatment. */
        background: var(--color-accent-subtle);
        border-color: var(--color-accent);
        color: var(--color-text-primary);
    }

    .chip:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .chip.is-highlighted {
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

    /* The recent pill truncates its (potentially long) query text. */
    .chip-recent .chip-label {
        overflow: hidden;
        text-overflow: ellipsis;
        max-width: 180px;
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
