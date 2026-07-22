<script lang="ts">
    import {
        getSetting,
        setSetting,
        getSettingDefinition,
        onSpecificSettingChange,
        type SettingId,
        type SettingsValues,
    } from '$lib/settings'
    import Checkbox from '$lib/ui/Checkbox.svelte'
    import { onMount } from 'svelte'

    interface Props {
        id: SettingId
        disabled?: boolean
    }

    const { id, disabled = false }: Props = $props()
    const label = getSettingDefinition(id)?.label ?? id

    let checked = $state(getSetting(id) as boolean)

    // Subscribe to setting changes (for external resets)
    onMount(() => {
        return onSpecificSettingChange(id, (_id, newValue) => {
            checked = newValue as boolean
        })
    })

    function handleChange(value: boolean) {
        checked = value
        setSetting(id, value as SettingsValues[typeof id])
    }
</script>

<Checkbox {checked} {disabled} ariaLabel={label} onCheckedChange={handleChange} />
