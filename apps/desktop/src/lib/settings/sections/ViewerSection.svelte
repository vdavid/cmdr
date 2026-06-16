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

    const wordWrapDef = getSettingDefinition('viewer.wordWrap') ?? { label: '', description: '' }
</script>

<SettingsSection title={tString('settings.section.viewer')}>
    {#if shouldShow('viewer.wordWrap')}
        <SettingRow
            id="viewer.wordWrap"
            label={wordWrapDef.label}
            description={wordWrapDef.description}
            {searchQuery}
        >
            <SettingSwitch id="viewer.wordWrap" />
        </SettingRow>
    {/if}
</SettingsSection>
