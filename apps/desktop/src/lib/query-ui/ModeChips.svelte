<script lang="ts">
    /**
     * ModeChips: the chip row below the unified query bar.
     *
     * Renders one chip per available query mode. Built on top of the shared
     * `lib/ui/ToggleGroup.svelte` primitive with `semantics="tabs"` so the row exposes
     * `role="tablist"` ARIA (the active option drives a UI mode, not a stored value).
     *
     * Search renders the full set:
     *     [AI Ask anything ⌥A] [Filename ⌥F] [Content (disabled)] [Regex ⌥R]
     * When AI is disabled, the AI chip is hidden. Selection (M7+) renders the same minus
     * the disabled Content chip.
     *
     * The Content chip stays visible-disabled with a "Coming soon" tooltip. It has no
     * keyboard shortcut. Hostile-disabled controls (shortcut that no-ops) are worse than
     * visible-disabled controls with an explanation; see §3.1.1 of the search redesign plan.
     *
     * Keyboard motion (arrow keys skipping disabled options, active option as the tab-stop)
     * lives in `ToggleGroup`; this component just declares the option set.
     */
    import ToggleGroup, { type ToggleGroupOption } from '$lib/ui/ToggleGroup.svelte'
    import type { SearchMode } from './query-filter-state.svelte'

    interface Props {
        mode: SearchMode
        aiEnabled: boolean
        disabled: boolean
        onSelect: (mode: SearchMode) => void
    }

    const { mode, aiEnabled, disabled, onSelect }: Props = $props()

    /**
     * Builds the option set for the underlying ToggleGroup. The "content" option has no
     * `SearchMode` counterpart on purpose: it's a placeholder for the future full-text
     * search feature and lives only in the visual chip set so users see it on the horizon.
     */
    const options = $derived.by<ToggleGroupOption[]>(() => {
        const list: ToggleGroupOption[] = []
        if (aiEnabled) {
            list.push({
                value: 'ai',
                label: 'Ask anything',
                badge: 'AI',
                hint: '⌥A',
                ariaLabel: 'AI mode: ask anything (Alt+A)',
            })
        }
        list.push({ value: 'filename', label: 'Filename', hint: '⌥F', ariaLabel: 'Filename mode (Alt+F)' })
        list.push({
            value: 'content',
            label: 'Content',
            disabled: true,
            tooltip: 'Coming soon: full-text search inside files',
            ariaLabel: 'Content mode (coming soon)',
        })
        list.push({ value: 'regex', label: 'Regex', hint: '⌥R', ariaLabel: 'Regex mode (Alt+R)' })
        return list
    })

    function handleChange(next: string): void {
        // ToggleGroup blocks activation of disabled options, so `next` is one of the
        // SearchMode values (AI / Filename / Regex). Cast is safe.
        onSelect(next as SearchMode)
    }
</script>

<!-- ToggleGroup's `.tg-root` carries the visual chrome shared with Settings's segmented
     controls. The wrapper adds the chip-row's outer padding + background. -->
<div class="mode-chips-wrap">
    <ToggleGroup
        semantics="tabs"
        value={mode}
        {options}
        onChange={handleChange}
        ariaLabel="Search mode"
        {disabled}
    />
</div>

<style>
    .mode-chips-wrap {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-sm) var(--spacing-lg);
        background: var(--color-bg-primary);
        flex-wrap: wrap;
    }
</style>
