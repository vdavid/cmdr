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
        /**
         * Lays the options out in this many equal full-width columns, filling row by row
         * (5 options over 3 columns read as 3 + 2). Overrides `orientation`. Use it when a
         * wrapping row would break at an arbitrary place and leave the options looking
         * scattered; the grid gives them a shared left edge per column.
         */
        columns?: number
        /** Accessible name for the group root. */
        ariaLabel?: string
        /**
         * Rendered after the items, receiving the current `value`. Preserves the "custom content when
         * a specific option is selected" feature; the caller decides visibility.
         */
        footer?: Snippet<[string]>
        /**
         * Rendered on the same line as each option, receiving that option's `value`; the caller
         * returns content for the one option it belongs to and nothing for the rest. It renders
         * BESIDE the option, never inside it: a focusable control nested in a `role="radio"`
         * element trips axe's nested-interactive rule.
         */
        itemTrailing?: Snippet<[string]>
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let {
        value = $bindable(''),
        items,
        onValueChange,
        disabled = false,
        orientation = 'vertical',
        columns,
        ariaLabel,
        footer,
        itemTrailing,
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
    <div
        class="radio-group"
        class:horizontal={orientation === 'horizontal' && columns === undefined}
        class:grid={columns !== undefined}
        style={columns === undefined ? undefined : `grid-template-columns: repeat(${String(columns)}, minmax(0, 1fr))`}
    >
        {#each items as item (item.value)}
            <div class="radio-row">
                <!-- The described case is a DATA ATTRIBUTE, not a second class: a
                     computed `class` string hides the name from the unused-CSS
                     scanner, which then reads every `.radio-*` rule as dead. -->
                <RadioGroup.Item
                    value={item.value}
                    class="radio-item"
                    data-described={item.description ? 'true' : undefined}
                    disabled={disabled || item.disabled}
                >
                    <RadioGroup.ItemControl class="radio-control" />
                    <RadioGroup.ItemText class="radio-text">
                        <span class="radio-label">{item.label}</span>
                        {#if item.description}
                            <span class="radio-description">{item.description}</span>
                        {/if}
                    </RadioGroup.ItemText>
                    <RadioGroup.ItemHiddenInput />
                </RadioGroup.Item>
                {#if itemTrailing}
                    {@render itemTrailing(item.value)}
                {/if}
            </div>
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

    /* Holds an option and its optional `itemTrailing` control on one line. With no trailing
       control the option is the row's only element child, so it stretches the way it did
       before the wrapper existed and the whole row width stays clickable. */
    .radio-row {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }

    .radio-row > :global(.radio-item:only-child) {
        flex: 1 1 auto;
    }

    .radio-group.horizontal {
        flex-direction: row;
        flex-wrap: wrap;
        gap: var(--spacing-md);
    }

    /* Equal columns filling the full width, so a leftover row (5 options over 3
       columns) still lines its options up under the ones above. The track list is
       inline (the count is a prop): `repeat(n, minmax(0, 1fr))`, `minmax` rather
       than a bare `1fr` so a long label wraps inside its column instead of widening
       the whole grid. */
    .radio-group.grid {
        display: grid;
        gap: var(--spacing-xs) var(--spacing-md);
    }

    /* A plain option centers its dot on its label: with `flex-start` a single-line
       label sits visibly high against the 16px control, which reads as sloppy across
       a row of options. Only a described option top-aligns (`[data-described]`),
       because there the dot belongs beside the LABEL line, not the block's middle. */
    :global(.radio-item) {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) 0;
        cursor: default;
        /* Contain each item's Ark `ItemHiddenInput` so it resolves against its
           own row, not the window shell. See `Switch.svelte`'s `.switch-root`
           for the full why (the shell-scroll-under-traffic-lights bug). */
        position: relative;
    }

    :global(.radio-item[data-described]) {
        align-items: flex-start;
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
        transition: all var(--transition-base);
    }

    /* Nudges the dot onto the label's optical middle when a description stacks
       below it, which `align-items: flex-start` alone leaves a hair high. */
    :global(.radio-item[data-described] .radio-control) {
        margin-top: var(--spacing-xxs);
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
