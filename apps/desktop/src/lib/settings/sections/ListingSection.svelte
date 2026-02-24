<script lang="ts">
    import SettingRow from '../components/SettingRow.svelte'
    import SettingToggleGroup from '../components/SettingToggleGroup.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { getMatchingSettingIds } from '$lib/settings/settings-search'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    // Get matching setting IDs for filtering
    const matchingIds = $derived(searchQuery.trim() ? getMatchingSettingIds(searchQuery) : null)

    // Check if a setting should be shown
    function shouldShow(id: string): boolean {
        if (!matchingIds) return true
        return matchingIds.has(id)
    }

    const dirSortDef = getSettingDefinition('listing.directorySortMode') ?? { label: '', description: '' }
</script>

<div class="section">
    <h2 class="section-title">Listing</h2>

    {#if shouldShow('listing.directorySortMode')}
        <SettingRow
            id="listing.directorySortMode"
            label={dirSortDef.label}
            description={dirSortDef.description}
            {searchQuery}
        >
            <SettingToggleGroup id="listing.directorySortMode" />
        </SettingRow>
    {/if}
</div>

<style>
    .section {
        margin-bottom: var(--spacing-lg);
    }

    .section-title {
        font-size: var(--font-size-lg);
        font-weight: 600;
        color: var(--color-text-primary);
        margin: 0 0 var(--spacing-sm);
        padding-bottom: var(--spacing-xs);
        border-bottom: 1px solid var(--color-border);
    }
</style>
