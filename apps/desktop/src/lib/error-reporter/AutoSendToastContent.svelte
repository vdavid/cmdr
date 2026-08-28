<script lang="ts">
    import { dismissToast } from '$lib/ui/toast'
    import Button from '$lib/ui/Button.svelte'
    import { openSettingsWindow } from '$lib/settings/settings-window'
    import { tString } from '$lib/intl/messages.svelte'
    import { openErrorReportDialogForAutoSentReport } from './error-report-flow.svelte'
    import { getLastAutoSentReportId } from './auto-send-toast-state.svelte'

    const TOAST_ID = 'error-report-auto-sent'

    function handleViewOrAddNotes() {
        // Amend mode, ❌ never the compose entry point: this dialog shows the bundle that
        // actually shipped and adds the note to THAT report, so one incident stays one
        // report with one id.
        dismissToast(TOAST_ID)
        openErrorReportDialogForAutoSentReport()
    }

    function handleChangeSettings() {
        dismissToast(TOAST_ID)
        void openSettingsWindow('error-toast')
    }
</script>

<div class="content">
    <div class="title">{tString('errorReporter.autoSentToast.title')}</div>
    <div class="body">
        {tString('errorReporter.autoSentToast.referenceIdLabel')}
        <span class="id-badge">{getLastAutoSentReportId()}</span>
    </div>
    <div class="actions">
        <Button size="mini" variant="secondary" onclick={handleChangeSettings}
            >{tString('errorReporter.autoSentToast.changeSettings')}</Button
        >
        <Button size="mini" variant="primary" onclick={handleViewOrAddNotes}
            >{tString('errorReporter.autoSentToast.viewOrAddNotes')}</Button
        >
    </div>
</div>

<style>
    .content {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
    }

    .title {
        color: var(--color-text-primary);
        font-weight: 600;
    }

    .body {
        color: var(--color-text-primary);
    }

    .id-badge {
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        background: var(--color-bg-tertiary);
        padding: 0 var(--spacing-xs);
        border-radius: var(--radius-sm);
        white-space: nowrap;
    }

    .actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-md);
    }
</style>
