<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SettingCheckbox from '../components/SettingCheckbox.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { createShouldShow } from '$lib/settings/settings-search'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const defaultDef = { label: '', description: '' }
    const mtpEnabledDef = getSettingDefinition('fileOperations.mtpEnabled') ?? defaultDef
    const mtpWarningDef = getSettingDefinition('fileOperations.mtpConnectionWarning') ?? defaultDef
</script>

<SettingsSection title="MTP">
    {#if shouldShow('fileOperations.mtpEnabled')}
        <SettingRow
            id="fileOperations.mtpEnabled"
            label={mtpEnabledDef.label}
            description={mtpEnabledDef.description}
            {searchQuery}
        >
            <SettingSwitch id="fileOperations.mtpEnabled" />
        </SettingRow>
    {/if}

    {#if shouldShow('fileOperations.mtpConnectionWarning')}
        <div class="secondary-setting">
            <SettingRow
                id="fileOperations.mtpConnectionWarning"
                label={mtpWarningDef.label}
                description={mtpWarningDef.description}
                {searchQuery}
            >
                <SettingCheckbox id="fileOperations.mtpConnectionWarning" />
            </SettingRow>
        </div>
    {/if}
</SettingsSection>

<style>
    .secondary-setting {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }
</style>
