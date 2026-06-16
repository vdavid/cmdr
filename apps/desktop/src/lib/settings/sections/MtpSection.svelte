<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SettingCheckbox from '../components/SettingCheckbox.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { createShouldShow, anyVisible } from '$lib/settings/settings-search'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const defaultDef = { label: '', description: '' }
    const mtpEnabledDef = getSettingDefinition('fileOperations.mtpEnabled') ?? defaultDef
    const mtpWarningDef = getSettingDefinition('fileOperations.mtpConnectionWarning') ?? defaultDef
</script>

<SettingsSection title={tString('settings.section.mtp')}>
    {#if anyVisible(shouldShow, 'fileOperations.mtpEnabled', 'fileOperations.mtpConnectionWarning')}
        <SectionCard>
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
        </SectionCard>
    {/if}
</SettingsSection>

<style>
    .secondary-setting {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }
</style>
