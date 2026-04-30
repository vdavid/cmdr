<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { createShouldShow } from '$lib/settings/settings-search'
    import Button from '$lib/ui/Button.svelte'
    import { updateState, checkForUpdates } from '$lib/updates/updater.svelte'
    import { formatUpdateStatus } from '$lib/updates/update-status-text'
    import { openErrorReportDialog } from '$lib/error-reporter/error-report-flow.svelte'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const autoCheckDef = getSettingDefinition('updates.autoCheck') ?? { label: '', description: '' }
    const crashReportsDef = getSettingDefinition('updates.crashReports') ?? { label: '', description: '' }
    const errorReportsDef = getSettingDefinition('updates.errorReports') ?? { label: '', description: '' }

    const statusText = $derived(formatUpdateStatus(updateState))
    const buttonDisabled = $derived(updateState.status !== 'idle')

    function handleCheckForUpdates() {
        void checkForUpdates()
    }

    function handleSendErrorReport() {
        openErrorReportDialog(`Update check failed: ${updateState.error ?? ''}`)
    }
</script>

<SettingsSection title="Updates">
    <div class="check-row">
        <Button variant="secondary" size="mini" onclick={handleCheckForUpdates} disabled={buttonDisabled}>
            Check for updates
        </Button>
        <div class="status">
            {#if updateState.error !== null}
                <span class="error-message">Error: {updateState.error}</span>
                <button class="link-button" onclick={handleSendErrorReport}>Send error report</button>
            {:else if statusText}
                <span class="status-text">{statusText}</span>
            {/if}
        </div>
    </div>
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
    {#if shouldShow('updates.errorReports')}
        <SettingRow
            id="updates.errorReports"
            label={errorReportsDef.label}
            description={errorReportsDef.description}
            {searchQuery}
        >
            <SettingSwitch id="updates.errorReports" />
        </SettingRow>
    {/if}
</SettingsSection>

<style>
    .check-row {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        margin-bottom: var(--spacing-md);
    }

    .status {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        min-height: 1.4em;
    }

    .status-text {
        line-height: 1.4;
    }

    .error-message {
        color: var(--color-text-primary);
        line-height: 1.4;
    }

    .link-button {
        background: none;
        border: none;
        padding: 0;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        cursor: default;
        text-align: left;
        align-self: flex-start;
    }

    .link-button:hover {
        color: var(--color-text-secondary);
    }
</style>
