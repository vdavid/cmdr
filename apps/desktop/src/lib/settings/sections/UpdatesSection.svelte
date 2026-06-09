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
    import { betaSignup } from '$lib/tauri-commands'

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

    // The beta contact email persists to settings on every keystroke (local only). On commit (blur
    // or Enter) with a valid address, we subscribe it to the beta mailing list via `betaSignup`,
    // which sends ONLY the email (never an install id), so usage stats can't be tied back to it.
    let email = $state(getSetting('analytics.email'))
    onSpecificSettingChange('analytics.email', (value) => {
        email = value
    })

    // The inline result under the field. A typed kind, not a parsed message.
    type SignupFeedback = { kind: 'success' | 'failure' } | null
    let signupFeedback = $state<SignupFeedback>(null)
    // The last address we successfully submitted, so re-blurring an unchanged field doesn't resend.
    let lastSubmittedEmail = $state('')
    let signupInFlight = $state(false)

    const emailPattern = /^[^\s@]+@[^\s@]+\.[^\s@]+$/

    function handleEmailInput(event: Event) {
        const target = event.target as HTMLInputElement
        email = target.value
        setSetting('analytics.email', target.value)
        // Clearing the field only clears the local copy. Unsubscribing from the list happens via
        // Listmonk's own link, per the field note.
        if (target.value.trim() === '') {
            signupFeedback = null
            lastSubmittedEmail = ''
        }
    }

    async function handleEmailCommit() {
        const trimmed = email.trim()
        if (trimmed === '' || trimmed === lastSubmittedEmail || !emailPattern.test(trimmed)) {
            return
        }

        signupInFlight = true
        try {
            const result = await betaSignup(trimmed)
            if (result.kind === 'subscribed') {
                signupFeedback = { kind: 'success' }
                lastSubmittedEmail = trimmed
            } else {
                // `invalidEmail` or `softFailure`: a gentle try-again either way.
                signupFeedback = { kind: 'failure' }
            }
        } finally {
            signupInFlight = false
        }
    }

    function handleEmailKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter') {
            void handleEmailCommit()
        }
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
                onblur={handleEmailCommit}
                onkeydown={handleEmailKeydown}
                disabled={signupInFlight}
                aria-label={emailDef.label}
            />
        </SettingRow>
        {#if signupFeedback?.kind === 'success'}
            <p class="signup-feedback success" role="status">
                Check your inbox to confirm your email. Thanks for helping out!
            </p>
        {:else if signupFeedback?.kind === 'failure'}
            <p class="signup-feedback failure" role="status">
                Sorry, we couldn't sign you up right now. Try again?
            </p>
        {/if}
        <p class="email-note">
            Stored only on your Mac. We never send it together with your anonymous usage data, so your stats can't be
            tied back to you. Used only to reach out and to optionally attach to a report you send. To stop getting
            emails, use the unsubscribe link in any message we send.
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

    .email-input:disabled {
        opacity: 0.6;
    }

    .signup-feedback {
        margin: var(--spacing-xs) 0 0;
        font-size: var(--font-size-sm);
        line-height: 1.4;
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
        line-height: 1.5;
        color: var(--color-text-secondary);
    }
</style>
