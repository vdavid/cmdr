<script lang="ts">
    import { dismissToast } from '$lib/ui/toast'
    import Button from '$lib/ui/Button.svelte'
    import { openSettingsWindow } from '$lib/settings/settings-window'
    import { tString } from '$lib/intl/messages.svelte'
    import { openErrorReportDialogForAutoSentReport } from './error-report-flow.svelte'
    import { getLastAutoSentReportId } from './auto-send-toast-state.svelte'
    import SentReportToastBody from './SentReportToastBody.svelte'

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

{#snippet actions()}
    <Button size="mini" variant="secondary" onclick={handleChangeSettings}
        >{tString('errorReporter.autoSentToast.changeSettings')}</Button
    >
    <Button size="mini" variant="primary" onclick={handleViewOrAddNotes}
        >{tString('errorReporter.autoSentToast.viewOrAddNotes')}</Button
    >
{/snippet}

<SentReportToastBody
    title={tString('errorReporter.autoSentToast.title')}
    message={tString('errorReporter.autoSentToast.referenceIdLabel')}
    reportId={getLastAutoSentReportId()}
    {actions}
/>
