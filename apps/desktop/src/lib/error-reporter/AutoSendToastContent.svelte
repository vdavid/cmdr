<script module lang="ts">
    /**
     * Module-level slot for the most recently auto-sent report ID. Mirrors the bridge
     * pattern used by `ErrorReportToastContent` and `MtpConnectedToastContent`: the
     * toast helper sets the ID right before `addToast(...)` so the rendered toast can
     * read it without prop bridging.
     */
    let lastAutoSentReportId = $state('')

    export function setLastAutoSentReportId(id: string): void {
        lastAutoSentReportId = id
    }

    export function getLastAutoSentReportId(): string {
        return lastAutoSentReportId
    }
</script>

<script lang="ts">
    import { dismissToast } from '$lib/ui/toast'
    import { openSettingsWindow } from '$lib/settings/settings-window'
    import { openErrorReportDialog } from './error-report-flow.svelte'

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
    <div class="title">Error report sent</div>
    <div class="body">
        Reference ID:
        <span class="id-badge">{lastAutoSentReportId}</span>
    </div>
    <div class="actions">
        <button class="link-button" onclick={handleView}>View</button>
        <button class="link-button" onclick={handleChangeSettings}>Change settings</button>
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
        gap: var(--spacing-md);
    }

    .link-button {
        background: none;
        border: none;
        padding: 0;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        cursor: default;
    }

    .link-button:hover {
        color: var(--color-text-secondary);
    }
</style>
