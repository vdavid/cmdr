<script module lang="ts">
    /**
     * Module-level slot for the path of the most recently saved debug bundle.
     * `ErrorReportDialog` sets this right before `addToast(BundleSavedToastContent, ...)`.
     * Same prop-bridging pattern as `ErrorReportToastContent`.
     */
    let lastSavedBundlePath = $state('')

    export function setLastSavedBundlePath(path: string): void {
        lastSavedBundlePath = path
    }
</script>

<script lang="ts">
    import { dismissToast } from '$lib/ui/toast'
    import { showInFinder } from '$lib/tauri-commands'

    const toastId = 'error-report-bundle-saved'

    function handleReveal() {
        if (lastSavedBundlePath) {
            void showInFinder(lastSavedBundlePath)
        }
    }

    function handleDismiss() {
        dismissToast(toastId)
    }
</script>

<div class="content">
    <span class="message">Saved bundle to disk</span>
    <span class="path" title={lastSavedBundlePath}>{lastSavedBundlePath}</span>
    <div class="actions">
        <button class="link-button" onclick={handleReveal}>Reveal in Finder</button>
        <button class="link-button" onclick={handleDismiss}>Dismiss</button>
    </div>
</div>

<style>
    .content {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
    }

    .message {
        color: var(--color-text-primary);
        line-height: 1.4;
    }

    .path {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
        max-width: 320px;
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
