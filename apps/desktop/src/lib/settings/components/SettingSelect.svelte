<script lang="ts">
    import { Select, createListCollection, type SelectValueChangeDetails } from '@ark-ui/svelte/select'
    import {
        getSetting,
        setSetting,
        getSettingDefinition,
        onSpecificSettingChange,
        type SettingId,
        type SettingsValues,
    } from '$lib/settings'
    import type { EnumOption } from '$lib/settings/types'
    import { onMount } from 'svelte'

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
    let customInputRef: HTMLInputElement | undefined = $state()

    // Focus custom input when it becomes visible (next microtask after render)
    $effect(() => {
        if (showCustomInput) {
            // Use setTimeout to wait for DOM to update after reactive state change
            setTimeout(() => {
                if (customInputRef) {
                    customInputRef.focus()
                    customInputRef.select()
                }
            }, 0)
        }
    })

    // Subscribe to setting changes (for external resets)
    onMount(() => {
        return onSpecificSettingChange(id, (_id, newValue) => {
            value = newValue
            // Reset custom input state if value is back to a standard option
            const isStandard = options.some((o) => o.value === newValue)
            if (isStandard) {
                showCustomInput = false
            }
        })
    })

    // Check if current value is a custom value (not in options)
    const isCustomValue = $derived(!options.some((o) => o.value === value))

    $effect(() => {
        if (isCustomValue && allowCustom) {
            customValue = String(value)
        }
    })

    // Create the collection for the select - include current custom value if set
    const collection = $derived.by(() => {
        const items: EnumOption[] = [...options]
        // If we have a custom value that's not in options, add it to the list
        if (allowCustom && isCustomValue) {
            items.push({ value: value as string | number, label: `Custom: ${String(value)}` })
        }
        if (allowCustom) {
            items.push({ value: '__custom__', label: 'Custom...' })
        }
        return createListCollection({
            items,
            itemToString: (item: EnumOption) => item.label,
            itemToValue: (item: EnumOption) => String(item.value),
        })
    })

    function handleValueChange(details: SelectValueChangeDetails<EnumOption>) {
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
        setSetting(id, actualValue as SettingsValues[typeof id])
    }

    // Track if "Custom..." is highlighted to hide selection from other items
    let customHighlighted = $state(false)

    // Handle highlight change (keyboard navigation) - immediately apply selection
    function handleHighlightChange(details: { highlightedValue: string | null }) {
        // Track if Custom... is highlighted
        customHighlighted = details.highlightedValue === '__custom__'

        if (details.highlightedValue && details.highlightedValue !== '__custom__') {
            const option = options.find((o) => String(o.value) === details.highlightedValue)
            if (option) {
                // Check if it's not the current custom value marker
                const actualValue = option.value
                value = actualValue
                setSetting(id, actualValue as SettingsValues[typeof id])
            } else if (allowCustom && isCustomValue && String(value) === details.highlightedValue) {
                // Re-selecting current custom value - do nothing
            }
        }
    }

    let wrapperRef: HTMLElement | null = $state(null)

    function handleCustomSubmit() {
        const numValue = Number(customValue)
        if (!isNaN(numValue)) {
            value = numValue
            setSetting(id, numValue as SettingsValues[typeof id])
            // Close custom input and return to dropdown
            showCustomInput = false
            // Focus the dropdown trigger after it renders
            requestAnimationFrame(() => {
                const trigger = wrapperRef?.querySelector('.select-trigger') as HTMLElement | null
                trigger?.focus()
            })
        }
    }
</script>

