<script lang="ts" module>
    /**
     * One option in a `RadioGroup`. `value` is the stable identity (compared and emitted as a
     * string); `label` is the visible text; `description` renders as quieter text below the label
     * (used by settings' option descriptions); `disabled` greys out and blocks the single option.
     */
    export interface RadioItem {
        value: string
        label: string
        description?: string
        disabled?: boolean
    }
</script>

<script lang="ts">
    /**
     * Presentational, items-driven single-select radio group ("options") built on Ark UI's
     * `RadioGroup`. The house radio group: `SettingRadioGroup` and any raw `<input type="radio">`
     * group converge here so the styled control lives in one place. Ark owns keyboard a11y and ARIA
     * (`role="radiogroup"` with `role="radio"` items); we style the control dot and layout.
     */
    import { RadioGroup, type RadioGroupValueChangeDetails } from '@ark-ui/svelte/radio-group'
    import type { Snippet } from 'svelte'

    interface Props {
        /** Selected item's `value`. Empty string means nothing selected. Bindable. */
        value?: string
        items: RadioItem[]
        onValueChange?: (value: string) => void
        /** Group-level disable; short-circuits every option. */
        disabled?: boolean
        /** `vertical` stacks the options; `horizontal` lays them in a wrapping row. */
        orientation?: 'vertical' | 'horizontal'
        /** Accessible name for the group root. */
        ariaLabel?: string
        /**
         * Rendered after the items, receiving the current `value`. Preserves the "custom content when
         * a specific option is selected" feature; the caller decides visibility.
         */
        footer?: Snippet<[string]>
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let {
        value = $bindable(''),
        items,
        onValueChange,
        disabled = false,
        orientation = 'vertical',
        ariaLabel,
        footer,
    }: Props = $props()
    /* eslint-enable prefer-const */

    function handleValueChange(details: RadioGroupValueChangeDetails): void {
        if (details.value) {
            value = details.value
            onValueChange?.(details.value)
        }
    }
</script>

<RadioGroup.Root {value} onValueChange={handleValueChange} {disabled} aria-label={ariaLabel}>
    <div class="radio-group" class:horizontal={orientation === 'horizontal'}>
        {#each items as item (item.value)}
            <RadioGroup.Item value={item.value} class="radio-item" disabled={disabled || item.disabled}>
                <RadioGroup.ItemControl class="radio-control" />
                <RadioGroup.ItemText class="radio-text">
                    <span class="radio-label">{item.label}</span>
                    {#if item.description}
                        <span class="radio-description">{item.description}</span>
                    {/if}
                </RadioGroup.ItemText>
                <RadioGroup.ItemHiddenInput />
            </RadioGroup.Item>
        {/each}
        {#if footer}
            {@render footer(value)}
        {/if}
    </div>
</RadioGroup.Root>

<style>
    .radio-group {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
    }

    .radio-group.horizontal {
        flex-direction: row;
        flex-wrap: wrap;
        gap: var(--spacing-md);
    }

    :global(.radio-item) {
        display: flex;
        align-items: flex-start;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) 0;
        cursor: default;
        /* Contain each item's Ark `ItemHiddenInput` so it resolves against its
           own row, not the window shell. See `Switch.svelte`'s `.switch-root`
           for the full why (the shell-scroll-under-traffic-lights bug). */
        position: relative;
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
        border: 2px solid var(--color-border-strong);
        border-radius: var(--radius-full);
        background: var(--color-bg-primary);
        flex-shrink: 0;
        margin-top: var(--spacing-xxs);
        transition: all var(--transition-base);
    }

    :global(.radio-control[data-state='checked']) {
        border-color: var(--color-accent);
        background: var(--color-accent);
        box-shadow: inset 0 0 0 3px var(--color-bg-primary);
    }

    :global(.radio-item:hover .radio-control[data-state='checked']) {
        border-color: var(--color-accent-hover);
        background: var(--color-accent-hover);
    }

    /* Ark UI uses data-focus attribute when the hidden input is focused */
    :global(.radio-item[data-focus]) {
        outline: 2px solid var(--color-accent);
        outline-offset: 2px;
        border-radius: var(--radius-sm);
        box-shadow: var(--shadow-focus);
    }

    :global(.radio-text) {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
    }

    .radio-label {
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
    }

    .radio-description {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
    }
</style>
