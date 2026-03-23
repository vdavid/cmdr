<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { createShouldShow } from '$lib/settings/settings-search'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const autoCheckDef = getSettingDefinition('updates.autoCheck') ?? { label: '', description: '' }
    const crashReportsDef = getSettingDefinition('updates.crashReports') ?? { label: '', description: '' }
</script>

<SettingsSection title="Updates">
    {#if shouldShow('updates.autoCheck')}
        <SettingRow
            id="updates.autoCheck"
            label={autoCheckDef.label}
            description={autoCheckDef.description}
            {searchQuery}
        >
            <SettingSwitch id="updates.autoCheck" />
        </SettingRow>
    {/if}
    {#if shouldShow('updates.crashReports')}
        <SettingRow
            id="updates.crashReports"
            label={crashReportsDef.label}
            description={crashReportsDef.description}
            {searchQuery}
        >
            <SettingSwitch id="updates.crashReports" />
        </SettingRow>
    {/if}
</SettingsSection>
