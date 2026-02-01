<script lang="ts">
    import { Switch } from '@ark-ui/svelte/switch'
    import { getSetting, setSetting, onSpecificSettingChange, type SettingId, type SettingsValues } from '$lib/settings'
    import { onMount } from 'svelte'

    interface Props {
        id: SettingId
        disabled?: boolean
    }

    const { id, disabled = false }: Props = $props()

    let checked = $state(getSetting(id) as boolean)

    // Subscribe to setting changes (for external resets)
    onMount(() => {
        return onSpecificSettingChange(id, (_id, newValue) => {
            checked = newValue as boolean
        })
    })

    function handleChange(details: { checked: boolean }) {
        checked = details.checked
        setSetting(id, details.checked as SettingsValues[typeof id])
    }
</script>

<Switch.Root {checked} onCheckedChange={handleChange} {disabled}>
    <Switch.Control class="switch-control">
        <Switch.Thumb class="switch-thumb" />
    </Switch.Control>
    <Switch.HiddenInput />
</Switch.Root>

<style>
    :global(.switch-control) {
        display: inline-flex;
        align-items: center;
        width: 36px;
        height: 20px;
        background: var(--color-bg-tertiary);
        border-radius: 10px;
        padding: 2px;
        cursor: default;
        transition: background-color 0.15s;
    }

    :global(.switch-control[data-state='checked']) {
        background: var(--color-accent);
    }

    :global(.switch-control[data-disabled]) {
        cursor: not-allowed;
        opacity: 0.5;
    }

    :global(.switch-thumb) {
        width: 16px;
        height: 16px;
        background: white;
        border-radius: 50%;
        transition: transform 0.15s;
        box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
    }

    :global(.switch-control[data-state='checked'] .switch-thumb) {
        transform: translateX(16px);
    }

    /* Ark UI uses data-focus attribute when the hidden input is focused */
    :global(.switch-control[data-focus]) {
        outline: 2px solid color-mix(in srgb, var(--color-accent) 70%, black);
        outline-offset: 2px;
        box-shadow: 0 0 0 4px rgba(77, 163, 255, 0.3);
    }
</style>
