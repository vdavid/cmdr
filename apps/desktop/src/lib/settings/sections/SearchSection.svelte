<script lang="ts">
    /**
     * Settings > Behavior > Search.
     *
     * One card: the auto-apply toggle. The recent-searches and recent-selections
     * caps live in Advanced (their single home); this page holds only its own
     * settings, never a mirror.
     *
     * Card visibility is section-owned: the `SectionCard` frame is wrapped in
     * `{#if anyVisible(shouldShow, ...ids)}` over the SAME `shouldShow` predicate
     * that gates each row.
     */
    import SettingsSection from '../components/SettingsSection.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { createShouldShow, anyVisible } from '$lib/settings/settings-search'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const defaultDef = { label: '', description: '' }
    const autoApplyDef = getSettingDefinition('search.autoApply') ?? defaultDef
</script>

<SettingsSection title={tString('settings.section.search')}>
    {#if anyVisible(shouldShow, 'search.autoApply')}
        <SectionCard>
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
        </SectionCard>
    {/if}
</SettingsSection>
