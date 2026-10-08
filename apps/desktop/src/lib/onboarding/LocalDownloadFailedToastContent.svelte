<script lang="ts">
    /**
     * The toast `local-download-notice.ts` raises after onboarding when the local AI model
     * didn't finish downloading. Without it, the only clue is the Download button reappearing
     * in Settings, and the user thinks the local model is ready. The button goes to where that
     * Download button lives.
     */
    import Button from '$lib/ui/Button.svelte'
    import { dismissToast } from '$lib/ui/toast'
    import { tString } from '$lib/intl/messages.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import { openSettingsWindow } from '$lib/settings/settings-window'

    const { toastId }: { toastId: string } = $props()

    const log = getAppLogger('onboarding')

    async function handleOpenSettings(): Promise<void> {
        try {
            await openSettingsWindow('local-download-toast', ['AI', 'Provider'])
        } catch (error) {
            // The toast stays up, so the button is still there to try again.
            log.warn("Couldn't open Settings from the local AI download toast: {error}", { error: String(error) })
            return
        }
        dismissToast(toastId)
    }
</script>

<div class="content">
    <strong class="title">{tString('onboarding.localDownloadFailed.title')}</strong>
    <span class="body">{tString('onboarding.localDownloadFailed.body')}</span>
    <div class="actions">
        <Button size="mini" variant="primary" onclick={handleOpenSettings}>
            {tString('onboarding.localDownloadFailed.openSettings')}
        </Button>
    </div>
</div>

<style>
    .content {
        font-size: var(--font-size-sm);
    }

    /* Its own line, while the root stays block flow so the body wraps around the toast's corner. */
    .title {
        display: block;
        color: var(--color-text-primary);
        font-weight: 600;
    }

    .body {
        color: var(--color-text-primary);
    }

    .actions {
        display: flex;
        justify-content: flex-end;
        margin-top: calc(var(--spacing-xs) + var(--spacing-xxs));
    }
</style>
