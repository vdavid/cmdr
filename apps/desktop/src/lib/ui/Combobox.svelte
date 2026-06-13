<script lang="ts" module>
    /** One suggestion in a `Combobox`. `value` is the stable identity; `label` is the visible text. */
    export interface ComboboxItem {
        value: string
        label: string
    }
</script>

<script lang="ts">
    /**
     * Presentational text-field-with-suggestions built on Ark UI's `Combobox`. Pick from the list OR
     * type your own; the list can be empty (cold start) or load async, and the field stays usable
     * throughout. The AI model picker (settings + onboarding) is the consumer.
     *
     * Critical value model (per the dropdown-uniformization plan, finding #4): this is NOT a
     * value-bound select. Ark's default `selectionBehavior: "replace"` runs `stringifyMany` on every
     * `value` change, which DROPS any value not in the collection and BLANKS the input — exactly the
     * regression we forbid on empty/mid-fetch lists and custom model names. So:
     * - The displayed text is `inputValue` (the saved/typed string), controlled separately from
     *   `value` and NEVER derived from collection membership.
     * - `selectionBehavior="preserve"` keeps a typed custom value through a list sync.
     * - `allowCustomValue` accepts the custom value on close.
     * - `value` is intentionally left uncontrolled / unused: a typed custom value persists.
     *
     * Open-on-focus is wired via a controlled `open` state (Ark has no `openOnFocus` prop). `loading`
     * is OUR in-field spinner overlay (Ark has no loading prop). No `Portal` (keeps the viewer's
     * restricted capability set unaffected). No entrance animation by default.
     */
    import { Combobox, createListCollection } from '@ark-ui/svelte/combobox'
    import IconChevronDown from '~icons/lucide/chevron-down'

    interface Props {
        items: ComboboxItem[]
        /** The displayed/typed text. Controlled by the consumer (the saved or typed value). */
        inputValue: string
        onInputValueChange: (inputValue: string) => void
        /** Shows an in-field spinner overlay while the suggestion list is being fetched. */
        loading?: boolean
        disabled?: boolean
        placeholder?: string
        ariaLabel: string
        /** Shown in the popup when there are no suggestions (cold start or no match). */
        emptyText?: string
    }

    const {
        items,
        inputValue,
        onInputValueChange,
        loading = false,
        disabled = false,
        placeholder,
        ariaLabel,
        emptyText = 'No matches. Keep typing to use your own value.',
    }: Props = $props()

    const collection = $derived(
        createListCollection({
            items,
            itemToString: (item: ComboboxItem) => item.label,
            itemToValue: (item: ComboboxItem) => item.value,
        }),
    )

    let open = $state(false)

    function handleInputValueChange(details: { inputValue: string }): void {
        onInputValueChange(details.inputValue)
    }

    function handleOpenChange(details: { open: boolean }): void {
        open = details.open
    }
</script>

<div class="combobox-wrapper">
    <Combobox.Root
        {collection}
        {inputValue}
        {open}
        {disabled}
        selectionBehavior="preserve"
        allowCustomValue
        onInputValueChange={handleInputValueChange}
        onOpenChange={handleOpenChange}
    >
        <Combobox.Control class="combobox-control">
            <Combobox.Input
                class="combobox-input"
                {placeholder}
                aria-label={ariaLabel}
                onfocus={() => {
                    open = true
                }}
            />
            {#if loading}
                <span class="spinner spinner-sm combobox-spinner" aria-label="Loading suggestions" role="status"
                ></span>
            {/if}
            <Combobox.Trigger class="combobox-trigger" aria-label="Show suggestions">
                <span class="combobox-indicator"><IconChevronDown width="16" height="16" /></span>
            </Combobox.Trigger>
        </Combobox.Control>
        <Combobox.Positioner>
            <Combobox.Content
                class="combobox-content"
                onkeydown={(e: KeyboardEvent) => {
                    // Keep Escape scoped to the popup so a host dialog's capture-phase Escape doesn't
                    // also close the whole dialog.
                    if (e.key === 'Escape') e.stopPropagation()
                }}
            >
                {#each items as item (item.value)}
                    <Combobox.Item {item} class="combobox-item">
                        <Combobox.ItemText>{item.label}</Combobox.ItemText>
                        <Combobox.ItemIndicator class="combobox-item-indicator">✓</Combobox.ItemIndicator>
                    </Combobox.Item>
                {/each}
                {#if items.length === 0}
                    <!-- A non-actionable `option` so the `role="listbox"` content satisfies axe's
                         `aria-required-children` on a cold-start / no-match empty list, instead of an
                         empty listbox (axe flags that even when hidden). Reads as a "no matches" row. -->
                    <div class="combobox-empty" role="option" aria-disabled="true" aria-selected="false">
                        {emptyText}
                    </div>
                {/if}
            </Combobox.Content>
        </Combobox.Positioner>
    </Combobox.Root>
</div>

<style>
    .combobox-wrapper {
        min-width: 180px;
        width: 100%;
    }

    :global(.combobox-control) {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        width: 100%;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-bg-primary);
        padding-right: var(--spacing-xs);
    }

    :global(.combobox-control:focus-within) {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
        box-shadow: var(--shadow-focus);
    }

    :global(.combobox-input) {
        flex: 1;
        min-width: 0;
        padding: var(--spacing-xs) var(--spacing-sm);
        border: none;
        background: transparent;
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
    }

    :global(.combobox-input:focus) {
        outline: none;
    }

    :global(.combobox-input[data-disabled]) {
        cursor: not-allowed;
        opacity: 0.5;
    }

    .combobox-spinner {
        align-self: center;
    }

    :global(.combobox-trigger) {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        flex-shrink: 0;
        padding: 0;
        border: none;
        background: transparent;
        cursor: default;
    }

    :global(.combobox-indicator) {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        color: var(--color-text-tertiary);
    }

    :global(.combobox-content) {
        background: var(--color-bg-primary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        box-shadow: var(--shadow-md);
        padding: var(--spacing-xs) 0;
        z-index: var(--z-dropdown);
        max-height: 300px;
        overflow-y: auto;
        min-width: 180px;
        width: max-content;
        outline: none;
    }

    :global(.combobox-content:focus),
    :global(.combobox-content:focus-visible) {
        outline: none;
    }

    :global(.combobox-item) {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) var(--spacing-sm);
        cursor: default;
        font-size: var(--font-size-sm);
        outline: none;
    }

    :global(.combobox-item[data-highlighted]) {
        background: var(--color-accent);
        color: var(--color-accent-fg);
    }

    :global(.combobox-item[data-state='checked']) {
        background: var(--color-accent);
        color: var(--color-accent-fg);
    }

    :global(.combobox-item[data-highlighted]:hover),
    :global(.combobox-item[data-state='checked']:hover) {
        background: var(--color-accent-hover);
    }

    :global(.combobox-item-indicator) {
        min-width: 1em;
        text-align: center;
        visibility: hidden;
    }

    :global(.combobox-item[data-state='checked'] .combobox-item-indicator),
    :global(.combobox-item[data-highlighted] .combobox-item-indicator) {
        visibility: visible;
        color: var(--color-accent-fg);
    }

    :global(.combobox-empty) {
        padding: var(--spacing-xs) var(--spacing-sm);
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
    }
</style>
