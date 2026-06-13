<script lang="ts" module>
    /**
     * One item in a `Select`. `value` is the stable identity (compared and emitted as a string);
     * `label` is the visible text; `description` renders as quieter inline text after the label
     * (used by `SettingSelect`'s option descriptions); `group` is an optional group/optgroup label
     * that, when present, buckets the item under an Ark `ItemGroup` with that heading.
     */
    export interface SelectItem {
        value: string
        label: string
        description?: string
        group?: string
    }
</script>

<script lang="ts">
    /**
     * Presentational, items-driven single-select built on Ark UI's `Select`. The house dropdown:
     * `SettingSelect` and the viewer / transfer / debug native-`<select>` replacements all render
     * through it so the macOS-y look stays in one place.
     *
     * Stable class contract (don't rename without updating consumers + the a11y-contrast checker's
     * `scripts/check-a11y-contrast/dropdown_states.go`, which keys on literal selector strings):
     * `.select-trigger`, `.select-item`, `.select-content`, `.option-description`. `SettingSelect`'s
     * `handleCustomSubmit` focuses `.select-trigger` via `querySelector`. The `ariaLabel` lands on
     * the trigger.
     *
     * No entrance animation by default (matches `SettingSelect`); any polish anim must be gated
     * behind `prefers-reduced-motion`. Not wrapped in `Portal` (the viewer's restricted capability
     * set depends on no portal-to-body).
     */
    import { Select, createListCollection, type SelectValueChangeDetails } from '@ark-ui/svelte/select'
    import IconChevronDown from '~icons/lucide/chevron-down'

    interface Props {
        items: SelectItem[]
        /** Selected item's `value`. Empty string means nothing selected (renders `placeholder`). */
        value: string
        onChange: (value: string) => void
        /** Fires on keyboard/pointer highlight. `SettingSelect` applies immediately on highlight. */
        onHighlightChange?: (highlightedValue: string | null) => void
        disabled?: boolean
        placeholder?: string
        ariaLabel: string
        /** Extra class on the `.select-content` element (for example `custom-highlighted`). */
        contentClass?: string
    }

    const {
        items,
        value,
        onChange,
        onHighlightChange,
        disabled = false,
        placeholder = 'Select...',
        ariaLabel,
        contentClass = '',
    }: Props = $props()

    const collection = $derived(
        createListCollection({
            items,
            itemToString: (item: SelectItem) => item.label,
            itemToValue: (item: SelectItem) => item.value,
        }),
    )

    // Items render either flat or bucketed under their `group` label. Preserve first-seen group
    // order; items without a `group` bucket under '' (rendered as an unlabelled group).
    const groupedItems = $derived.by((): { label: string; items: SelectItem[] }[] | null => {
        const hasGroups = items.some((item) => item.group)
        if (!hasGroups) return null
        const groups: { label: string; items: SelectItem[] }[] = []
        for (const item of items) {
            const key = item.group ?? ''
            const existing = groups.find((g) => g.label === key)
            if (existing) existing.items.push(item)
            else groups.push({ label: key, items: [item] })
        }
        return groups
    })

    function handleValueChange(details: SelectValueChangeDetails<SelectItem>): void {
        if (details.value.length > 0) onChange(details.value[0])
    }

    function handleHighlightChange(details: { highlightedValue: string | null }): void {
        onHighlightChange?.(details.highlightedValue)
    }
</script>

<Select.Root
    {collection}
    value={value ? [value] : []}
    onValueChange={handleValueChange}
    onHighlightChange={handleHighlightChange}
    {disabled}
