<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
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
    const confirmDeleteDef = getSettingDefinition('fileOperations.confirmBeforeDelete') ?? defaultDef
    const deletePermanentlyDef = getSettingDefinition('fileOperations.deletePermanently') ?? defaultDef
    const extensionChangesDef = getSettingDefinition('fileOperations.allowFileExtensionChanges') ?? defaultDef
    const progressIntervalDef = getSettingDefinition('fileOperations.progressUpdateInterval') ?? defaultDef
    const maxConflictsDef = getSettingDefinition('fileOperations.maxConflictsToShow') ?? defaultDef
</script>

<SettingsSection title="File operations">
    {#if shouldShow('fileOperations.confirmBeforeDelete')}
        <SettingRow
            id="fileOperations.confirmBeforeDelete"
            label={confirmDeleteDef.label}
            description={confirmDeleteDef.description}
            disabled={confirmDeleteDef.disabled}
            disabledReason={confirmDeleteDef.disabledReason}
            {searchQuery}
        >
            <SettingSwitch id="fileOperations.confirmBeforeDelete" disabled={confirmDeleteDef.disabled} />
        </SettingRow>
    {/if}

    {#if shouldShow('fileOperations.deletePermanently')}
        <SettingRow
            id="fileOperations.deletePermanently"
            label={deletePermanentlyDef.label}
            description={deletePermanentlyDef.description}
            disabled={deletePermanentlyDef.disabled}
            disabledReason={deletePermanentlyDef.disabledReason}
            {searchQuery}
        >
            <SettingSwitch id="fileOperations.deletePermanently" disabled={deletePermanentlyDef.disabled} />
        </SettingRow>
    {/if}

    {#if shouldShow('fileOperations.allowFileExtensionChanges')}
        <SettingRow
            id="fileOperations.allowFileExtensionChanges"
            label={extensionChangesDef.label}
            description={extensionChangesDef.description}
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
            {searchQuery}
        >
            <SettingSelect id="fileOperations.maxConflictsToShow" />
        </SettingRow>
    {/if}
</SettingsSection>
