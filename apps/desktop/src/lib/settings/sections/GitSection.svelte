<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { createShouldShow } from '$lib/settings/settings-search'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const defaultDef = { label: '', description: '' }
    const repoChipDef = getSettingDefinition('fileExplorer.git.showRepoChip') ?? defaultDef
    const statusColumnDef = getSettingDefinition('fileExplorer.git.showStatusColumn') ?? defaultDef
    const virtualPortalDef = getSettingDefinition('fileExplorer.git.showVirtualGitPortal') ?? defaultDef
</script>

<SettingsSection title={tString('settings.section.git')}>
    {#if shouldShow('fileExplorer.git.showRepoChip')}
        <SettingRow
            id="fileExplorer.git.showRepoChip"
            label={repoChipDef.label}
            description={repoChipDef.description}
            {searchQuery}
        >
            <SettingSwitch id="fileExplorer.git.showRepoChip" />
        </SettingRow>
    {/if}
    {#if shouldShow('fileExplorer.git.showStatusColumn')}
        <SettingRow
            id="fileExplorer.git.showStatusColumn"
            label={statusColumnDef.label}
            description={statusColumnDef.description}
            {searchQuery}
        >
            <SettingSwitch id="fileExplorer.git.showStatusColumn" />
        </SettingRow>
    {/if}
    {#if shouldShow('fileExplorer.git.showVirtualGitPortal')}
        <SettingRow
            id="fileExplorer.git.showVirtualGitPortal"
            label={virtualPortalDef.label}
            description={virtualPortalDef.description}
            {searchQuery}
        >
            <SettingSwitch id="fileExplorer.git.showVirtualGitPortal" />
        </SettingRow>
    {/if}
</SettingsSection>