<div class="select-wrapper" bind:this={wrapperRef}>
    {#if showCustomInput}
        <div class="custom-input-wrapper">
            <input
                bind:this={customInputRef}
                type="number"
                class="custom-input"
                bind:value={customValue}
                onblur={handleCustomSubmit}
                onkeydown={(e) => {
                    if (e.key === 'Enter') handleCustomSubmit()
                }}
                placeholder="Enter custom value"
                min={definition?.constraints?.customMin}
                max={definition?.constraints?.customMax}
                {disabled}
            />
            <button class="back-to-select" onclick={() => (showCustomInput = false)} type="button"> ↩ </button>
        </div>
    {:else}
        <Select.Root
            {collection}
            value={[String(value)]}
            onValueChange={handleValueChange}
            onHighlightChange={handleHighlightChange}
            {disabled}
        >
            <Select.Control>
                <Select.Trigger class="select-trigger">
                    <Select.ValueText placeholder="Select..." />
                    <Select.Indicator class="select-indicator">▼</Select.Indicator>
                </Select.Trigger>
            </Select.Control>
            <Select.Positioner>
                <Select.Content
                    class={`select-content${customHighlighted ? ' custom-highlighted' : ''}`}
                    onkeydown={(e: KeyboardEvent) => {
                        if (e.key === 'Escape') {
                            e.stopPropagation()
                        }
                    }}
                >
                    {#each options as option (option.value)}
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
                    {#if allowCustom && isCustomValue}
                        <Select.Item
                            item={{ value: value as string | number, label: `Custom: ${String(value)}` }}
                            class="select-item"
                        >
                            <Select.ItemText>Custom: {String(value)}</Select.ItemText>
                            <Select.ItemIndicator class="item-indicator">✓</Select.ItemIndicator>
                        </Select.Item>
                    {/if}
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
        border-radius: var(--radius-sm);
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
    }

    /* Hide native spinners - users can use keyboard up/down arrows */
    .custom-input::-webkit-inner-spin-button,
    .custom-input::-webkit-outer-spin-button {
        -webkit-appearance: none;
        margin: 0;
    }

    .custom-input[type='number'] {
        appearance: textfield;
        -moz-appearance: textfield;
    }

    .custom-input:focus {
        outline: none;
        border-color: var(--color-accent);
        box-shadow: var(--shadow-focus);
    }

    .back-to-select {
        padding: var(--spacing-xs) var(--spacing-sm);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-bg-secondary);
        color: var(--color-text-secondary);
        cursor: default;
    }

    :global(.select-trigger) {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) var(--spacing-sm);
        min-width: 180px;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        cursor: default;
    }

    :global(.select-trigger[data-disabled]) {
        cursor: not-allowed;
        opacity: 0.5;
    }

    :global(.select-trigger:focus-visible) {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
        box-shadow: var(--shadow-focus);
    }

    :global(.select-indicator) {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    :global(.select-content) {
        background: var(--color-bg-primary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        box-shadow: var(--shadow-md);
        padding: var(--spacing-xs) 0;
        z-index: var(--z-dropdown);
        max-height: 300px;
        overflow-y: auto;
        /* Ensure consistent width regardless of content */
        min-width: 180px;
        width: max-content;
        /* No outline on dropdown content */
        outline: none;
    }

    :global(.select-content:focus),
    :global(.select-content:focus-visible) {
        outline: none;
    }

    :global(.select-item) {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) var(--spacing-sm);
        cursor: default;
        font-size: var(--font-size-sm);
        /* No outline on items */
        outline: none;
    }

    /* No hover visual indication - only checked state matters */
    :global(.select-item:hover) {
        background: transparent;
    }

    /* Highlighted item (keyboard navigation) - same as checked for immediate feedback */
    :global(.select-item[data-highlighted]) {
        background: var(--color-accent);
        color: white;
    }

    :global(.select-item[data-state='checked']) {
        background: var(--color-accent);
        color: white;
    }

    :global(.select-item[data-state='checked']:hover),
    :global(.select-item[data-highlighted]:hover) {
        background: var(--color-accent-hover);
    }

    /* Remove any focus outline from items */
    :global(.select-item:focus),
    :global(.select-item:focus-visible) {
        outline: none;
    }

    :global(.item-indicator) {
        /* Always reserve space for the checkmark to prevent layout shift */
        min-width: 1em;
        text-align: center;
        color: var(--color-accent);
        visibility: hidden;
    }

    :global(.select-item[data-state='checked'] .item-indicator),
    :global(.select-item[data-highlighted] .item-indicator) {
        visibility: visible;
        color: white;
    }

    /* When Custom... is highlighted, hide the checked state from other items */
    :global(.custom-highlighted .select-item[data-state='checked']:not([data-highlighted])) {
        background: transparent;
        color: var(--color-text-primary);
    }

    :global(.custom-highlighted .select-item[data-state='checked']:not([data-highlighted]) .item-indicator) {
        visibility: hidden;
    }

    .option-description {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
    }
</style>
