<script lang="ts">
    import type { Snippet } from 'svelte'

    type Variant = 'primary' | 'secondary' | 'danger'
    type Size = 'regular' | 'mini'

    interface Props {
        variant?: Variant
        size?: Size
        disabled?: boolean
        type?: 'button' | 'submit'
        onclick?: (e: MouseEvent) => void
        'aria-label'?: string
        children: Snippet
    }

    const {
        variant = 'secondary',
        size = 'regular',
        disabled = false,
        type = 'button',
        onclick,
        'aria-label': ariaLabel,
        children,
    }: Props = $props()
</script>

<button {type} class="btn btn-{variant} btn-{size}" {disabled} {onclick} aria-label={ariaLabel}>
    {@render children()}
</button>

<style>
    .btn {
        font-weight: 500;
        line-height: 1;
        border-radius: var(--radius-md);
        transition: all var(--transition-base);
    }

    .btn:disabled {
        opacity: 0.4;
        cursor: not-allowed;
        pointer-events: none;
    }

    .btn:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
        box-shadow: var(--shadow-focus-contrast);
    }

    /* === Size: regular === */
    .btn-regular {
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- Button height target: 32px */
        padding: 7px 20px;
        font-size: var(--font-size-md);
    }

    /* === Size: mini === */
    .btn-mini {
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- Mini button height target: 22px */
        padding: 3px 12px;
        font-size: var(--font-size-sm);
        border-radius: var(--radius-sm);
    }

    /* === Variant: primary === */
    .btn-primary {
        background: var(--color-accent);
        color: var(--color-accent-fg);
        border: none;
    }

    .btn-primary:hover:not(:disabled) {
        background: var(--color-accent-hover);
    }

    /* === Variant: secondary === */
    .btn-secondary {
        background: transparent;
        color: var(--color-text-secondary);
        border: 1px solid var(--color-border);
    }

    .btn-secondary:hover:not(:disabled) {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    /* === Variant: danger === */
    .btn-danger {
        background: transparent;
        color: var(--color-error-text);
        border: 1px solid var(--color-error);
    }

    .btn-danger:hover:not(:disabled) {
        background: color-mix(in srgb, var(--color-error), transparent 90%);
    }
</style>
