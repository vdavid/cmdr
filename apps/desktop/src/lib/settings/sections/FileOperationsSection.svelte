<script lang="ts">
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SettingSelect from '../components/SettingSelect.svelte'
    import SettingSlider from '../components/SettingSlider.svelte'
    import { getSettingDefinition } from '$lib/settings'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const confirmDeleteDef = getSettingDefinition('fileOperations.confirmBeforeDelete')!
    const deletePermanentlyDef = getSettingDefinition('fileOperations.deletePermanently')!
    const progressIntervalDef = getSettingDefinition('fileOperations.progressUpdateInterval')!
    const maxConflictsDef = getSettingDefinition('fileOperations.maxConflictsToShow')!
</script>

<div class="section">
    <h2 class="section-title">File operations</h2>

    <SettingRow
        id="fileOperations.confirmBeforeDelete"
        label={confirmDeleteDef.label}
        description={confirmDeleteDef.description}
        disabled={confirmDeleteDef.disabled}
        disabledReason={confirmDeleteDef.disabledReason}
    >
        <SettingSwitch id="fileOperations.confirmBeforeDelete" disabled={confirmDeleteDef.disabled} />
    </SettingRow>

    <SettingRow
        id="fileOperations.deletePermanently"
        label={deletePermanentlyDef.label}
        description={deletePermanentlyDef.description}
        disabled={deletePermanentlyDef.disabled}
        disabledReason={deletePermanentlyDef.disabledReason}
    >
        <SettingSwitch id="fileOperations.deletePermanently" disabled={deletePermanentlyDef.disabled} />
    </SettingRow>

    <SettingRow
        id="fileOperations.progressUpdateInterval"
        label={progressIntervalDef.label}
        description={progressIntervalDef.description}
    >
        <SettingSlider id="fileOperations.progressUpdateInterval" unit="ms" />
    </SettingRow>

    <SettingRow
        id="fileOperations.maxConflictsToShow"
        label={maxConflictsDef.label}
        description={maxConflictsDef.description}
    >
        <SettingSelect id="fileOperations.maxConflictsToShow" />
    </SettingRow>
</div>

<style>
    .section {
        margin-bottom: var(--spacing-md);
    }

    .section-title {
        font-size: 16px;
        font-weight: 600;
        color: var(--color-text-primary);
        margin: 0 0 var(--spacing-sm);
        padding-bottom: var(--spacing-xs);
        border-bottom: 1px solid var(--color-border);
    }
</style>
