<script lang="ts">
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import type { CrashReport } from '$lib/tauri-commands'
    import { sendCrashReport, dismissCrashReport } from '$lib/tauri-commands'
    import { setSetting } from '$lib/settings'
    import { getAppLogger } from '$lib/logging/logger'

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
            await sendCrashReport(report)
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
    {#snippet title()}Send crash report?{/snippet}

    <div class="body">
        <p id="crash-report-body" class="description">
            Cmdr quit unexpectedly last time. Here's a crash report with details that can help fix this.
        </p>
        <p class="description privacy-note">
            It includes the app version, macOS version, and which part of the code crashed — no file names or personal
            data.
        </p>

        <!-- Expandable details -->
        <button class="details-toggle" onclick={() => (detailsExpanded = !detailsExpanded)}>
            <span class="toggle-arrow" class:expanded={detailsExpanded}>&#x25B8;</span>
            Show report details
        </button>

        {#if detailsExpanded}
            <div class="details-container">
                <button class="copy-button" onclick={() => void handleCopy()}>
                    {copied ? 'Copied' : 'Copy'}
                </button>
                <pre class="details-json" style="user-select: text">{reportJson}</pre>
            </div>
        {/if}

        <!-- Always send checkbox -->
        <label class="always-send">
            <input type="checkbox" bind:checked={alwaysSend} />
            <span>Always send crash reports</span>
        </label>

        <!-- Actions -->
        <div class="button-row">
            <Button variant="secondary" onclick={handleDismiss} disabled={sending}>Dismiss</Button>
            <Button variant="primary" onclick={() => void handleSend()} disabled={sending}>
                {sending ? 'Sending...' : 'Send report'}
            </Button>
        </div>
    </div>
</ModalDialog>

<style>
    .body {
        padding: 0 var(--spacing-xl) var(--spacing-xl);
    }

    .description {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: 1.5;
    }

    .privacy-note {
        margin-bottom: var(--spacing-lg);
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

    .button-row {
        display: flex;
        gap: var(--spacing-md);
        justify-content: center;
    }
</style>
