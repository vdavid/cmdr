<script lang="ts">
    import RadioGroup, { type RadioItem } from '$lib/ui/RadioGroup.svelte'
    import {
        getSetting,
        setSetting,
        getSettingDefinition,
        onSpecificSettingChange,
        type SettingId,
        type SettingsValues,
    } from '$lib/settings'
    import type { Snippet } from 'svelte'
    import { onMount } from 'svelte'

    interface Props {
        id: SettingId
        disabled?: boolean
        customContent?: Snippet<[string]>
        /** Control rendered on the same line as one option; see `lib/ui/RadioGroup`. */
        itemTrailing?: Snippet<[string]>
    }

    const { id, disabled = false, customContent, itemTrailing }: Props = $props()

    const definition = getSettingDefinition(id)
    const label = definition?.label ?? id
    const options = definition?.constraints?.options ?? []

    const items: RadioItem[] = options.map((option) => ({
        value: String(option.value),
        label: option.label,
        description: option.description,
    }))

    let value = $state(String(getSetting(id)))

    // Subscribe to setting changes (for external resets)
    onMount(() => {
        return onSpecificSettingChange(id, (_id, newValue) => {
            value = String(newValue)
        })
    })

    function handleValueChange(newValue: string) {
        setSetting(id, newValue as SettingsValues[typeof id])
    }
</script>

<RadioGroup {items} bind:value {disabled} ariaLabel={label} onValueChange={handleValueChange} {itemTrailing}>
    {#snippet footer(currentValue: string)}
        {#if customContent}
            <!-- Custom content rendered at end, visible only when 'custom' is selected -->
            <div class="custom-content" class:hidden={currentValue !== 'custom'}>
                {@render customContent(currentValue)}
            </div>
        {/if}
    {/snippet}
</RadioGroup>

<style>
    .custom-content {
        margin-left: var(--spacing-xl);
        margin-top: var(--spacing-xs);
        margin-bottom: var(--spacing-sm);
    }

    .custom-content.hidden {
        visibility: hidden;
        height: 0;
        margin: 0;
        overflow: hidden;
    }
</style>
