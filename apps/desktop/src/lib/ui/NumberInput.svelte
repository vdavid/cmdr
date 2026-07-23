<script lang="ts">
    /**
     * Numeric field built on Ark UI's `NumberInput`: the house number input. Every numeric
     * control converges here, so the framed box, the steppers, and the focus ring live in one
     * place. Ark owns the spinbutton ARIA, keyboard stepping, and clamp-on-blur; we style it
     * and keep the value a `number` at the boundary (Ark speaks strings internally).
     */
    import { NumberInput, type NumberInputValueChangeDetails } from '@ark-ui/svelte/number-input'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        value: number
        /** Fires with an already-clamped number; an emptied field emits nothing until blur. */
        onChange: (value: number) => void
        min: number
        max: number
        step?: number
        disabled?: boolean
        /** Accessible name for the field; also names the steppers ("Increase {ariaLabel}"). */
        ariaLabel: string
        /** Quiet trailing unit ("px", "%", "s"). Presentational: the value stays unitless. */
        unit?: string
    }

    const { value, onChange, min, max, step = 1, disabled = false, ariaLabel, unit = '' }: Props = $props()

    function handleValueChange(details: NumberInputValueChangeDetails): void {
        // An emptied field parses as `NaN`. Swallow it rather than committing a broken
        // number: Ark's `clampValueOnBlur` restores a real value when focus leaves.
        if (isNaN(details.valueAsNumber)) return
        onChange(Math.min(max, Math.max(min, details.valueAsNumber)))
    }
</script>

<div class="ni-wrapper">
    <NumberInput.Root value={String(value)} onValueChange={handleValueChange} {min} {max} {step} {disabled}>
        <NumberInput.Control class="ni-control">
            <NumberInput.DecrementTrigger class="ni-btn" aria-label={tString('ui.numberInput.decrease', { label: ariaLabel })}
                >−</NumberInput.DecrementTrigger
            >
            <NumberInput.Input class="ni-input" aria-label={ariaLabel} />
            <NumberInput.IncrementTrigger class="ni-btn" aria-label={tString('ui.numberInput.increase', { label: ariaLabel })}
                >+</NumberInput.IncrementTrigger
            >
        </NumberInput.Control>
    </NumberInput.Root>

    {#if unit}
        <span class="ni-unit">{unit}</span>
    {/if}
</div>

<style>
    /* Every selector handed to an Ark part is `:global(...)`: Svelte 5 doesn't propagate this
       component's scoping hash through a `class` prop forwarded into a third-party component,
       so a scoped selector would whiff against the Ark-rendered DOM. The `ni-` prefix is this
       component's alone, which is what keeps the unscoping safe. */
    .ni-wrapper {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    :global(.ni-control) {
        display: flex;
        align-items: center;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        overflow: hidden;
    }

    :global(.ni-btn) {
        width: 28px;
        height: 28px;
        display: flex;
        align-items: center;
        justify-content: center;
        background: var(--color-bg-secondary);
        border: none;
        color: var(--color-text-primary);
        cursor: default;
        font-size: var(--font-size-md);
        font-weight: 500;
    }

    :global(.ni-btn[data-disabled]) {
        cursor: not-allowed;
        opacity: 0.5;
    }

    :global(.ni-input) {
        width: 70px;
        padding: var(--spacing-xs);
        border: none;
        border-left: 1px solid var(--color-border);
        border-right: 1px solid var(--color-border);
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        text-align: center;
    }

    :global(.ni-input:focus) {
        outline: none;
    }

    :global(.ni-control:focus-within) {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
        box-shadow: var(--shadow-focus);
    }

    :global(.ni-btn:focus-visible) {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
        box-shadow: var(--shadow-focus);
        z-index: 1;
    }

    :global(.ni-input[data-disabled]) {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .ni-unit {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
    }
</style>
