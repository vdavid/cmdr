<script lang="ts">
    import { onMount, type Snippet } from 'svelte'
    import { tooltip, type TooltipParam } from '#lib/tooltip/tooltip.js'
    import type { CommandId } from '#lib/commands/index.js'
    import ShortcutChip from './ShortcutChip.svelte'

    type Variant = 'primary' | 'secondary' | 'danger'
    type Size = 'regular' | 'mini'
    /** A fixed key (`"F2"`, `"S"`, `"⏎"`), or a command whose live binding shows (nothing while it has none). */
    type KeyHint = string | { commandId: CommandId }

    interface Props {
        variant?: Variant
        size?: Size
        disabled?: boolean
        /**
         * Blocked, not disabled: the button looks and reads unavailable (`aria-disabled`,
         * dimmed, `not-allowed` cursor) but stays focusable and still fires `onclick`, so
         * the handler can explain WHY instead of the press silently doing nothing. Reach
         * for this over `disabled` whenever the user can do something about the block, and
         * pair it with a `tooltipContent` naming the missing precondition. `disabled` stays right
         * for a button that's inert for reasons the user can't act on.
         */
        ariaDisabled?: boolean
        /** Tooltip on the button itself; a wrapper can't carry it, since focus doesn't bubble. */
        tooltipContent?: TooltipParam
        type?: 'button' | 'submit'
        onclick?: (e: MouseEvent) => void
        'aria-label'?: string
        /**
         * Id of the element that says WHICH thing this button acts on. For a button
         * repeated once per row, this is what keeps the name short ("Roll back") while
         * still telling a screen reader which row it belongs to.
         */
        'aria-describedby'?: string
        /**
         * The key that presses this button, drawn as a chip after the label in its own slot:
         * centered on the button, never wrapping away from the label, and `aria-hidden` (read
         * into the name it garbles it: "Skip S"). ❌ Never a `ShortcutChip` in `children`
         * (`cmdr/key-hint-via-button-prop`). Pair a fixed key with `aria-keyshortcuts`. ❌ No
         * `aria-label` beside a letter key: axe counts the on-screen chip as visible text, so no
         * label can contain it (WCAG 2.5.3); the visible word is the name.
         */
        keyHint?: KeyHint
        /** The key that presses this button, for a screen reader, in ARIA's spelling (`"Alt+C"`, `"S Enter"`). */
        'aria-keyshortcuts'?: string
        /**
         * For a button that shows and hides a region (a disclosure): the pair tells a screen
         * reader the button opens something, which one, and whether it's open now.
         */
        'aria-expanded'?: boolean
        'aria-controls'?: string
        /**
         * `-1` takes the button out of the Tab order while it stays clickable and focusable from
         * code: for a composite widget (a grid row) whose arrow keys reach its buttons instead.
         */
        tabindex?: -1 | 0
        /**
         * Focus this button after mount. Uses `requestAnimationFrame` so it lands
         * after a parent `ModalDialog` has settled its own mount-time focus (the
         * dialog leaves the scrim unfocused once something inside owns focus).
         */
        autoFocus?: boolean
        children: Snippet
    }

    const {
        variant = 'secondary',
        size = 'regular',
        disabled = false,
        ariaDisabled = false,
        tooltipContent,
        type = 'button',
        onclick,
        'aria-label': ariaLabel,
        'aria-describedby': ariaDescribedby,
        keyHint,
        'aria-keyshortcuts': ariaKeyshortcuts,
        'aria-expanded': ariaExpanded,
        'aria-controls': ariaControls,
        tabindex,
        autoFocus = false,
        children,
    }: Props = $props()

    let buttonEl: HTMLButtonElement | undefined
    onMount(() => {
        if (!autoFocus) return
        requestAnimationFrame(() => {
            buttonEl?.focus()
        })
    })
</script>

<!-- eslint-disable-next-line cmdr/key-hint-via-button-prop -- this IS the key hint's slot -->
<button
    bind:this={buttonEl}
    {type}
    class="btn btn-{variant} btn-{size}"
    class:btn-keyed={keyHint !== undefined}
    {disabled}
    aria-disabled={ariaDisabled ? 'true' : undefined}
    {onclick}
    aria-label={ariaLabel}
    aria-describedby={ariaDescribedby}
    aria-keyshortcuts={ariaKeyshortcuts}
    aria-expanded={ariaExpanded}
    aria-controls={ariaControls}
    {tabindex}
    use:tooltip={tooltipContent}
>
    {#if keyHint === undefined}
        {@render children()}
    {:else}
        <span class="btn-label">{@render children()}</span>
        <!-- No whitespace inside: an unbound command draws nothing, and `:empty` then drops the slot. -->
        <span class="btn-key-hint" aria-hidden="true"
            >{#if typeof keyHint === 'string'}<ShortcutChip key={keyHint} size="sm" />{:else}<ShortcutChip
                    commandId={keyHint.commandId}
                    clickable={false}
                    size="sm"
                />{/if}</span
        >
    {/if}
</button>

<style>
    /* Fully-rounded ends (capsule), the shape macOS gives an alert's action buttons.
       `--radius-full` rather than a percentage: a percentage curves against the box
       and would go oval on a wide button. */
    .btn {
        font-weight: 500;
        line-height: var(--font-line-height-normal);
        border-radius: var(--radius-full);
        transition: all var(--transition-base);
    }

    .btn:disabled {
        opacity: 0.4;
        cursor: not-allowed;
        pointer-events: none;
    }

    /* Blocked: same read as disabled, but pointer events stay live (and the button stays
       focusable) so the press reaches `onclick` and the handler can say what's missing. */
    .btn[aria-disabled='true'] {
        opacity: 0.4;
        cursor: not-allowed;
    }

    .btn:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
        box-shadow: var(--shadow-focus-contrast);
    }

    /* A key hint gets its own flex slot, so the chip centers on the button's cross axis
       (inline, it sat on the text's baseline and read low) and never wraps under the
       label: the button grows instead. */
    .btn-keyed {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-sm);
        white-space: nowrap;
    }

    .btn-label,
    .btn-key-hint {
        display: inline-flex;
        align-items: center;
    }

    /* A command with no binding draws no chip, and its empty slot mustn't add a gap. */
    .btn-key-hint:empty {
        display: none;
    }

    /* === Size: regular === */
    .btn-regular {
        padding: 7px 20px;
        font-size: var(--font-size-md);
    }

    /* === Size: mini === */
    .btn-mini {
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- Mini button height target: 22px */
        padding: 3px 12px;
        font-size: var(--font-size-sm);
    }

    /* === Variant: primary === */
    .btn-primary {
        background: var(--color-accent);
        color: var(--color-accent-fg);
        border: none;
    }

    .btn-primary:hover:not(:disabled, [aria-disabled='true']) {
        background: var(--color-accent-hover);
    }

    /* === Variant: secondary === */
    .btn-secondary {
        background: transparent;
        color: var(--color-text-secondary);
        border: 1px solid var(--color-border);
    }

    .btn-secondary:hover:not(:disabled, [aria-disabled='true']) {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    /* === Variant: danger === */
    .btn-danger {
        background: transparent;
        color: var(--color-error-text);
        border: 1px solid var(--color-error);
    }

    .btn-danger:hover:not(:disabled, [aria-disabled='true']) {
        background: color-mix(in srgb, var(--color-error), transparent 90%);
    }
</style>
