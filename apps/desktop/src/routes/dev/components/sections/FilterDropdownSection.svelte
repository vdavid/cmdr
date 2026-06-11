<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import FilterDropdown from '$lib/ui/FilterDropdown.svelte'

    let anchorEl: HTMLButtonElement | undefined = $state()
    let open = $state(false)
</script>

<SectionCard id="components-filter-dropdown" label="Filter dropdown">
    <div class="cell">
        <p class="caption">
            The labelled-grid filter surface: a `Dropdown` with an uppercase section header. Used by the query dialogs'
            Size / Modified / Search-in popovers. Click the anchor to toggle.
        </p>
        <button
            bind:this={anchorEl}
            type="button"
            class="demo-anchor"
            onclick={() => {
                open = !open
            }}
        >
            {open ? 'Close filter' : 'Open filter'}
        </button>
        {#if anchorEl}
            <FilterDropdown
                anchor={anchorEl}
                {open}
                onClose={() => {
                    open = false
                }}
                label="Size"
                ariaLabel="Size filter options"
            >
                <div class="demo-grid" role="radiogroup" aria-label="Comparator">
                    <button type="button" class="demo-cell">any</button>
                    <button type="button" class="demo-cell is-selected">≥</button>
                    <button type="button" class="demo-cell">≤</button>
                </div>
            </FilterDropdown>
        {/if}
    </div>
</SectionCard>

<style>
    .caption {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .demo-anchor {
        padding: var(--spacing-xs) var(--spacing-md);
        font-size: var(--font-size-sm);
        background: transparent;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
    }

    .demo-grid {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
    }

    .demo-cell {
        text-align: left;
        padding: var(--spacing-xxs) var(--spacing-sm);
        font-size: var(--font-size-sm);
        background: transparent;
        border: 1px solid transparent;
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
    }

    .demo-cell.is-selected {
        background: var(--color-accent-subtle);
        border-color: var(--color-accent);
    }
</style>
