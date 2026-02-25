<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingToggleGroup from '../components/SettingToggleGroup.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { createShouldShow } from '$lib/settings/settings-search'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const dirSortDef = getSettingDefinition('listing.directorySortMode') ?? { label: '', description: '' }
</script>

<SettingsSection title="Listing">
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
</SettingsSection>
