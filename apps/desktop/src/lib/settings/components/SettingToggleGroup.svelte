<script lang="ts">
    /**
     * `SettingToggleGroup` is a thin wrapper around the generic `lib/ui/ToggleGroup` primitive
     * with `semantics="toggles"`. It reads the setting definition from the registry, builds the
     * options array (applying optional per-value label overrides), and delegates rendering plus
     * keyboard handling to the shared component so Settings and the future Query mode chips share
     * one segmented-control look.
     *
     * Public API is unchanged: `{ id, disabled, labelOverrides }`.
     */
    import ToggleGroup, { type ToggleGroupOption } from '$lib/ui/ToggleGroup.svelte'
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
        /**
         * Optional per-value label overrides. Use when a button label must
         * reflect another reactive setting (for example, the kilobyte
         * casing on `listing.sizeUnit` swapping `kB` ↔ `KB` with binary/SI).
         * Keys are stringified option values; missing entries fall back to
         * the definition label.
         */
        labelOverrides?: Record<string, string>
    }

    const { id, disabled = false, labelOverrides }: Props = $props()

    const definition = getSettingDefinition(id)
    const label = definition?.label ?? id
    const definitionOptions = definition?.constraints?.options ?? []

    let value = $state(String(getSetting(id)))

    onMount(() => {
        return onSpecificSettingChange(id, (_id, newValue) => {
            value = String(newValue)
        })
    })

    const options = $derived<ToggleGroupOption[]>(
        definitionOptions.map((opt) => {
            const key = String(opt.value)
            return {
                value: key,
                label: labelOverrides?.[key] ?? opt.label,
                icon: opt.icon,
            }
        }),
    )

    function handleChange(next: string): void {
        value = next
        setSetting(id, next as SettingsValues[typeof id])
    }
</script>

<ToggleGroup
    semantics="toggles"
    {value}
    {options}
    onChange={handleChange}
    ariaLabel={label}
    {disabled}
/>
