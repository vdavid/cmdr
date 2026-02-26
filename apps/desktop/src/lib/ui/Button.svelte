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
        box-shadow: 0 0 0 4px rgba(0, 0, 0, 0.1);
    }

    @media (prefers-color-scheme: dark) {
        .btn:focus-visible {
            box-shadow: 0 0 0 4px rgba(255, 255, 255, 0.08);
        }
    }

    /* === Size: regular === */
    .btn-regular {
        padding: 7px 20px;
        font-size: var(--font-size-md);
    }

    /* === Size: mini === */
    .btn-mini {
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
        color: var(--color-error);
        border: 1px solid var(--color-error);
    }

    .btn-danger:hover:not(:disabled) {
        background: color-mix(in srgb, var(--color-error), transparent 90%);
    }
</style>
