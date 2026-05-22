<script lang="ts" module>
    /**
     * Option for the `ToggleGroup` primitive.
     *
     * `badge` renders a small uppercase pill before the label (for example `AI` on the search mode chip).
     * `hint` renders inline tertiary mono text after the label (for example `⌥A` to surface a keyboard hint).
     * `disabled` blocks activation. Combined with `tooltip`, this is the "visible-disabled with tooltip"
     *   pattern used for "Coming soon" affordances: the option stays interactive enough that the tooltip
     *   still fires on hover / focus.
     * `ariaLabel` overrides the computed accessible name when the visible label alone isn't enough
     *   (for example to include the keyboard shortcut for AT users).
     */
    export interface ToggleGroupOption {
        value: string
        label: string
        badge?: string
        hint?: string
        disabled?: boolean
        tooltip?: string
        ariaLabel?: string
    }
</script>

<script lang="ts">
    /**
     * Generic segmented-control primitive used by Settings (`SettingToggleGroup`) and the search /
     * selection mode chips. One visual contract, two ARIA shapes selected via the `semantics` prop:
     *
     * - `semantics: 'tabs'` renders `<div role="tablist">` + `<button role="tab" aria-selected>`.
     *   Use this when the active option drives a UI mode (the user hears "tab 2 of 4, Filename, selected").
     *   Arrow keys cycle through options skipping disabled ones; the active option is `tabindex=0` and
     *   the rest are `tabindex=-1` so Tab from a sibling input lands on the active option directly.
     *
     * - `semantics: 'toggles'` wraps Ark UI's `ToggleGroup.Root` + `ToggleGroup.Item`, single-select.
     *   Use this when the active option picks a stored value (the user hears "toggle button, kB,
     *   pressed"). Ark handles the keyboard contract for this shape.
     *
     * Both shapes share visual CSS so they render identically. Badge and hint slots work the same
     * way in both modes.
     */
    import { ToggleGroup as ArkToggleGroup } from '@ark-ui/svelte/toggle-group'
    import { tooltip } from '$lib/tooltip/tooltip'

    interface Props {
        semantics: 'tabs' | 'toggles'
        value: string
        options: ToggleGroupOption[]
        onChange: (value: string) => void
        ariaLabel: string
        disabled?: boolean
    }

    const { semantics, value, options, onChange, ariaLabel, disabled = false }: Props = $props()

    // Index of the option that should carry `tabindex=0` in tabs mode: the active one if it's
    // interactive, otherwise the first interactive option. Mirrors today's `SearchModeChips` logic
    // so an all-disabled-but-active row still has one tab-stop.
    const focusableIndex = $derived.by(() => {
        const activeIdx = options.findIndex((o) => o.value === value && !o.disabled)
        if (activeIdx >= 0) return activeIdx
        return options.findIndex((o) => !o.disabled)
    })

    function activate(option: ToggleGroupOption): void {
        if (disabled || option.disabled) return
        if (option.value === value) return
        onChange(option.value)
    }

    function handleToggleValueChange(details: { value: string[] }): void {
        if (disabled) return
        if (details.value.length === 0) return // Single-select: don't allow deselecting all.
        const next = details.value[0]
        if (next === value) return
        onChange(next)
    }

    // === Tabs-specific keyboard motion ===
    // Ported verbatim from `SearchModeChips.svelte`. The bespoke `<button role="tab">` shape doesn't
    // get keyboard handling for free, so we mirror the existing algorithm: ArrowLeft / ArrowRight
    // cycle wrap-around, skipping disabled options; Enter / Space activate. We keep an array of button
    // refs (`tabButtons[]`) so we can `.focus()` the right one without querying the DOM.
    const tabButtons: HTMLButtonElement[] = $state([])

    function moveFocus(from: number, direction: 1 | -1): void {
        const count = options.length
        if (count === 0) return
        let i = from
        for (let step = 0; step < count; step++) {
            i = (i + direction + count) % count
            if (!options[i].disabled) {
                tabButtons[i]?.focus()
                return
            }
        }
    }

    function handleTabKeyDown(e: KeyboardEvent, index: number, option: ToggleGroupOption): void {
        if (e.key === 'ArrowRight') {
            e.preventDefault()
            moveFocus(index, 1)
            return
        }
        if (e.key === 'ArrowLeft') {
            e.preventDefault()
            moveFocus(index, -1)
            return
        }
        if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault()
            activate(option)
        }
    }
</script>

