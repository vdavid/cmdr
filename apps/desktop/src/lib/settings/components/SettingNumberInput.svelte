<script lang="ts">
    /**
     * Registry-driven number row: reads bounds from the registry and writes through
     * `setSetting`. The control itself is `$lib/ui/NumberInput`.
     */
    import NumberInput from '$lib/ui/NumberInput.svelte'
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

    function handleChange(next: number): void {
        value = next
        setSetting(id, durationValueToMs(next, durationUnit) as SettingsValues[typeof id])
    }
</script>

<NumberInput
    {value}
    onChange={handleChange}
    {min}
    {max}
    {step}
    {disabled}
    ariaLabel={label}
    unit={displayUnit}
/>
