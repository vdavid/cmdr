<script lang="ts">
    import { ToggleGroup } from '@ark-ui/svelte/toggle-group'
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

    const definition = getSettingDefinition(id)
    const options = definition?.constraints?.options ?? []

    let value = $state([String(getSetting(id))])

    // Subscribe to setting changes (for external resets)
    onMount(() => {
        return onSpecificSettingChange(id, (_id, newValue) => {
            value = [String(newValue)]
        })
    })

    function handleValueChange(details: { value: string[] }) {
        if (details.value.length === 0) return // Don't allow deselecting all

        const newValue = details.value[0]
        value = [newValue]
        setSetting(id, newValue as SettingsValues[typeof id])
    }
</script>

<ToggleGroup.Root {value} onValueChange={handleValueChange} {disabled}>
    {#each options as option (option.value)}
        <ToggleGroup.Item value={String(option.value)} class="toggle-item" {disabled}>
            {option.label}
        </ToggleGroup.Item>
    {/each}
</ToggleGroup.Root>

<style>
    :global([data-scope='toggle-group'][data-part='root']) {
        display: inline-flex;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        overflow: hidden;
    }

    :global(.toggle-item) {
        padding: var(--spacing-xs) var(--spacing-md);
        border: none;
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        cursor: default;
        transition: all var(--transition-base);
        border-right: 1px solid var(--color-border);
    }

    :global(.toggle-item:last-child) {
        border-right: none;
    }

    :global(.toggle-item[data-state='on']) {
        background: var(--color-accent);
        color: var(--color-accent-fg);
    }

    :global(.toggle-item[data-state='on']:hover) {
        background: var(--color-accent-hover);
    }

    :global(.toggle-item[data-disabled]) {
        cursor: not-allowed;
        opacity: 0.5;
    }

    :global(.toggle-item:focus-visible) {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
        box-shadow: var(--shadow-focus);
        z-index: 1;
    }
</style>
