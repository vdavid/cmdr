<script lang="ts">
    /**
     * Settings > Behavior > Search.
     *
     * Renders the auto-apply toggle (canonical home) plus a mirror of
     * `search.recentSearches.maxCount` so users who search "search" land on
     * something useful. The mirror follows the pattern in
     * `lib/settings/CLAUDE.md` § "Mirroring a setting in multiple sections":
     * the registry stays single-entry under Advanced, and rendering it a
     * second time here just adds a discoverability surface. Search-tree
     * highlighting and full-text search remain canonical-only by design.
     */
    import SettingsSection from '../components/SettingsSection.svelte'
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
    // The registry entry stays where it is; we just render the row here too so users hunting under
    // "search" find it. `shouldShow('search.recentSearches.maxCount')` still gates it under a query.
    const recentMaxDef = getSettingDefinition('search.recentSearches.maxCount') ?? defaultDef
</script>

<SettingsSection title="Search">
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
            label={recentMaxDef.label}
            description={recentMaxDef.description}
            split
            {searchQuery}
        >
            <SettingNumberInput id="search.recentSearches.maxCount" />
        </SettingRow>
    {/if}
</SettingsSection>