{#if semantics === 'tabs'}
    <div class="tg-root" role="tablist" aria-label={ariaLabel}>
        {#each options as option, index (option.value)}
            <button
                bind:this={tabButtons[index]}
                type="button"
                class="tg-item"
                class:is-active={option.value === value}
                class:is-disabled={option.disabled}
                role="tab"
                aria-selected={option.value === value}
                aria-label={option.ariaLabel}
                tabindex={index === focusableIndex ? 0 : -1}
                disabled={disabled || option.disabled}
                onclick={() => {
                    activate(option)
                }}
                onkeydown={(e: KeyboardEvent) => {
                    handleTabKeyDown(e, index, option)
                }}
                use:tooltip={option.tooltip ?? ''}
            >
                {#if option.badge}
                    <span class="tg-badge">{option.badge}</span>
                {/if}
                <span class="tg-label">{option.label}</span>
                {#if option.hint}
                    <span class="tg-hint" aria-hidden="true">{option.hint}</span>
                {/if}
            </button>
        {/each}
    </div>
{:else}
    <ArkToggleGroup.Root
        class="tg-root"
        value={[value]}
        onValueChange={handleToggleValueChange}
        {disabled}
        aria-label={ariaLabel}
    >
        {#each options as option (option.value)}
            <ArkToggleGroup.Item
                value={option.value}
                class="tg-item"
                disabled={disabled || option.disabled}
                aria-label={option.ariaLabel}
            >
                <span class="tg-item-inner" use:tooltip={option.tooltip ?? ''}>
                    {#if option.badge}
                        <span class="tg-badge">{option.badge}</span>
                    {/if}
                    <span class="tg-label">{option.label}</span>
                    {#if option.hint}
                        <span class="tg-hint" aria-hidden="true">{option.hint}</span>
                    {/if}
                </span>
            </ArkToggleGroup.Item>
        {/each}
    </ArkToggleGroup.Root>
{/if}

<style>
    /*
     * Shared visual chrome for both semantics. Tokens migrated from
     * `lib/settings/components/SettingToggleGroup.svelte`'s globals so both Settings and the
     * future Query mode chips render identically.
     */
    .tg-root {
        display: inline-flex;
        align-items: center;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        overflow: hidden;
    }

    /* Tabs and Ark toggles both render their items via `.tg-item`. Ark prints
       `data-scope="toggle-group"][data-part="item"]` on the same node, so the same selector covers
       both shapes for the cell styling that follows. */
    .tg-root :global(.tg-item) {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-xs) var(--spacing-md);
        border: none;
        border-right: 1px solid var(--color-border);
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        font-weight: 500;
        line-height: 1;
        white-space: nowrap;
        transition:
            background var(--transition-base),
            border-color var(--transition-base),
            color var(--transition-base);
    }

    .tg-root :global(.tg-item:last-child) {
        border-right: none;
    }

    .tg-root :global(.tg-item:not(:disabled):hover) {
        background: var(--color-bg-tertiary);
    }

    /* Active state: tabs branch uses an `.is-active` class; toggles branch uses Ark's
       `data-state="on"` attribute. Spell both out so we don't drift. */
    .tg-root :global(.tg-item.is-active),
    .tg-root :global(.tg-item[data-state='on']) {
        background: var(--color-accent);
        color: var(--color-accent-fg);
    }

    .tg-root :global(.tg-item.is-active:hover),
    .tg-root :global(.tg-item[data-state='on']:hover) {
        background: var(--color-accent-hover);
    }

    .tg-root :global(.tg-item:disabled),
    .tg-root :global(.tg-item.is-disabled),
    .tg-root :global(.tg-item[data-disabled]) {
        opacity: 0.5;
    }

    .tg-root :global(.tg-item:focus-visible) {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
        box-shadow: var(--shadow-focus);
        z-index: 1;
    }

    /* Inner wrapper for Ark items: the tooltip action needs a real element to attach to and Ark's
       item is the host button. We wrap the contents in a span so badge / label / hint can sit
       inside it the same way the tabs branch lays them out. */
    .tg-root :global(.tg-item-inner) {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .tg-root :global(.tg-badge) {
        font-size: var(--font-size-xs);
        font-family: var(--font-mono);
        font-weight: 600;
        letter-spacing: 0.04em;
        padding: var(--spacing-xxs) var(--spacing-xs);
        background: var(--color-accent-subtle);
        color: var(--color-text-primary);
        border-radius: var(--radius-xs);
        line-height: 1;
    }

    .tg-root :global(.tg-label) {
        line-height: 1;
    }

    /* Mono tertiary hint (for example `⌥A`). Visible only in the resting state so it doesn't
       compete with the accent-filled active cell. */
    .tg-root :global(.tg-hint) {
        margin-left: var(--spacing-xxs);
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        line-height: 1;
        opacity: 0.7;
    }
</style>
