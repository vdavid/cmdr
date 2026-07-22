<script lang="ts">
    /**
     * Persistent WARN toast for the low-disk-space warning.
     *
     * Live-follows the boot volume: the space numbers seed from the props at
     * creation, then track the `volume-space-changed` stream (the boot-volume
     * watcher already emits it every tick while the warning is on), so the
     * readout stays honest as the disk fills or drains. The toast itself is
     * shown and dismissed by the event-bridge on the backend's discrete
     * `low-disk-space` edges (`is_low` true/false); this component only keeps
     * the numbers current while it's up.
     *
     * "Disable these notifications" flips the setting to `'off'` (the
     * settings-applier pushes the change to the backend poller live) and
     * deep-links to the Low disk space sub-group so the user sees where to
     * re-enable it.
     */
    import { onMount } from 'svelte'
    import { dismissToast } from '$lib/ui/toast'
    import Button from '$lib/ui/Button.svelte'
    import { formatFileSizeWithFormat } from '$lib/settings/format-utils'
    import { getFileSizeFormat } from '$lib/settings/reactive-settings.svelte'
    import { onVolumeSpaceChanged } from '$lib/tauri-commands'
    import { setLowDiskSpaceNotificationsMode, openSettingsToLowDiskSpace } from './notifications-mode'
    import { getAppLogger } from '$lib/logging/logger'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        toastId: string
        /** Boot volume id, so we only track its space updates. */
        volumeId: string
        availableBytes: number
        totalBytes: number
    }

    const { toastId, volumeId, availableBytes, totalBytes }: Props = $props()
    const log = getAppLogger('low-disk-space')

    // Seed from the snapshot, then live-follow the backend stream.
    let available = $state(availableBytes)
    let total = $state(totalBytes)

    // Mirror the backend's `free_percent`: an unknown total reads as 100 (not low)
    // so a bogus fetch can't render a nonsense percentage.
    const freePercent = $derived(total === 0 ? 100 : (available / total) * 100)
    const freeText = $derived(formatFileSizeWithFormat(available, getFileSizeFormat()))
    const percentText = $derived(freePercent.toFixed(1))

    onMount(() => {
        let unlisten: (() => void) | undefined
        let disposed = false
        void onVolumeSpaceChanged((payload) => {
            if (payload.volumeId !== volumeId) return
            available = payload.availableBytes
            total = payload.totalBytes
        }).then((fn) => {
            // If the toast was dismissed before the listener resolved, unsubscribe now.
            if (disposed) fn()
            else unlisten = fn
        })
        return () => {
            disposed = true
            unlisten?.()
        }
    })

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
