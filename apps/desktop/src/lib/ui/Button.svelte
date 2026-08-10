<script lang="ts">
    import { onMount, type Snippet } from 'svelte'
    import { tooltip, type TooltipParam } from '$lib/tooltip/tooltip'

    type Variant = 'primary' | 'secondary' | 'danger'
    type Size = 'regular' | 'mini'

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
         * Focus this button after mount. Uses `requestAnimationFrame` so it lands
         * after a parent `ModalDialog`'s post-`tick()` overlay focus, which would
         * otherwise win and steal focus to the scrim.
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

<button
    bind:this={buttonEl}
    {type}
    class="btn btn-{variant} btn-{size}"
    {disabled}
    aria-disabled={ariaDisabled ? 'true' : undefined}
    {onclick}
    aria-label={ariaLabel}
    use:tooltip={tooltipContent}
>
    {@render children()}
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
