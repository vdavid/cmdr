<script lang="ts">
    /**
     * Chip: the house chip family. Every variant shares one shape (border, radius, hover, focus
     * ring, transitions) and one "on" signal, the accent TINT (`--color-accent-subtle` fill,
     * `--color-accent` border, primary text). ❌ No solid accent fill on a chip: that's for
     * primary buttons and `ToggleGroup`'s chosen cell. What each variant means:
     *
     *   - Filter chip (`variant="filter"`, default): opens a popover. Default state shows just the
     *     label ("Size", "Modified"); a chip carrying a value shows "Size: > 100 MB", tinted.
     *     Carries `aria-haspopup="dialog"` + `aria-expanded`. Backspace on a focused configured
     *     chip clears it.
     *   - Toggle chip (`variant="toggle"`): an on/off option. Carries `aria-pressed`, and tints
     *     while `pressed`. A short glyph label wants the full name as `ariaLabel`.
     *   - Insert chip (`variant="insert"`): a momentary action (insert this text). A plain button
     *     that never shows an "on" state.
     *   - Recent pill (`variant="recent"`): a denser pill with a leading mode badge and a
     *     middle-truncated label. Click loads + runs the entry; right-click removes it. No popover
     *     semantics, no clear.
     *
     * `mono` sets literal syntax (`[N]`, `Aa`, `.*`) in the monospace face; words stay sans.
     * `size="field"` stands a chip as tall as the text fields beside it, for a chip in a row of
     * fields; the default sits in a chip strip beside `ToggleGroup`.
     *
     * TINT and the `×` answer different questions, and the scope chip is why they had to split.
     * The tint says "this chip is CONSTRAINING the search" and follows `value`; the `×` says "you
     * set this, you can unset it" and follows `configured`. The scope chip always constrains (an
     * empty scope box means the pane's current folder, never "everywhere") but often wasn't
     * chosen, so it needs the tint without the `×`. Drawn untinted it read as an empty filter
     * slot, and a user reported search as broken when it was merely scoped (`ERR-FCAXU`).
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
        /**
         * `filter` (popover trigger), `toggle` (on/off option), `insert` (momentary action), or
         * `recent` (history pill). Drives semantics + density.
         */
        variant?: 'filter' | 'toggle' | 'insert' | 'recent'
        /** `strip` (default) matches `ToggleGroup`'s height; `field` matches a text field's. */
        size?: 'strip' | 'field'
        /** Sets the label in the monospace face, for literal syntax like `[N]` or `.*`. */
        mono?: boolean
        /** Whether a `toggle` chip is on. Drives `aria-pressed` and the tint; ignored elsewhere. */
        pressed?: boolean
        /** Static label shown when there's no value ("Size"), or the pill's primary text. */
        label: string
        /**
         * Summary rendered as "label: value" whenever it's set. Independent of `configured`:
         * a chip can voice a DEFAULT it didn't get from the user (the scope chip's "Current
         * folder") without offering to clear it. Drives the tint (see the TINT note above).
         */
        value?: string
        /** Whether the USER configured this filter. Drives the × affordance and Backspace-clears. */
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
        size = 'strip',
        mono = false,
        pressed = false,
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

    const computedAriaLabel = $derived(ariaLabel ?? (value ? `${label}: ${value}` : label))
    const haspopup = $derived(variant === 'filter')
    /**
     * A filter chip carrying a value is narrowing the results, whoever put the value there, so
     * it reads as active. The recent pill has its own hover-only treatment and opts out.
     */
    const filled = $derived(variant === 'filter' && value !== '')
    const isToggle = $derived(variant === 'toggle')

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
    class:chip-field={size === 'field'}
    class:is-mono={mono}
    class:is-filled={filled}
    class:is-pressed={isToggle && pressed}
    class:is-open={isOpen}
    class:is-highlighted={highlighted}
    aria-haspopup={haspopup ? 'dialog' : undefined}
    aria-expanded={haspopup ? isOpen : undefined}
    aria-pressed={isToggle ? pressed : undefined}
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
        {#if value}{label}: {value}{:else}{label}{/if}
    </span>
    {#if haspopup && configured && onClear}
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
    /* The strip size (every variant but `recent`): padding is `ToggleGroup`'s `.tg-item`
       padding, and at the same `--font-size-md` + `line-height: 1` the two land on the same
       height (4 + 14 + 4 + 2 px of border), which is what lets a chip sit beside the Type
       toggle in the filter strip without either looking like the odd one out. Change one,
       change the other. */
    .chip {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        font-weight: 500;
        line-height: var(--font-line-height-flat);
        color: var(--color-text-secondary);
        background: transparent;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        padding: var(--spacing-xs) var(--spacing-md);
        font-size: var(--font-size-md);
        white-space: nowrap;
        transition:
            background var(--transition-base),
            border-color var(--transition-base),
            color var(--transition-base);
    }

    /* === Field size: as tall as the text fields beside it, by their frame's own recipe
       (`app-field.css`: font × tight leading + two input paddings + the border), and at
       least square, so a two-glyph toggle doesn't read as a sliver. === */
    .chip-field {
        justify-content: center;
        min-width: calc(var(--font-size-input) * var(--font-line-height-tight) + 2 * var(--spacing-input) + 2px);
        height: calc(var(--font-size-input) * var(--font-line-height-tight) + 2 * var(--spacing-input) + 2px);
        padding: 0 var(--spacing-xs);
    }

    .is-mono {
        font-family: var(--font-mono);
    }

    /* === Recent pill: deliberately denser than the filter chip (it stacks in a history
       list, not in a strip beside segmented controls), with a truncating label and a
       capped width. === */
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

    .chip.is-filled,
    .chip.is-open,
    .chip.is-pressed {
        /* The family's one "on" signal: a filter chip whose popover is open or that carries a
           value, and a pressed toggle, all read as active through the same tint. */
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
        line-height: var(--font-line-height-flat);
    }

    /* The recent pill truncates its (potentially long) query text. */
    .chip-recent .chip-label {
        overflow: hidden;
        text-overflow: ellipsis;
        max-width: 180px;
    }

    /* The × is the filter chip's alone: it's the one variant a user configures. */
    .chip-filter .chip-clear {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 14px;
        height: 14px;
        border-radius: var(--radius-full);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-md);
        line-height: var(--font-line-height-flat);
    }

    .chip-filter .chip-clear:hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }
</style>
