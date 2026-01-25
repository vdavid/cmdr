<script lang="ts">
    import { Slider, type SliderValueChangeDetails } from '@ark-ui/svelte/slider'
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
    const max = definition?.constraints?.max ?? 100
    const step = definition?.constraints?.step ?? 1
    const sliderStops = definition?.constraints?.sliderStops ?? []

    let value = $state(getSetting(id) as number)

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

    function handleInputChange(details: NumberInputValueChangeDetails) {
        const newValue = Math.min(max, Math.max(min, details.valueAsNumber))
        value = newValue
        setSetting(id, newValue as SettingsValues[typeof id])
    }
</script>

<div class="slider-wrapper">
    <Slider.Root value={[value]} onValueChange={handleSliderChange} {min} {max} {step} {disabled}>
        <Slider.Control class="slider-control">
            <Slider.Track class="slider-track">
                <Slider.Range class="slider-range" />
            </Slider.Track>
            <Slider.Thumb index={0} class="slider-thumb" />
        </Slider.Control>
        <!-- Show tick marks for slider stops -->
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
    </Slider.Root>

    <NumberInput.Root value={String(value)} onValueChange={handleInputChange} {min} {max} {step} {disabled}>
        <NumberInput.Control class="number-control">
            <NumberInput.Input class="number-input" />
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

    :global(.slider-control) {
        position: relative;
        flex: 1;
        display: flex;
        align-items: center;
        height: 20px;
    }

    :global(.slider-track) {
        flex: 1;
        height: 4px;
        background: var(--color-bg-tertiary);
        border-radius: 2px;
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
        cursor: pointer;
        box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
    }

    :global(.slider-thumb:hover) {
        transform: scale(1.1);
    }

    :global(.slider-thumb[data-disabled]) {
        cursor: not-allowed;
    }

    .slider-ticks {
        position: absolute;
        width: 100%;
        height: 4px;
        top: 50%;
        transform: translateY(-50%);
        pointer-events: none;
    }

    .slider-tick {
        position: absolute;
        width: 2px;
        height: 8px;
        background: var(--color-border-primary);
        transform: translateX(-50%);
        top: -2px;
    }

    .slider-tick.active {
        background: var(--color-accent);
    }

    :global(.number-control) {
        width: 80px;
    }

    :global(.number-input) {
        width: 100%;
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
