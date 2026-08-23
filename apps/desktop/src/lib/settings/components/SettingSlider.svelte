<script lang="ts">
    /**
     * Registry-driven slider row: reads bounds, stops, and the default from the registry, and
     * writes through `setSetting`. The control itself is `$lib/ui/Slider`.
     *
     * There's no paired number field. The live readout is a label, so the value can only be set
     * by dragging, which keeps the row honest about being a coarse choice. `sliderStops` do
     * double duty as the tick marks and the magnetic snap targets.
     *
     * ⚠️ **Two number spaces when `constraints.stopsAreDiscrete` is set.** The TRACK carries a
     * stop index and the STORE carries the stop's value, so every crossing goes through
     * `slider-stops.ts`: the seed, an external change, a commit, and the double-click reset.
     * ❌ The index is never stored — reordering the table would then silently change what every
     * user chose.
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
    import { nearestStopIndex, stopAt } from './slider-stops'
    import { onMount } from 'svelte'

    interface Props {
        id: SettingId
        disabled?: boolean
        /** Suffix for the readout ("%"), joined without a space. */
        unit?: string
        /**
         * Renders the stored value for the readout and for screen readers, replacing the
         * grouped-integer default. For a row whose number isn't a plain count (the Ask Cmdr
         * wake cadence, which reads as `30s` / `5m` / `2h` through `$lib/units`).
         */
        formatValue?: (value: number) => string
        /**
         * A RUNTIME maximum that wins over the registry `constraints.max`. For a control
         * whose ceiling isn't known until launch (the enrichment-parallelism slider, capped
         * at this machine's CPU count), the section fetches it and passes it here; the
         * registry keeps a static fallback for search and off-runtime rendering. Ignored in
         * discrete mode, where the stop table decides both ends.
         */
        maxOverride?: number
        /** Quiet captions under the track's two ends ("Faster" / "Smaller"). */
        endLabels?: [string, string]
    }

    const { id, disabled = false, unit = '', formatValue, maxOverride, endLabels }: Props = $props()

    const definition = getSettingDefinition(id)
    const label = definition?.label ?? id
    const step = definition?.constraints?.step ?? 1
    const sliderStops = definition?.constraints?.sliderStops ?? []
    const discrete = (definition?.constraints?.stopsAreDiscrete ?? false) && sliderStops.length > 0
    const defaultValue = getDefaultValue(id) as number

    /** The stored value a track position means. Identity outside discrete mode. */
    function toStored(track: number): number {
        return discrete ? stopAt(sliderStops, track) : track
    }

    /** Where a stored value sits on the track. Identity outside discrete mode. */
    function toTrack(stored: number): number {
        return discrete ? nearestStopIndex(sliderStops, stored) : stored
    }

    // The track's own bounds, ticks, and snap targets. In discrete mode they're all index
    // space, which is exactly what `ui/Slider` already assumes: `positionOf` is linear over
    // min/max, and it consumes ticks and snap targets in the same space.
    const trackMin = discrete ? 0 : (definition?.constraints?.min ?? 0)
    const trackMax = $derived(discrete ? sliderStops.length - 1 : (maxOverride ?? definition?.constraints?.max ?? 100))
    const trackStep = discrete ? 1 : step
    const trackStops = discrete ? sliderStops.map((_stop, index) => index) : sliderStops

    let value = $state(toTrack(getSetting(id) as number))

    // Subscribe to setting changes (for external resets).
    onMount(() => {
        return onSpecificSettingChange(id, (_id, newValue) => {
            value = toTrack(newValue as number)
        })
    })

    /**
     * The readout for a TRACK position. ⚠️ Both the visible label and `ariaValueText` are handed
     * the raw Ark value, so the mapping back has to happen here or a screen reader announces
     * "5" for a five-minute cadence.
     */
    function readout(track: number): string {
        const stored = toStored(track)
        return formatValue ? formatValue(stored) : `${formatInteger(stored)}${unit}`
    }

    /**
     * Whether the raw number is worth replacing for a screen reader. ⚠️ Discrete mode makes it
     * mandatory (the raw value is an index), which is why this isn't gated on `unit` alone.
     */
    const spokenValue = Boolean(unit) || Boolean(formatValue) || discrete

    function commit(track: number): void {
        const stored = toStored(track)
        value = toTrack(stored)
        setSetting(id, stored as SettingsValues[typeof id])
    }
</script>

<Slider
    {value}
    onChange={commit}
    min={trackMin}
    max={trackMax}
    step={trackStep}
    {disabled}
    ariaLabel={label}
    ariaValueText={spokenValue ? readout : undefined}
    ticks={trackStops}
    snapTargets={trackStops}
    {endLabels}
    valueLabel={readout(value)}
    onThumbDoubleClick={() => {
        commit(toTrack(defaultValue))
    }}
/>
