<script lang="ts">
    import { NumberInput, type NumberInputValueChangeDetails } from '@ark-ui/svelte/number-input'
    import {
        getSetting,
        setSetting,
        getSettingDefinition,
        onSpecificSettingChange,
        msToDurationValue,
        durationValueToMs,
        type SettingId,
        type SettingsValues,
    } from '$lib/settings'
    import { onMount } from 'svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        id: SettingId
        disabled?: boolean
        /** Unit label shown after the input. For a `duration` setting it defaults to the
            setting's own unit (`ms` / `s` / `min` / …); pass this to override or to label a
            plain `number`. */
        unit?: string
    }

    const { id, disabled = false, unit = '' }: Props = $props()

    const definition = getSettingDefinition(id)
    const label = definition?.label ?? id

    // A `duration` setting stores milliseconds but is edited in a coarser unit (its
    // `constraints.unit`). We convert stored ms <-> the displayed value here so the store
    // stays in ms and callers never see the scaling. A plain `number` has no unit, so the
    // factor is 1 and every conversion is a passthrough.
    const durationUnit = definition?.type === 'duration' ? definition.constraints?.unit : undefined
    const displayUnit = unit || (durationUnit ?? '')

    // Bounds and step are expressed in the DISPLAY unit. Duration bounds come from
    // `minMs`/`maxMs` (scaled down); plain numbers use `min`/`max` directly.
    const min = durationUnit
        ? msToDurationValue(definition?.constraints?.minMs ?? 0, durationUnit)
        : (definition?.constraints?.min ?? 0)
    const max = durationUnit
        ? msToDurationValue(definition?.constraints?.maxMs ?? Number.MAX_SAFE_INTEGER, durationUnit)
        : (definition?.constraints?.max ?? 999999)
    const step = definition?.constraints?.step ?? 1

    // `value` is the DISPLAYED number (already in `displayUnit`).
    let value = $state(msToDurationValue(getSetting(id) as number, durationUnit))

    // Subscribe to setting changes (for external resets); the store speaks ms.
    onMount(() => {
        return onSpecificSettingChange(id, (_id, newValue) => {
            value = msToDurationValue(newValue as number, durationUnit)
        })
    })

    function handleChange(details: NumberInputValueChangeDetails) {
        const clampedDisplay = Math.min(max, Math.max(min, details.valueAsNumber))
        value = clampedDisplay
        setSetting(id, durationValueToMs(clampedDisplay, durationUnit) as SettingsValues[typeof id])
    }
</script>

<div class="number-input-wrapper">
    <NumberInput.Root value={String(value)} onValueChange={handleChange} {min} {max} {step} {disabled}>
        <NumberInput.Control class="number-control">
            <NumberInput.DecrementTrigger class="number-btn" aria-label={tString('settings.control.decrease', { label })}
                >−</NumberInput.DecrementTrigger
            >
            <NumberInput.Input class="number-input" aria-label={label} />
            <NumberInput.IncrementTrigger class="number-btn" aria-label={tString('settings.control.increase', { label })}
                >+</NumberInput.IncrementTrigger
            >
        </NumberInput.Control>
    </NumberInput.Root>

    {#if displayUnit}
        <span class="unit">{displayUnit}</span>
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
