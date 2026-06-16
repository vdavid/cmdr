<script lang="ts">
    /**
     * Persistent WARN toast for the low-disk-space warning.
     *
     * Props-only: the space numbers are snapshotted at toast-creation time.
     * The backend's hysteresis means one toast per crossing, and the dedup id
     * (set by the bridge) replaces the content in place if a re-fire lands
     * while the toast is still visible.
     *
     * "Disable these notifications" flips the setting to `'off'` (the
     * settings-applier pushes the change to the backend poller live) and
     * deep-links to the Low disk space sub-group so the user sees where to
     * re-enable it.
     */
    import { dismissToast } from '$lib/ui/toast'
    import Button from '$lib/ui/Button.svelte'
    import { formatFileSizeWithFormat } from '$lib/settings/format-utils'
    import { getFileSizeFormat } from '$lib/settings/reactive-settings.svelte'
    import { setLowDiskSpaceNotificationsMode, openSettingsToLowDiskSpace } from './notifications-mode'
    import { getAppLogger } from '$lib/logging/logger'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        toastId: string
        availableBytes: number
        freePercent: number
    }

    const { toastId, availableBytes, freePercent }: Props = $props()
    const log = getAppLogger('low-disk-space')

    const freeText = formatFileSizeWithFormat(availableBytes, getFileSizeFormat())
    const percentText = freePercent.toFixed(1)

    async function handleDisable(): Promise<void> {
        setLowDiskSpaceNotificationsMode('off')
        dismissToast(toastId)
        try {
            await openSettingsToLowDiskSpace()
        } catch (err) {
            log.warn('Failed to open Settings from the low-disk-space toast: {err}', { err: String(err) })
        }
    }
</script>

<div class="content">
    <span class="message">
        {tString('lowDiskSpace.toast.message', { freeText, percentText })}
    </span>
    <div class="actions">
        <Button size="mini" variant="secondary" onclick={() => void handleDisable()}
            >{tString('lowDiskSpace.toast.disable')}</Button
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
