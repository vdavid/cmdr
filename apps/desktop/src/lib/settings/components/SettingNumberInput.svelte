<script lang="ts">
    import { NumberInput, type NumberInputValueChangeDetails } from '@ark-ui/svelte/number-input'
    import { getSetting, setSetting, getSettingDefinition, type SettingId, type SettingsValues } from '$lib/settings'

    interface Props {
        id: SettingId
        disabled?: boolean
        unit?: string
    }

    const { id, disabled = false, unit = '' }: Props = $props()

    const definition = getSettingDefinition(id)
    const min = definition?.constraints?.min ?? 0
    const max = definition?.constraints?.max ?? 999999
    const step = definition?.constraints?.step ?? 1

    let value = $state(getSetting(id) as number)

    async function handleChange(details: NumberInputValueChangeDetails) {
        const newValue = Math.min(max, Math.max(min, details.valueAsNumber))
        value = newValue
        await setSetting(id, newValue as SettingsValues[typeof id])
    }
</script>

<div class="number-input-wrapper">
    <NumberInput.Root value={String(value)} onValueChange={handleChange} {min} {max} {step} {disabled}>
        <NumberInput.Control class="number-control">
            <NumberInput.DecrementTrigger class="number-btn">−</NumberInput.DecrementTrigger>
            <NumberInput.Input class="number-input" />
            <NumberInput.IncrementTrigger class="number-btn">+</NumberInput.IncrementTrigger>
        </NumberInput.Control>
    </NumberInput.Root>

    {#if unit}
        <span class="unit">{unit}</span>
    {/if}
</div>

<style>
    .number-input-wrapper {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    :global(.number-control) {
        display: flex;
        align-items: center;
        border: 1px solid var(--color-border);
        border-radius: 4px;
        overflow: hidden;
    }

    :global(.number-btn) {
        width: 28px;
        height: 28px;
        display: flex;
        align-items: center;
        justify-content: center;
        background: var(--color-bg-secondary);
        border: none;
        color: var(--color-text-primary);
        cursor: pointer;
        font-size: 14px;
        font-weight: 500;
    }

    :global(.number-btn:hover:not([data-disabled])) {
        background: var(--color-bg-tertiary);
    }

    :global(.number-btn[data-disabled]) {
        cursor: not-allowed;
        opacity: 0.5;
    }

    :global(.number-input) {
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

    :global(.number-input:focus) {
        outline: none;
    }

    :global(.number-input[data-disabled]) {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .unit {
        color: var(--color-text-muted);
        font-size: var(--font-size-sm);
    }
</style>
