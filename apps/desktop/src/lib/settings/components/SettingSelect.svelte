<script lang="ts">
    import { Select, createListCollection, type SelectValueChangeDetails } from '@ark-ui/svelte/select'
    import { getSetting, setSetting, getSettingDefinition, type SettingId, type SettingsValues } from '$lib/settings'
    import type { EnumOption } from '$lib/settings/types'

    interface Props {
        id: SettingId
        disabled?: boolean
    }

    const { id, disabled = false }: Props = $props()

    const definition = getSettingDefinition(id)
    const options = definition?.constraints?.options ?? []
    const allowCustom = definition?.constraints?.allowCustom ?? false

    let value = $state(getSetting(id))
    let showCustomInput = $state(false)
    let customValue = $state('')

    // Check if current value is a custom value (not in options)
    $effect(() => {
        const isCustom = !options.some((o) => o.value === value)
        if (isCustom && allowCustom) {
            showCustomInput = true
            customValue = String(value)
        }
    })

    // Create the collection for the select
    const allItems = allowCustom ? [...options, { value: '__custom__', label: 'Custom...' }] : options
    const collection = createListCollection({
        items: allItems,
        itemToString: (item: EnumOption) => item.label,
        itemToValue: (item: EnumOption) => String(item.value),
    })

    async function handleValueChange(details: SelectValueChangeDetails<EnumOption>) {
        const newValue = details.value[0]
        if (newValue === '__custom__') {
            showCustomInput = true
            return
        }

        showCustomInput = false
        // Convert back to number if needed
        const option = options.find((o) => String(o.value) === newValue)
        const actualValue = option ? option.value : newValue
        value = actualValue
        await setSetting(id, actualValue as SettingsValues[typeof id])
    }

    async function handleCustomSubmit() {
        const numValue = Number(customValue)
        if (!isNaN(numValue)) {
            value = numValue
            await setSetting(id, numValue as SettingsValues[typeof id])
        }
    }
</script>

<div class="select-wrapper">
    {#if showCustomInput}
        <div class="custom-input-wrapper">
            <input
                type="number"
                class="custom-input"
                bind:value={customValue}
                onblur={handleCustomSubmit}
                onkeydown={(e) => e.key === 'Enter' && handleCustomSubmit()}
                min={definition?.constraints?.customMin}
                max={definition?.constraints?.customMax}
                {disabled}
            />
            <button class="back-to-select" onclick={() => (showCustomInput = false)} type="button"> ↩ </button>
        </div>
    {:else}
        <Select.Root {collection} value={[String(value)]} onValueChange={handleValueChange} {disabled}>
            <Select.Control>
                <Select.Trigger class="select-trigger">
                    <Select.ValueText placeholder="Select..." />
                    <Select.Indicator class="select-indicator">▼</Select.Indicator>
                </Select.Trigger>
            </Select.Control>
            <Select.Positioner>
                <Select.Content class="select-content">
                    {#each options as option}
                        <Select.Item item={option} class="select-item">
                            <Select.ItemText>
                                {option.label}
                                {#if option.description}
                                    <span class="option-description"> — {option.description}</span>
                                {/if}
                            </Select.ItemText>
                            <Select.ItemIndicator class="item-indicator">✓</Select.ItemIndicator>
                        </Select.Item>
                    {/each}
                    {#if allowCustom}
                        <Select.Item item={{ value: '__custom__', label: 'Custom...' }} class="select-item">
                            <Select.ItemText>Custom...</Select.ItemText>
                        </Select.Item>
                    {/if}
                </Select.Content>
            </Select.Positioner>
            <Select.HiddenSelect />
        </Select.Root>
    {/if}
</div>

<style>
    .select-wrapper {
        min-width: 180px;
    }

    .custom-input-wrapper {
        display: flex;
        gap: var(--spacing-xs);
    }

    .custom-input {
        width: 100px;
        padding: var(--spacing-xs) var(--spacing-sm);
        border: 1px solid var(--color-border);
        border-radius: 4px;
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
    }

    .custom-input:focus {
        outline: none;
        border-color: var(--color-accent);
    }

    .back-to-select {
        padding: var(--spacing-xs) var(--spacing-sm);
        border: 1px solid var(--color-border);
        border-radius: 4px;
        background: var(--color-bg-secondary);
        color: var(--color-text-secondary);
        cursor: pointer;
    }

    .back-to-select:hover {
        background: var(--color-bg-tertiary);
    }

    :global(.select-trigger) {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) var(--spacing-sm);
        min-width: 180px;
        border: 1px solid var(--color-border);
        border-radius: 4px;
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        cursor: pointer;
    }

    :global(.select-trigger:hover) {
        border-color: var(--color-border-primary);
    }

    :global(.select-trigger[data-disabled]) {
        cursor: not-allowed;
        opacity: 0.5;
    }

    :global(.select-indicator) {
        font-size: 10px;
        color: var(--color-text-muted);
    }

    :global(.select-content) {
        background: var(--color-bg-primary);
        border: 1px solid var(--color-border);
        border-radius: 4px;
        box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
        padding: var(--spacing-xs) 0;
        z-index: 100;
        max-height: 300px;
        overflow-y: auto;
    }

    :global(.select-item) {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: var(--spacing-xs) var(--spacing-sm);
        cursor: pointer;
        font-size: var(--font-size-sm);
    }

    :global(.select-item:hover),
    :global(.select-item[data-highlighted]) {
        background: var(--color-bg-hover);
    }

    :global(.select-item[data-state='checked']) {
        background: var(--color-accent);
        color: white;
    }

    :global(.item-indicator) {
        color: var(--color-accent);
    }

    :global(.select-item[data-state='checked'] .item-indicator) {
        color: white;
    }

    .option-description {
        color: var(--color-text-muted);
        font-size: var(--font-size-xs);
    }
</style>
