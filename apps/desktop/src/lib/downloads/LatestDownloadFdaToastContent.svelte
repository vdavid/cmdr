<script lang="ts">
    import { dismissToast } from '$lib/ui/toast'
    import Button from '$lib/ui/Button.svelte'
    import { openPrivacySettings } from '$lib/tauri-commands'
    import { tString } from '$lib/intl/messages.svelte'
    import { LATEST_DOWNLOAD_FDA_TOAST_ID } from './go-to-latest-ids'

    async function handleOpenSystemSettings() {
        await openPrivacySettings()
        dismissToast(LATEST_DOWNLOAD_FDA_TOAST_ID)
    }

    function handleDismiss() {
        dismissToast(LATEST_DOWNLOAD_FDA_TOAST_ID)
    }
</script>

<div class="content">
    <span class="message">{tString('downloads.fda.message')}</span>
    <div class="actions">
        <Button size="mini" variant="secondary" onclick={handleDismiss}>{tString('downloads.fda.dismiss')}</Button>
        <Button size="mini" variant="primary" onclick={handleOpenSystemSettings}
            >{tString('downloads.fda.openSystemSettings')}</Button
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

    .actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-md);
    }
</style>