>
    <Select.Control>
        <Select.Trigger class="select-trigger" aria-label={ariaLabel}>
            <Select.ValueText {placeholder} />
            <span class="select-indicator"><IconChevronDown width="16" height="16" /></span>
        </Select.Trigger>
    </Select.Control>
    <Select.Positioner>
        <Select.Content
            class={`select-content${contentClass ? ` ${contentClass}` : ''}`}
            onkeydown={(e: KeyboardEvent) => {
                // Keep Escape scoped to the dropdown so a host dialog's capture-phase Escape
                // doesn't also fire and close the whole dialog.
                if (e.key === 'Escape') e.stopPropagation()
            }}
        >
            {#if groupedItems}
                {#each groupedItems as group (group.label)}
                    <Select.ItemGroup>
                        {#if group.label}
                            <Select.ItemGroupLabel class="select-group-label">
                                {group.label}
                            </Select.ItemGroupLabel>
                        {/if}
                        {#each group.items as item (item.value)}
                            <Select.Item {item} class="select-item">
                                <Select.ItemText>
                                    {item.label}
                                    {#if item.description}
                                        <span class="option-description"> — {item.description}</span>
                                    {/if}
                                </Select.ItemText>
                                <Select.ItemIndicator class="item-indicator">✓</Select.ItemIndicator>
                            </Select.Item>
                        {/each}
                    </Select.ItemGroup>
                {/each}
            {:else}
                {#each items as item (item.value)}
                    <Select.Item {item} class="select-item">
                        <Select.ItemText>
                            {item.label}
                            {#if item.description}
                                <span class="option-description"> — {item.description}</span>
                            {/if}
                        </Select.ItemText>
                        <Select.ItemIndicator class="item-indicator">✓</Select.ItemIndicator>
                    </Select.Item>
                {/each}
            {/if}
        </Select.Content>
    </Select.Positioner>
    <Select.HiddenSelect />
</Select.Root>

<style>
    :global(.select-trigger) {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) var(--spacing-sm);
        width: 100%;
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

    /* Standardized Lucide chevron with a real hit-area (replaces the old tiny ▼ glyph). */
    :global(.select-indicator) {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        flex-shrink: 0;
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
        /* Consistent width regardless of content. */
        min-width: 180px;
        width: max-content;
        outline: none;
    }

    :global(.select-content:focus),
    :global(.select-content:focus-visible) {
        outline: none;
    }

    :global(.select-group-label) {
        display: block;
        padding: var(--spacing-xs) var(--spacing-sm);
        font-size: var(--font-size-xs);
        font-weight: 600;
        color: var(--color-text-tertiary);
    }

    :global(.select-item) {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) var(--spacing-sm);
        cursor: default;
        font-size: var(--font-size-sm);
        outline: none;
    }

    /* No hover visual indication: only the checked/highlighted state matters. */
    :global(.select-item:hover) {
        background: transparent;
    }

    /* Highlighted item (keyboard navigation): same as checked for immediate feedback. */
    :global(.select-item[data-highlighted]) {
        background: var(--color-accent);
        color: var(--color-accent-fg);
    }

    :global(.select-item[data-state='checked']) {
        background: var(--color-accent);
        color: var(--color-accent-fg);
    }

    :global(.select-item[data-state='checked']:hover),
    :global(.select-item[data-highlighted]:hover) {
        background: var(--color-accent-hover);
    }

    :global(.select-item:focus),
    :global(.select-item:focus-visible) {
        outline: none;
    }

    :global(.item-indicator) {
        /* Always reserve space for the checkmark to prevent layout shift. */
        min-width: 1em;
        text-align: center;
        color: var(--color-accent-text);
        visibility: hidden;
    }

    :global(.select-item[data-state='checked'] .item-indicator),
    :global(.select-item[data-highlighted] .item-indicator) {
        visibility: visible;
        color: var(--color-accent-fg);
    }

    /* When a content-level class (`custom-highlighted`) is set, hide the checked state from other
       items so the highlighted "Custom…" row is the only visually-selected one. */
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

    /* When the parent option is highlighted (cursor over, or checked), the bg flips to
       `--color-accent`. The description's resting `--color-text-tertiary` (#666 / #a0a0a0) drops to
       ~1–2.4:1 contrast on every system accent. Switch it to `--color-accent-fg` (auto-picked
       black/white via `readableFgOn`) so it matches the label's color and stays readable. The
       secondary visual weight is lost in this state, but on a saturated bg there's no opacity that
       stays both secondary and AA-compliant for Apple Purple (the worst dark-fg case). The contrast
       checker's `dropdown_states.go` matrix validates this against every accent variant. */
    :global(.select-item[data-highlighted]) .option-description,
    :global(.select-item[data-state='checked']) .option-description {
        color: var(--color-accent-fg);
    }

    /* Exception: when `custom-highlighted` is set, the other checked items lose their accent bg (see
       the `.custom-highlighted .select-item…` rule above). Revert the description color so it stays
       readable on the now-transparent bg. */
    :global(.custom-highlighted .select-item[data-state='checked']:not([data-highlighted])) .option-description {
        color: var(--color-text-tertiary);
    }
</style>
