<script lang="ts">
    import { NumberInput, type NumberInputValueChangeDetails } from '@ark-ui/svelte/number-input'
    import {
        getSetting,
        setSetting,
        getSettingDefinition,
        onSpecificSettingChange,
        type SettingId,
        type SettingsValues,
    } from '$lib/settings'
    import { onMount } from 'svelte'

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

    // Subscribe to setting changes (for external resets)
    onMount(() => {
        return onSpecificSettingChange(id, (_id, newValue) => {
            value = newValue as number
        })
    })

    function handleChange(details: NumberInputValueChangeDetails) {
        const newValue = Math.min(max, Math.max(min, details.valueAsNumber))
        value = newValue
        setSetting(id, newValue as SettingsValues[typeof id])
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
        border-radius: var(--radius-sm);
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
        cursor: default;
        font-size: var(--font-size-md);
        font-weight: 500;
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

    :global(.number-control:focus-within) {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
        box-shadow: var(--shadow-focus);
    }

    :global(.number-btn:focus-visible) {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
        box-shadow: var(--shadow-focus);
        z-index: 1;
    }

    :global(.number-input[data-disabled]) {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .unit {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
    }
</style>
