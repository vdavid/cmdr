<script lang="ts">
    /**
     * Settings > Behavior > Search.
     *
     * Renders the auto-apply toggle (canonical home) plus mirrors of
     * `search.recentSearches.maxCount` and `selection.recentSelections.maxCount`
     * so users who browse "Behavior > Search" land on something useful. Both
     * mirrors follow the pattern in `lib/settings/CLAUDE.md` § "Mirroring a
     * setting in multiple sections": the registry entries stay in Advanced, and
     * rendering them here adds a discoverability surface. Search-tree
     * highlighting and full-text search remain canonical-only by design.
     */
    import SettingsSection from '../components/SettingsSection.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SettingNumberInput from '../components/SettingNumberInput.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { createShouldShow } from '$lib/settings/settings-search'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const defaultDef = { label: '', description: '' }
    const autoApplyDef = getSettingDefinition('search.autoApply') ?? defaultDef
    // Mirrored from Advanced: see `lib/settings/CLAUDE.md` § "Mirroring a setting in multiple sections".
    // The registry entries stay where they are; we just render the rows here too so users hunting
    // under "Search" find them. `shouldShow(...)` still gates them under a query.
    const recentSearchesMaxDef = getSettingDefinition('search.recentSearches.maxCount') ?? defaultDef
    const recentSelectionsMaxDef = getSettingDefinition('selection.recentSelections.maxCount') ?? defaultDef
</script>

<SettingsSection title={tString('settings.section.search')}>
    {#if shouldShow('search.autoApply')}
        <SettingRow
            id="search.autoApply"
            label={autoApplyDef.label}
            description={autoApplyDef.description}
            {searchQuery}
        >
            <SettingSwitch id="search.autoApply" />
        </SettingRow>
    {/if}

    {#if shouldShow('search.recentSearches.maxCount')}
        <SettingRow
            id="search.recentSearches.maxCount"
            label={recentSearchesMaxDef.label}
            description={recentSearchesMaxDef.description}
            split
            {searchQuery}
        >
            <SettingNumberInput id="search.recentSearches.maxCount" />
        </SettingRow>
    {/if}

    {#if shouldShow('selection.recentSelections.maxCount')}
        <SettingRow
            id="selection.recentSelections.maxCount"
            label={recentSelectionsMaxDef.label}
            description={recentSelectionsMaxDef.description}
            split
            {searchQuery}
        >
            <SettingNumberInput id="selection.recentSelections.maxCount" />
        </SettingRow>
    {/if}
</SettingsSection>
