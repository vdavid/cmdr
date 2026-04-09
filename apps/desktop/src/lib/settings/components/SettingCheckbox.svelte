<script lang="ts">
    import { Checkbox } from '@ark-ui/svelte/checkbox'
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

    function handleChange(details: { checked: boolean | 'indeterminate' }) {
        const value = details.checked === true
        checked = value
        setSetting(id, value as SettingsValues[typeof id])
    }
</script>

<Checkbox.Root {checked} onCheckedChange={handleChange} {disabled} aria-label={label}>
    <Checkbox.Control class="checkbox-control">
        <Checkbox.Indicator class="checkbox-indicator">
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none" aria-hidden="true">
                <path
                    d="M2.5 6L5 8.5L9.5 3.5"
                    stroke="currentColor"
                    stroke-width="1.5"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                />
            </svg>
        </Checkbox.Indicator>
    </Checkbox.Control>
    <Checkbox.HiddenInput />
</Checkbox.Root>

<style>
    :global(.checkbox-control) {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 16px;
        height: 16px;
        background: var(--color-bg-tertiary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-xs);
        cursor: default;
        transition:
            background-color var(--transition-base),
            border-color var(--transition-base);
    }

    :global(.checkbox-control[data-state='checked']) {
        background: var(--color-accent);
        border-color: var(--color-accent);
    }

    :global(.checkbox-control[data-state='checked']:hover) {
        background: var(--color-accent-hover);
        border-color: var(--color-accent-hover);
    }

    :global(.checkbox-control:hover) {
        border-color: var(--color-border-strong);
    }

    :global(.checkbox-control[data-disabled]) {
        cursor: not-allowed;
        opacity: 0.5;
    }

    :global(.checkbox-indicator) {
        display: flex;
        align-items: center;
        justify-content: center;
        color: white;
    }

    /* Ark UI uses data-focus attribute when the hidden input is focused */
    :global(.checkbox-control[data-focus]) {
        outline: 2px solid var(--color-accent);
        outline-offset: 2px;
        box-shadow: var(--shadow-focus);
    }
</style>
