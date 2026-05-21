<script lang="ts">
    /**
     * SearchModeChips: the chip row below the unified search bar.
     *
     * Renders one chip per available search mode. When AI is enabled, the row is:
     *   [AI Ask anything] [Filename] [Content (disabled)] [Regex]
     * When AI is disabled, the AI chip is hidden:
     *   [Filename] [Content (disabled)] [Regex]
     *
     * The Content chip is intentionally visible-disabled with a "Coming soon" tooltip. It has no
     * keyboard shortcut. Hostile-disabled controls (shortcut that no-ops) are worse than
     * visible-disabled controls with an explanation; see §3.1.1 of the search redesign plan.
     *
     * Keyboard:
     *   ←/→ moves focus between chips (skipping the disabled Content chip).
     *   Enter / Space on a chip activates it (no-op for the Content chip).
     *
     * The parent component owns the active-mode state. This component fires `onSelect(mode)` on
     * activation. The first interactive chip is `tabindex=0` when it matches the active mode;
     * the rest are `tabindex=-1` so Tab from the input lands on the active chip directly.
     */
    import { tooltip } from '$lib/tooltip/tooltip'
    import type { SearchMode } from './search-state.svelte'

    interface Props {
        mode: SearchMode
        aiEnabled: boolean
        disabled: boolean
        onSelect: (mode: SearchMode) => void
    }

    const { mode, aiEnabled, disabled, onSelect }: Props = $props()

    type ChipKey = SearchMode | 'content'
    interface Chip {
        key: ChipKey
        label: string
        badge?: string
        disabled?: boolean
        tooltipText?: string
        ariaLabel: string
    }

    const chips = $derived.by<Chip[]>(() => {
        const list: Chip[] = []
        if (aiEnabled) {
            list.push({
                key: 'ai',
                label: 'Ask anything',
                badge: 'AI',
                ariaLabel: 'AI mode: ask anything',
            })
        }
        list.push({ key: 'filename', label: 'Filename', ariaLabel: 'Filename mode' })
        list.push({
            key: 'content',
            label: 'Content',
            disabled: true,
            tooltipText: 'Coming soon: full-text search inside files',
            ariaLabel: 'Content mode (coming soon)',
        })
        list.push({ key: 'regex', label: 'Regex', ariaLabel: 'Regex mode' })
        return list
    })

    const chipButtons: HTMLButtonElement[] = $state([])

    function isActive(key: ChipKey): boolean {
        return key === mode
    }

    function activate(chip: Chip): void {
        if (chip.disabled || disabled) return
        if (chip.key === 'content') return
        // After the guard above, `key` is one of the SearchMode values (AI / Filename / Regex).
        onSelect(chip.key)
    }

    /** Index of the currently-focusable chip: the active one if it's interactive, otherwise the first interactive chip. */
    const focusableIndex = $derived.by(() => {
        const activeIdx = chips.findIndex((c) => isActive(c.key) && !c.disabled)
        if (activeIdx >= 0) return activeIdx
        return chips.findIndex((c) => !c.disabled)
    })

    /** Moves focus to the next or previous interactive chip, wrapping at edges. */
    function moveFocus(from: number, direction: 1 | -1): void {
        const count = chips.length
        if (count === 0) return
        let i = from
        for (let step = 0; step < count; step++) {
            i = (i + direction + count) % count
            const chip = chips[i]
            if (!chip.disabled) {
                chipButtons[i]?.focus()
                return
            }
        }
    }

    function handleKeyDown(e: KeyboardEvent, index: number, chip: Chip): void {
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
            activate(chip)
        }
    }
</script>

<div class="mode-chips" role="tablist" aria-label="Search mode">
    {#each chips as chip, index (chip.key)}
        <button
            bind:this={chipButtons[index]}
            type="button"
            class="mode-chip"
            class:is-active={isActive(chip.key)}
            class:is-coming-soon={chip.disabled}
            role="tab"
            aria-selected={isActive(chip.key)}
            aria-label={chip.ariaLabel}
            tabindex={index === focusableIndex ? 0 : -1}
            disabled={disabled || chip.disabled}
            onclick={() => {
                activate(chip)
            }}
            onkeydown={(e: KeyboardEvent) => {
                handleKeyDown(e, index, chip)
            }}
            use:tooltip={chip.tooltipText ?? ''}
        >
            {#if chip.badge}
                <span class="chip-badge">{chip.badge}</span>
            {/if}
            <span class="chip-label">{chip.label}</span>
        </button>
    {/each}
</div>

<style>
    .mode-chips {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-sm) var(--spacing-lg);
        background: var(--color-bg-primary);
        flex-wrap: wrap;
    }

    .mode-chip {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-xxs) var(--spacing-sm);
        font-size: var(--font-size-sm);
        font-weight: 500;
        line-height: 1;
        color: var(--color-text-secondary);
        background: transparent;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        white-space: nowrap;
        transition:
            background var(--transition-base),
            border-color var(--transition-base),
            color var(--transition-base);
    }

    .mode-chip:not(:disabled):hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    .mode-chip.is-active {
        /* Dynamic accent at low saturation: tinted bg + accent border + primary text for contrast.
           Pure `--color-accent` as text fails WCAG AA at 12 px; primary text on the subtle accent
           tint reads ~10:1 in both modes. Matches the AI badge contrast strategy. */
        background: var(--color-accent-subtle);
        border-color: var(--color-accent);
        color: var(--color-text-primary);
    }

    .mode-chip:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    /* Coming-soon (the Content chip) keeps a softer look than a regular disabled state: it's
       a teaser, not a temporarily unavailable action. */
    .mode-chip.is-coming-soon {
        opacity: 0.6;
        font-style: italic;
        color: var(--color-text-tertiary);
    }

    .chip-badge {
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

    .chip-label {
        line-height: 1;
    }
</style>
