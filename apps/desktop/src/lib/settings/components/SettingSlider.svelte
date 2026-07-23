<script lang="ts">
    /**
     * Registry-driven slider row: reads bounds, stops, and the default from the registry, and
     * writes through `setSetting`. The control itself is `$lib/ui/Slider`.
     *
     * There's no paired number field. The live readout is a label, so the value can only be set
     * by dragging, which keeps the row honest about being a coarse choice. `sliderStops` do
     * double duty as the tick marks and the magnetic snap targets.
     */
    import Slider from '$lib/ui/Slider.svelte'
    import {
        getSetting,
        setSetting,
        getSettingDefinition,
        getDefaultValue,
        onSpecificSettingChange,
        type SettingId,
        type SettingsValues,
    } from '$lib/settings'
    import { formatInteger } from '$lib/intl/number-format'
    import { onMount } from 'svelte'

    interface Props {
        id: SettingId
        disabled?: boolean
        /** Suffix for the readout ("%"), joined without a space. */
        unit?: string
        /**
         * A RUNTIME maximum that wins over the registry `constraints.max`. For a control
         * whose ceiling isn't known until launch (the enrichment-parallelism slider, capped
         * at this machine's CPU count), the section fetches it and passes it here; the
         * registry keeps a static fallback for search and off-runtime rendering.
         */
        maxOverride?: number
        /** Quiet captions under the track's two ends ("Faster" / "Smaller"). */
        endLabels?: [string, string]
    }

    const { id, disabled = false, unit = '', maxOverride, endLabels }: Props = $props()

    const definition = getSettingDefinition(id)
    const label = definition?.label ?? id
    const min = definition?.constraints?.min ?? 0
    const max = $derived(maxOverride ?? definition?.constraints?.max ?? 100)
    const step = definition?.constraints?.step ?? 1
    const sliderStops = definition?.constraints?.sliderStops ?? []
    const defaultValue = getDefaultValue(id) as number

    let value = $state(getSetting(id) as number)

    // Subscribe to setting changes (for external resets).
    onMount(() => {
        return onSpecificSettingChange(id, (_id, newValue) => {
            value = newValue as number
        })
    })

    function readout(n: number): string {
        return `${formatInteger(n)}${unit}`
    }

    function commit(next: number): void {
        value = next
        setSetting(id, next as SettingsValues[typeof id])
    }
</script>

<Slider
    {value}
    onChange={commit}
    {min}
    {max}
    {step}
    {disabled}
    ariaLabel={label}
    ariaValueText={unit ? readout : undefined}
    ticks={sliderStops}
    snapTargets={sliderStops}
    {endLabels}
    valueLabel={readout(value)}
    onThumbDoubleClick={() => {
        commit(defaultValue)
    }}
/>
