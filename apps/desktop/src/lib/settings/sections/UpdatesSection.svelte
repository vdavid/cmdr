<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import { getSetting, getSettingDefinition, setSetting } from '$lib/settings'
    import { onSpecificSettingChange } from '$lib/settings/settings-store'
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
    const analyticsDef = getSettingDefinition('analytics.enabled') ?? { label: '', description: '' }
    const emailDef = getSettingDefinition('analytics.email') ?? { label: '', description: '' }
    const crashReportsDef = getSettingDefinition('updates.crashReports') ?? { label: '', description: '' }
    const errorReportsDef = getSettingDefinition('updates.errorReports') ?? { label: '', description: '' }

    const statusText = $derived(formatUpdateStatus(updateState))
    const buttonDisabled = $derived(updateState.status !== 'idle')

    // The beta contact email persists to settings here. The beta-signup network call (subscribing
    // the address to the mailing list) lands in a later milestone.
    let email = $state(getSetting('analytics.email'))
    onSpecificSettingChange('analytics.email', (value) => {
        email = value
    })

    function handleEmailInput(event: Event) {
        const target = event.currentTarget as HTMLInputElement
        email = target.value
        setSetting('analytics.email', target.value)
    }

    function handleCheckForUpdates() {
        void checkForUpdates()
    }

    function handleSendErrorReport() {
        openErrorReportDialog(`Update check failed: ${updateState.error ?? ''}`)
    }
</script>

<SettingsSection title="Updates & privacy">
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
    {#if shouldShow('analytics.enabled')}
        <SettingRow
            id="analytics.enabled"
            label={analyticsDef.label}
            description={analyticsDef.description}
            {searchQuery}
        >
            <SettingSwitch id="analytics.enabled" />
        </SettingRow>
    {/if}
    {#if shouldShow('analytics.email')}
        <SettingRow id="analytics.email" label={emailDef.label} description={emailDef.description} split {searchQuery}>
            <input
                type="email"
                class="email-input"
                placeholder="you@example.com"
                value={email}
                oninput={handleEmailInput}
                aria-label={emailDef.label}
            />
        </SettingRow>
        <p class="email-note">
            Stored only on your Mac. We never send it together with your anonymous usage data, so your stats can't be
            tied back to you. Used only to reach out and to optionally attach to a report you send.
        </p>
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

    .email-input {
        width: 100%;
        padding: var(--spacing-xs) var(--spacing-sm);
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        background: var(--color-bg-primary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
    }

    .email-input:focus {
        outline: none;
        border-color: var(--color-accent);
        box-shadow: var(--shadow-focus);
    }

    .email-note {
        margin: var(--spacing-xs) 0 var(--spacing-md);
        font-size: var(--font-size-xs);
        line-height: 1.5;
        color: var(--color-text-secondary);
    }
</style>
