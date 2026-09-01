<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { createShouldShow, anyVisible } from '$lib/settings/settings-search'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Button from '$lib/ui/Button.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import { updateState, checkForUpdates } from '$lib/updates/updater.svelte'
    import { formatUpdateStatus } from '$lib/updates/update-status-text'
    import { openErrorReportDialog } from '$lib/error-reporter/error-report-flow.svelte'
    import { createBetaEmailSignup } from './beta-email-signup.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const autoCheckDef = getSettingDefinition('updates.autoCheck') ?? { label: '', description: '' }
    const whatsNewDef = getSettingDefinition('whatsNew.showOnUpdate') ?? { label: '', description: '' }
    const analyticsDef = getSettingDefinition('analytics.enabled') ?? { label: '', description: '' }
    const emailDef = getSettingDefinition('analytics.email') ?? { label: '', description: '' }
    const crashReportsDef = getSettingDefinition('updates.crashReports') ?? { label: '', description: '' }
    const errorReportsDef = getSettingDefinition('updates.errorReports') ?? { label: '', description: '' }

    const statusText = $derived(formatUpdateStatus(updateState))
    const buttonDisabled = $derived(updateState.status !== 'idle')

    // The beta contact email field: persists on every keystroke, subscribes on commit. The logic is
    // shared with the onboarding sheet's `StepBeta`, so both surfaces behave identically.
    const emailSignup = createBetaEmailSignup()

    function handleCheckForUpdates() {
        void checkForUpdates('settings')
    }

    function handleSendErrorReport() {
        openErrorReportDialog(`Update check failed: ${updateState.error ?? ''}`)
    }
</script>

<SettingsSection title={tString('settings.section.updatesAndPrivacy')}>
    {#if anyVisible(shouldShow, 'updates.autoCheck', 'whatsNew.showOnUpdate')}
        <SectionCard label={tString('settings.updates.card.updates')}>
            <div class="check-row">
                <Button variant="secondary" size="mini" onclick={handleCheckForUpdates} disabled={buttonDisabled}>
                    {tString('settings.updates.checkForUpdates')}
                </Button>
                <div class="status">
                    {#if updateState.error !== null}
                        <span class="error-message"
                            >{tString('settings.updates.errorPrefix')} {updateState.error}</span
                        >
                        <button class="link-button" onclick={handleSendErrorReport}
                            >{tString('settings.updates.sendErrorReport')}</button
                        >
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
            {#if shouldShow('whatsNew.showOnUpdate')}
                <SettingRow
                    id="whatsNew.showOnUpdate"
                    label={whatsNewDef.label}
                    description={whatsNewDef.description}
                    {searchQuery}
                >
                    <SettingSwitch id="whatsNew.showOnUpdate" />
                </SettingRow>
            {/if}
        </SectionCard>
    {/if}

    {#if anyVisible(shouldShow, 'analytics.enabled', 'analytics.email', 'updates.crashReports', 'updates.errorReports')}
        <SectionCard label={tString('settings.updates.card.privacyAndDataSharing')}>
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
                <SettingRow
                    id="analytics.email"
                    label={emailDef.label}
                    description={emailDef.description}
                    split
                    {searchQuery}
                >
                    <TextInput
                        type="email"
                        placeholder={tString('settings.updates.emailPlaceholder')}
                        value={emailSignup.email}
                        oninput={emailSignup.handleInput}
                        onblur={emailSignup.handleCommit}
                        onkeydown={emailSignup.handleKeydown}
                        disabled={emailSignup.signupInFlight}
                        ariaLabel={emailDef.label}
                    />
                </SettingRow>
                {#if emailSignup.signupFeedback?.kind === 'success'}
                    <p class="signup-feedback success" role="status">
                        {tString('settings.updates.emailConfirmHint')}
                    </p>
                {:else if emailSignup.signupFeedback?.kind === 'failure'}
                    <p class="signup-feedback failure" role="status">
                        {tString('settings.updates.emailSignupError')}
                    </p>
                {/if}
                <p class="email-note">
                    {tString('settings.updates.emailPrivacyNote')}
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
        </SectionCard>
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

    .error-message {
        color: var(--color-text-primary);
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

    .signup-feedback {
        margin: var(--spacing-xs) 0 0;
        font-size: var(--font-size-sm);
    }

    .signup-feedback.success {
        color: var(--color-toast-success-stripe);
    }

    .signup-feedback.failure {
        color: var(--color-text-primary);
    }

    .email-note {
        margin: var(--spacing-xs) 0 var(--spacing-md);
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
    }
</style>
