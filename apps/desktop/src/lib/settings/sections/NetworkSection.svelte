<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSelect from '../components/SettingSelect.svelte'
    import SettingRadioGroup from '../components/SettingRadioGroup.svelte'
    import SettingNumberInput from '../components/SettingNumberInput.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { createShouldShow } from '$lib/settings/settings-search'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const defaultDef = { label: '', description: '' }
    const cacheDurationDef = getSettingDefinition('network.shareCacheDuration') ?? defaultDef
    const timeoutModeDef = getSettingDefinition('network.timeoutMode') ?? defaultDef
</script>

<SettingsSection title="SMB/Network shares">
    {#if shouldShow('network.shareCacheDuration')}
        <SettingRow
            id="network.shareCacheDuration"
            label={cacheDurationDef.label}
            description={cacheDurationDef.description}
            {searchQuery}
        >
            <SettingSelect id="network.shareCacheDuration" />
        </SettingRow>
    {/if}

    {#if shouldShow('network.timeoutMode')}
        <SettingRow
            id="network.timeoutMode"
            label={timeoutModeDef.label}
            description={timeoutModeDef.description}
            {searchQuery}
        >
            <div class="timeout-setting">
                <SettingRadioGroup id="network.timeoutMode">
                    {#snippet customContent(value)}
                        {#if value === 'custom'}
                            <div class="custom-timeout">
                                <SettingNumberInput id="network.customTimeout" unit="seconds" />
                            </div>
                        {/if}
                    {/snippet}
                </SettingRadioGroup>
            </div>
        </SettingRow>
    {/if}
</SettingsSection>

<style>
    .timeout-setting {
        width: 100%;
    }

    .custom-timeout {
        margin-top: var(--spacing-xs);
    }
</style>
