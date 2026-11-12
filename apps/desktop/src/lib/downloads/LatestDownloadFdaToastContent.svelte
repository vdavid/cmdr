<script lang="ts">
    import { dismissToast } from '#lib/ui/toast/index.js'
    import Button from '#lib/ui/Button.svelte'
    import ToastActions from '#lib/ui/toast/ToastActions.svelte'
    import { openPrivacySettings } from '#lib/tauri-commands/index.js'
    import { getAppLogger } from '#lib/logging/logger.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { hasCapability } from '#lib/platform/index.js'
    import { LATEST_DOWNLOAD_FDA_TOAST_ID } from './go-to-latest-ids'

    const log = getAppLogger('downloads')

    // Only where the OS has a Full Disk Access grant is there a pane to open. Elsewhere the
    // watcher is down because it couldn't attach to the folder, and the message says so.
    const offersFullDiskAccess = hasCapability('fullDiskAccess')

    async function handleOpenSystemSettings() {
        try {
            await openPrivacySettings()
        } catch (error) {
            // The toast stays up, so the button is still there to try again.
            log.warn("Couldn't open System Settings from the latest-download toast: {error}", { error })
            return
        }
        dismissToast(LATEST_DOWNLOAD_FDA_TOAST_ID)
    }

    function handleDismiss() {
        dismissToast(LATEST_DOWNLOAD_FDA_TOAST_ID)
    }
</script>

<div class="content">
    <span class="message">{tString('downloads.fda.message')}</span>
    <ToastActions>
        <Button size="mini" variant="secondary" onclick={handleDismiss}>{tString('downloads.fda.dismiss')}</Button>
        {#if offersFullDiskAccess}
            <Button size="mini" variant="primary" onclick={handleOpenSystemSettings}
                >{tString('downloads.fda.openSystemSettings')}</Button
            >
        {/if}
    </ToastActions>
</div>

<style>
    .content {
        font-size: var(--font-size-sm);
    }

    .message {
        color: var(--color-text-primary);
    }
</style>
