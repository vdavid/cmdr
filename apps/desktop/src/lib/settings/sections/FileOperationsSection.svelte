<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSelect from '../components/SettingSelect.svelte'
    import SettingSlider from '../components/SettingSlider.svelte'
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
    const progressIntervalDef = getSettingDefinition('fileOperations.progressUpdateInterval') ?? defaultDef
    const maxConflictsDef = getSettingDefinition('fileOperations.maxConflictsToShow') ?? defaultDef
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

    {#if shouldShow('fileOperations.progressUpdateInterval')}
        <SettingRow
            id="fileOperations.progressUpdateInterval"
            label={progressIntervalDef.label}
            description={progressIntervalDef.description}
            split
            {searchQuery}
        >
            <SettingSlider id="fileOperations.progressUpdateInterval" unit="ms" />
        </SettingRow>
    {/if}

    {#if shouldShow('fileOperations.maxConflictsToShow')}
        <SettingRow
            id="fileOperations.maxConflictsToShow"
            label={maxConflictsDef.label}
            description={maxConflictsDef.description}
            split
            {searchQuery}
        >
            <SettingSelect id="fileOperations.maxConflictsToShow" />
        </SettingRow>
    {/if}
</SettingsSection>
