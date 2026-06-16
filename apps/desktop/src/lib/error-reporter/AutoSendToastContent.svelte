<script lang="ts">
    import { dismissToast } from '$lib/ui/toast'
    import Button from '$lib/ui/Button.svelte'
    import { openSettingsWindow } from '$lib/settings/settings-window'
    import { tString } from '$lib/intl/messages.svelte'
    import { openErrorReportDialog } from './error-report-flow.svelte'
    import { getLastAutoSentReportId } from './auto-send-toast-state.svelte'

    const TOAST_ID = 'error-report-auto-sent'

    function handleView() {
        // Reuse Flow A's preview dialog so the user can inspect what was just sent.
        // The dialog re-builds the bundle locally (same inputs, deterministic output)
        // so what they see matches what shipped (modulo the timestamp).
        dismissToast(TOAST_ID)
        openErrorReportDialog()
    }

    function handleChangeSettings() {
        dismissToast(TOAST_ID)
        void openSettingsWindow()
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
        <Button size="mini" variant="primary" onclick={handleView}
            >{tString('errorReporter.autoSentToast.view')}</Button
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
        line-height: 1.4;
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
