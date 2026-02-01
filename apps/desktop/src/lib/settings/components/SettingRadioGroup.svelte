<script lang="ts">
    import { RadioGroup, type RadioGroupValueChangeDetails } from '@ark-ui/svelte/radio-group'
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
    }

    const { id, disabled = false, customContent }: Props = $props()

    const definition = getSettingDefinition(id)
    const options = definition?.constraints?.options ?? []

    let value = $state(String(getSetting(id)))

    // Subscribe to setting changes (for external resets)
    onMount(() => {
        return onSpecificSettingChange(id, (_id, newValue) => {
            value = String(newValue)
        })
    })

    function handleValueChange(details: RadioGroupValueChangeDetails) {
        if (details.value) {
            value = details.value
            setSetting(id, details.value as SettingsValues[typeof id])
        }
    }
</script>

<RadioGroup.Root {value} onValueChange={handleValueChange} {disabled}>
    <div class="radio-group">
        {#each options as option (option.value)}
            <RadioGroup.Item value={String(option.value)} class="radio-item" {disabled}>
                <RadioGroup.ItemControl class="radio-control" />
                <RadioGroup.ItemText class="radio-text">
                    <span class="radio-label">{option.label}</span>
                    {#if option.description}
                        <span class="radio-description">{option.description}</span>
                    {/if}
                </RadioGroup.ItemText>
                <RadioGroup.ItemHiddenInput />
            </RadioGroup.Item>
        {/each}
        <!-- Custom content rendered at end, visible only when 'custom' is selected -->
        {#if customContent}
            <div class="custom-content" class:hidden={value !== 'custom'}>
                {@render customContent(value)}
            </div>
        {/if}
    </div>
</RadioGroup.Root>

<style>
    .radio-group {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
    }

    :global(.radio-item) {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) 0;
        cursor: default;
    }

    :global(.radio-item[data-disabled]) {
        cursor: not-allowed;
        opacity: 0.5;
    }

    :global(.radio-control) {
        width: 16px;
        height: 16px;
        min-width: 16px;
        min-height: 16px;
        border: 2px solid var(--color-border-primary);
        border-radius: 50%;
        background: var(--color-bg-primary);
        flex-shrink: 0;
        margin-top: 2px;
        transition: all 0.15s;
    }

    :global(.radio-control[data-state='checked']) {
        border-color: var(--color-accent);
        background: var(--color-accent);
        box-shadow: inset 0 0 0 3px var(--color-bg-primary);
    }

    /* Ark UI uses data-focus attribute when the hidden input is focused */
    :global(.radio-item[data-focus]) {
        outline: 2px solid color-mix(in srgb, var(--color-accent) 70%, black);
        outline-offset: 2px;
        border-radius: 4px;
        box-shadow: 0 0 0 4px rgba(77, 163, 255, 0.3);
    }

    :global(.radio-text) {
        display: flex;
        flex-direction: column;
        gap: 2px;
    }

    .radio-label {
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
    }

    .radio-description {
        color: var(--color-text-muted);
        font-size: var(--font-size-xs);
    }

    .custom-content {
        margin-left: 24px;
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
