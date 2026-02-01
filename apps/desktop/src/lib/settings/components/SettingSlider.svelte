<script lang="ts">
    import { Slider, type SliderValueChangeDetails } from '@ark-ui/svelte/slider'
    import { NumberInput, type NumberInputValueChangeDetails } from '@ark-ui/svelte/number-input'
    import {
        getSetting,
        setSetting,
        getSettingDefinition,
        getDefaultValue,
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
    const max = definition?.constraints?.max ?? 100
    const step = definition?.constraints?.step ?? 1
    const sliderStops = definition?.constraints?.sliderStops ?? []
    const defaultValue = getDefaultValue(id) as number

    let value = $state(getSetting(id) as number)

    // Subscribe to setting changes (for external resets)
    onMount(() => {
        return onSpecificSettingChange(id, (_id, newValue) => {
            value = newValue as number
        })
    })

    function handleSliderChange(details: SliderValueChangeDetails) {
        const newValue = details.value[0]
        // Snap to nearest slider stop if close
        let snappedValue = newValue
        if (sliderStops.length > 0) {
            const closest = sliderStops.reduce((prev, curr) =>
                Math.abs(curr - newValue) < Math.abs(prev - newValue) ? curr : prev,
            )
            if (Math.abs(closest - newValue) < step * 2) {
                snappedValue = closest
            }
        }
        value = snappedValue
        setSetting(id, snappedValue as SettingsValues[typeof id])
    }

    // Double-click on slider thumb resets to default
    function handleThumbDblClick() {
        value = defaultValue
        setSetting(id, defaultValue as SettingsValues[typeof id])
    }

    function handleInputChange(details: NumberInputValueChangeDetails) {
        // Handle NaN (empty input) - treat as minimum value
        if (isNaN(details.valueAsNumber)) {
            return // Don't update until blur
        }
        const newValue = Math.min(max, Math.max(min, details.valueAsNumber))
        value = newValue
        setSetting(id, newValue as SettingsValues[typeof id])
    }

    // On blur, if value is NaN or out of range, reset to min
    function handleInputBlur() {
        if (isNaN(value) || value < min) {
            value = min
            setSetting(id, min as SettingsValues[typeof id])
        }
    }
</script>

<div class="slider-wrapper">
    <Slider.Root value={[value]} onValueChange={handleSliderChange} {min} {max} {step} {disabled} class="slider-root">
        <Slider.Control class="slider-control">
            <Slider.Track class="slider-track">
                <Slider.Range class="slider-range" />
            </Slider.Track>
            <Slider.Thumb index={0} class="slider-thumb" ondblclick={handleThumbDblClick} />
            <!-- Show tick marks for slider stops - inside Control for proper positioning -->
            {#if sliderStops.length > 0}
                <div class="slider-ticks">
                    {#each sliderStops as stop (stop)}
                        <span
                            class="slider-tick"
                            class:active={value === stop}
                            style="left: {((stop - min) / (max - min)) * 100}%"
                        ></span>
                    {/each}
                </div>
            {/if}
        </Slider.Control>
    </Slider.Root>

    <NumberInput.Root value={String(value)} onValueChange={handleInputChange} {min} {max} {step} {disabled}>
        <NumberInput.Control class="number-control">
            <NumberInput.Input class="number-input" onblur={handleInputBlur} />
        </NumberInput.Control>
    </NumberInput.Root>

    {#if unit}
        <span class="unit">{unit}</span>
    {/if}
</div>

<style>
    .slider-wrapper {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        min-width: 280px;
    }

    /* The root needs explicit sizing for Ark UI slider to work */
    :global(.slider-root) {
        flex: 1;
        min-width: 120px;
    }

    :global(.slider-control) {
        position: relative;
        display: flex;
        align-items: center;
        height: 20px;
        width: 100%;
    }

    :global(.slider-track) {
        flex: 1;
        height: 4px;
        background: var(--color-bg-tertiary);
        border-radius: 2px;
        position: relative;
    }

    :global(.slider-range) {
        height: 100%;
        background: var(--color-accent);
        border-radius: 2px;
    }

    :global(.slider-thumb) {
        width: 16px;
        height: 16px;
        background: white;
        border: 2px solid var(--color-accent);
        border-radius: 50%;
        cursor: default;
        box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
        /* Ensure thumb is above tick marks */
        z-index: 2;
        position: relative;
    }

    :global(.slider-thumb[data-disabled]) {
        cursor: not-allowed;
    }

    :global(.slider-thumb:focus-visible) {
        outline: 2px solid color-mix(in srgb, var(--color-accent) 80%, black);
        outline-offset: 2px;
    }

    .slider-ticks {
        position: absolute;
        left: 0;
        right: 0;
        top: 50%;
        transform: translateY(-50%);
        height: 4px;
        pointer-events: none;
        /* Below the thumb (Ark UI sets z-index: 1 on thumb) */
        z-index: 0;
    }

    .slider-tick {
        position: absolute;
        width: 2px;
        height: 8px;
        background: var(--color-border);
        transform: translate(-50%, -50%);
        top: 50%;
    }

    .slider-tick.active {
        background: var(--color-accent);
    }

    :global(.number-control) {
        /* Remove any wrapper styling - let the input handle it */
        display: contents;
    }

    :global(.number-input) {
        width: 70px;
        padding: var(--spacing-xs) var(--spacing-sm);
        border: 1px solid var(--color-border);
        border-radius: 4px;
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        text-align: right;
    }

    :global(.number-input:focus) {
        outline: none;
        border-color: var(--color-accent);
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
