<script lang="ts">
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import type { CrashReport } from '$lib/tauri-commands'
    import { sendCrashReport, dismissCrashReport } from '$lib/tauri-commands'
    import { getSetting, setSetting } from '$lib/settings'
    import { getAppLogger } from '$lib/logging/logger'
    import { t, tString } from '$lib/intl/messages.svelte'

    const log = getAppLogger('crashReportDialog')

    interface Props {
        report: CrashReport
        onClose: () => void
    }

    const { report, onClose }: Props = $props()

    let detailsExpanded = $state(false)
    let alwaysSend = $state(false)
    let sending = $state(false)
    let copied = $state(false)

    // The beta contact email, if the user added one. Drives whether the attach-email
    // checkbox shows at all. Trimmed so a stray-space value doesn't count as "on file."
    const contactEmail = $derived(getSetting('analytics.email').trim())
    // Sticky default from the last choice (Advanced toggle or a prior report). Never
    // pre-ticked on first use: the registry default is false.
    let attachEmail = $state(getSetting('updates.attachEmailToReports'))

    const reportJson = $derived(JSON.stringify(report, null, 2))

    async function handleCopy() {
        await navigator.clipboard.writeText(reportJson)
        copied = true
        setTimeout(() => (copied = false), 2000)
    }

    async function handleSend() {
        sending = true
        try {
            if (alwaysSend) {
                setSetting('updates.crashReports', true)
            }
            // Remember the attach-email choice (sticky) and include the email only when
            // the box is checked AND an email is on file.
            if (contactEmail) {
                setSetting('updates.attachEmailToReports', attachEmail)
            }
            const reportToSend: CrashReport =
                attachEmail && contactEmail ? { ...report, email: contactEmail } : report
            await sendCrashReport(reportToSend)
            log.info('Crash report sent')
        } catch (e) {
            log.warn('Crash report send attempt returned an error: {error}', { error: String(e) })
        }
        onClose()
    }

    async function handleDismiss() {
        try {
            await dismissCrashReport()
        } catch {
            // Best effort
        }
        onClose()
    }

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter' && !sending) {
            void handleSend()
        }
    }
</script>

<ModalDialog
    titleId="crash-report-dialog-title"
    onkeydown={handleKeydown}
    dialogId="crash-report"
    role="dialog"
    onclose={handleDismiss}
    ariaDescribedby="crash-report-body"
    containerStyle="width: 440px"
>
    {#snippet title()}{tString('crashReporter.dialog.title')}{/snippet}

    <div class="body">
        <p id="crash-report-body" class="description">
            {tString('crashReporter.dialog.body')}
        </p>
        <p class="description privacy-note">
            {tString('crashReporter.dialog.privacyNote')}
        </p>

        {#if report.shortId}
            <p class="description short-id-line">
                {tString('crashReporter.dialog.reportIdLabel')}
                <code class="short-id">{report.shortId}</code>
                <span class="short-id-help">{tString('crashReporter.dialog.reportIdHelp')}</span>
            </p>
        {/if}

        <!-- Expandable details -->
        <button class="details-toggle" onclick={() => (detailsExpanded = !detailsExpanded)}>
            <span class="toggle-arrow" class:expanded={detailsExpanded}>&#x25B8;</span>
            {tString('crashReporter.dialog.showDetails')}
        </button>

        {#if detailsExpanded}
            <div class="details-container">
                <button class="copy-button" onclick={() => void handleCopy()}>
                    {copied ? tString('crashReporter.dialog.copied') : tString('crashReporter.dialog.copy')}
                </button>
                <pre class="details-json" style="user-select: text">{reportJson}</pre>
            </div>
        {/if}

        <!-- Always send checkbox -->
        <label class="always-send">
            <input type="checkbox" bind:checked={alwaysSend} />
            <span>{tString('crashReporter.dialog.alwaysSend')}</span>
        </label>

        <!-- Attach-email checkbox, shown only when a beta contact email is on file -->
        {#if contactEmail}
            <label class="always-send">
                <input type="checkbox" bind:checked={attachEmail} />
                <span>{t('crashReporter.dialog.attachEmail', { email: contactEmail })}</span>
            </label>
        {/if}
    </div>

    {#snippet footer()}
        <Button variant="secondary" onclick={handleDismiss} disabled={sending}
            >{tString('crashReporter.dialog.dismiss')}</Button
        >
        <Button variant="primary" onclick={() => void handleSend()} disabled={sending}>
            {sending ? tString('crashReporter.dialog.sending') : tString('crashReporter.dialog.send')}
        </Button>
    {/snippet}
</ModalDialog>

<style>
    .body {
        padding: 0 var(--spacing-xl);
    }

    .description {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: 1.5;
    }

    .privacy-note {
        margin-bottom: var(--spacing-md);
    }

    .short-id-line {
        margin-bottom: var(--spacing-lg);
    }

    .short-id {
        font-family: var(--font-mono);
        background: var(--color-bg-secondary);
        padding: 0 var(--spacing-xs);
        border-radius: var(--radius-sm);
        user-select: text;
    }

    .short-id-help {
        display: block;
        margin-top: var(--spacing-xs);
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
    }

    .details-toggle {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        background: none;
        border: none;
        padding: 0;
        margin-bottom: var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        cursor: default;
    }

    .details-toggle:hover {
        color: var(--color-text-primary);
    }

    .toggle-arrow {
        display: inline-block;
        transition: transform var(--transition-base);
    }

    .toggle-arrow.expanded {
        transform: rotate(90deg);
    }

    .details-container {
        position: relative;
        margin-bottom: var(--spacing-md);
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-md);
        max-height: 200px;
        overflow-y: auto;
    }

    .copy-button {
        position: sticky;
        top: var(--spacing-xs);
        float: right;
        margin: var(--spacing-xs) var(--spacing-xs) 0 0;
        padding: var(--spacing-xxs) var(--spacing-sm);
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        cursor: default;
        z-index: 1;
    }

    .copy-button:hover {
        color: var(--color-text-primary);
    }

    .details-json {
        margin: 0;
        padding: var(--spacing-sm) var(--spacing-md);
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        white-space: pre-wrap;
        word-break: break-all;
    }

    .always-send {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        margin-bottom: var(--spacing-lg);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        cursor: default;
    }

    .always-send input[type='checkbox'] {
        accent-color: var(--color-accent);
    }
</style>
