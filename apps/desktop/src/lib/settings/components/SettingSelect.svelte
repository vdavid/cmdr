<script lang="ts">
    import {
        getSetting,
        setSetting,
        getSettingDefinition,
        onSpecificSettingChange,
        type SettingId,
        type SettingsValues,
    } from '$lib/settings'
    import type { EnumOption } from '$lib/settings/types'
    import Select, { type SelectItem } from '$lib/ui/Select.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { onMount } from 'svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        id: SettingId
        disabled?: boolean
    }

    const { id, disabled = false }: Props = $props()

    const definition = getSettingDefinition(id)
    const label = definition?.label ?? id
    const options = definition?.constraints?.options ?? []
    const allowCustom = definition?.constraints?.allowCustom ?? false

    // Sentinel value for the inline "Custom…" row. `ui/Select` never sees this as a real selection;
    // we intercept it in the value/highlight handlers and switch to the inline number input.
    const CUSTOM_VALUE = '__custom__'

    let value = $state(getSetting(id))
    let showCustomInput = $state(false)
    let customValue = $state('')
    let customInputRef: HTMLInputElement | undefined = $state()

    // Focus custom input when it becomes visible (next microtask after render).
    $effect(() => {
        if (showCustomInput) {
            // setTimeout (not tick()) is load-bearing: Ark UI's Select finishes its own close
            // animation on a microtask, and a same-tick focus call gets eaten by the returning
            // focus from the trigger. See settings/components/CLAUDE.md.
            setTimeout(() => {
                if (customInputRef) {
                    customInputRef.focus()
                    customInputRef.select()
                }
            }, 0)
        }
    })

    // Subscribe to setting changes (for external resets).
    onMount(() => {
        return onSpecificSettingChange(id, (_id, newValue) => {
            value = newValue
            // Reset custom input state if value is back to a standard option.
            const isStandard = options.some((o) => o.value === newValue)
            if (isStandard) {
                showCustomInput = false
            }
        })
    })

    // Check if current value is a custom value (not in options).
    const isCustomValue = $derived(!options.some((o) => o.value === value))

    $effect(() => {
        if (isCustomValue && allowCustom) {
            customValue = String(value)
        }
    })

    // Build the items array for `ui/Select`: standard options, the current custom value (if any),
    // then the "Custom…" sentinel row when `allowCustom`.
    const selectItems = $derived.by((): SelectItem[] => {
        const items: SelectItem[] = options.map((option: EnumOption) => ({
            value: String(option.value),
            label: option.label,
            description: option.description,
        }))
        if (allowCustom && isCustomValue) {
            items.push({
                value: String(value),
                label: tString('settings.control.customPrefix', { value: String(value) }),
            })
        }
        if (allowCustom) {
            items.push({ value: CUSTOM_VALUE, label: tString('settings.control.customOption') })
        }
        return items
    })

    function applyValue(newValue: string): void {
        const option = options.find((o) => String(o.value) === newValue)
        const actualValue = option ? option.value : newValue
        value = actualValue
        setSetting(id, actualValue as SettingsValues[typeof id])
    }

    function handleChange(newValue: string): void {
        if (newValue === CUSTOM_VALUE) {
            showCustomInput = true
            return
        }
        showCustomInput = false
        applyValue(newValue)
    }

    // Track if "Custom…" is highlighted so the content can hide the checked state on other items.
    let customHighlighted = $state(false)

    // Handle highlight change (keyboard navigation): immediately apply the highlighted selection.
    function handleHighlightChange(highlightedValue: string | null): void {
        customHighlighted = highlightedValue === CUSTOM_VALUE

        if (highlightedValue && highlightedValue !== CUSTOM_VALUE) {
            const option = options.find((o) => String(o.value) === highlightedValue)
            if (option) {
                applyValue(highlightedValue)
            }
            // Re-highlighting the current custom-value row is a no-op (no matching standard option).
        }
    }

    let wrapperRef: HTMLElement | null = $state(null)

    function handleCustomSubmit(): void {
        const numValue = Number(customValue)
        if (!isNaN(numValue)) {
            value = numValue
            setSetting(id, numValue as SettingsValues[typeof id])
            // Close custom input and return to dropdown.
            showCustomInput = false
            // Focus the dropdown trigger after it renders.
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
                placeholder={tString('settings.control.customValuePlaceholder')}
                min={definition?.constraints?.customMin}
                max={definition?.constraints?.customMax}
                {disabled}
            />
            <button
                class="back-to-select"
                onclick={() => (showCustomInput = false)}
                type="button"
                aria-label={tString('settings.control.backToPresetsAriaLabel')}
                use:tooltip={tString('settings.control.backToPresetsAriaLabel')}
            >
                <Icon name="corner-down-left" size={14} aria-hidden="true" />
            </button>
        </div>
    {:else}
        <Select
            items={selectItems}
            value={String(value)}
            onChange={handleChange}
            onHighlightChange={handleHighlightChange}
            contentClass={customHighlighted ? 'custom-highlighted' : ''}
            ariaLabel={label}
            portal
            {disabled}
        />
    {/if}
</div>

<style>
    .select-wrapper {
        min-width: 180px;
        width: 100%;
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

    /* Hide native spinners: users can use keyboard up/down arrows. */
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
        display: inline-flex;
        align-items: center;
        justify-content: center;
        padding: var(--spacing-xs) var(--spacing-sm);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-bg-secondary);
        color: var(--color-text-secondary);
        cursor: default;
    }
</style>
