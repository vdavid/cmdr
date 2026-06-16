<script lang="ts">
    import { dismissToast } from '$lib/ui/toast'
    import Button from '$lib/ui/Button.svelte'
    import { showInFinder } from '$lib/tauri-commands'
    import { tString } from '$lib/intl/messages.svelte'
    import { getLastSavedBundlePath } from './bundle-saved-toast-state.svelte'

    const toastId = 'error-report-bundle-saved'

    function handleReveal() {
        const path = getLastSavedBundlePath()
        if (path) {
            void showInFinder(path)
        }
    }

    function handleDismiss() {
        dismissToast(toastId)
    }
</script>

<div class="content">
    <span class="message">{tString('errorReporter.bundleSavedToast.message')}</span>
    <span class="path" title={getLastSavedBundlePath()}>{getLastSavedBundlePath()}</span>
    <div class="actions">
        <Button size="mini" variant="secondary" onclick={handleDismiss}
            >{tString('errorReporter.bundleSavedToast.dismiss')}</Button
        >
        <Button size="mini" variant="primary" onclick={handleReveal}
            >{tString('errorReporter.bundleSavedToast.reveal')}</Button
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
        justify-content: flex-end;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-md);
    }
</style>
