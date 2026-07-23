<script lang="ts">
    import Switch from '$lib/ui/Switch.svelte'
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

    function handleChange(next: boolean) {
        checked = next
        setSetting(id, next as SettingsValues[typeof id])
    }
</script>

<Switch {checked} onCheckedChange={handleChange} {disabled} ariaLabel={label} />
