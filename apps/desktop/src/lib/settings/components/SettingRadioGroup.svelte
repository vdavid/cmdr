<script lang="ts">
    import { RadioGroup, type RadioGroupValueChangeDetails } from '@ark-ui/svelte/radio-group'
    import { getSetting, setSetting, getSettingDefinition, type SettingId, type SettingsValues } from '$lib/settings'
    import type { Snippet } from 'svelte'

    interface Props {
        id: SettingId
        disabled?: boolean
        customContent?: Snippet<[string]>
    }

    const { id, disabled = false, customContent }: Props = $props()

    const definition = getSettingDefinition(id)
    const options = definition?.constraints?.options ?? []

    let value = $state(String(getSetting(id)))

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

            <!-- Show custom content after the matching option -->
            {#if customContent && option.value === value}
                <div class="custom-content">
                    {@render customContent(value)}
                </div>
            {/if}
        {/each}
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
        cursor: pointer;
    }

    :global(.radio-item[data-disabled]) {
        cursor: not-allowed;
        opacity: 0.5;
    }

    :global(.radio-control) {
        width: 16px;
        height: 16px;
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

    :global(.radio-item:hover:not([data-disabled]) .radio-control) {
        border-color: var(--color-accent);
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
</style>
