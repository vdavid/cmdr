<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingRadioGroup from '../components/SettingRadioGroup.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { createShouldShow } from '$lib/settings/settings-search'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const defaultDef = { label: '', description: '', disabled: false, disabledReason: '' }
    const extensionChangesDef = getSettingDefinition('fileOperations.allowFileExtensionChanges') ?? defaultDef
</script>

<SettingsSection title="File operations">
    {#if shouldShow('fileOperations.allowFileExtensionChanges')}
        <SettingRow
            id="fileOperations.allowFileExtensionChanges"
            label={extensionChangesDef.label}
            description={extensionChangesDef.description}
            split
            {searchQuery}
        >
            <SettingRadioGroup id="fileOperations.allowFileExtensionChanges" />
        </SettingRow>
    {/if}
</SettingsSection>
