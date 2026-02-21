<script lang="ts">
    import { isMtpVolumeId, constructMtpPath } from '$lib/mtp'
    import { connect as connectMtpDevice } from '$lib/mtp/mtp-store.svelte'
    import { getAppLogger } from '$lib/logger'

    const log = getAppLogger('mtpConnection')

    interface Props {
        volumeId: string
        onVolumeChange?: (volumeId: string, volumePath: string, targetPath: string) => void
    }

    const { volumeId, onVolumeChange }: Props = $props()

    // Check if this is a device-only MTP ID (needs connection)
    // Device-only IDs start with "mtp-" but don't contain ":" (no storage ID)
    const isMtpDeviceOnly = $derived(isMtpVolumeId(volumeId) && volumeId.startsWith('mtp-') && !volumeId.includes(':'))

    // MTP connection state for device-only IDs
    let mtpConnecting = $state(false)
    let mtpConnectionError = $state<string | null>(null)
    // Track the device ID we've successfully connected to, to prevent re-triggering auto-connect
    // while waiting for the parent to update volumeId after onVolumeChange
    let mtpConnectedDeviceId = $state<string | null>(null)

    // Effect: Reset connected device ID when we're no longer on a device-only MTP volume
    // This runs when the volume change completes and we switch to a storage-specific ID
    $effect(() => {
        if (!isMtpDeviceOnly) {
            mtpConnectedDeviceId = null
        }
    })

    // Helper to convert error type to user-friendly message
    // Note: Rust serde uses camelCase for enum variants (like "timeout" not "Timeout")
    function getMessageForType(errType: string | undefined): string | undefined {
        switch (errType) {
            case 'timeout':
                return 'Connection timed out. The device may be slow or unresponsive.'
            case 'exclusiveAccess':
                return 'Another app is using this device. Run the ptpcamerad workaround.'
            case 'deviceNotFound':
                return 'Device not found. It may have been unplugged.'
            case 'disconnected':
                return 'Device was disconnected.'
            case 'deviceBusy':
                return 'Device is busy. Please try again.'
            default:
                return undefined
        }
    }

    // Effect: Auto-connect when a device-only MTP ID is selected
    $effect(() => {
        // Log all conditions for debugging reconnection issues
        log.debug(
            'MTP auto-connect effect evaluated: isMtpDeviceOnly={isMtpDeviceOnly}, mtpConnecting={mtpConnecting}, mtpConnectionError={mtpConnectionError}, mtpConnectedDeviceId={mtpConnectedDeviceId}, volumeId={volumeId}',
            {
                isMtpDeviceOnly,
                mtpConnecting,
                mtpConnectionError,
                mtpConnectedDeviceId,
                volumeId,
            },
        )

        // Skip if we've already successfully connected to this device (waiting for volume change)
        if (isMtpDeviceOnly && !mtpConnecting && !mtpConnectionError && mtpConnectedDeviceId !== volumeId) {
            // The whole volumeId is the device ID for device-only format
            const deviceId = volumeId

            log.info('MTP auto-connect conditions met, starting connection to device: {deviceId}', { deviceId })
            mtpConnecting = true
            mtpConnectionError = null

            log.info('Auto-connecting to MTP device: {deviceId}', { deviceId })

            void connectMtpDevice(deviceId)
                .then((result) => {
                    log.info('MTP connection result: {result}', { result: JSON.stringify(result) })
                    if (result && result.storages.length > 0) {
                        // Connection successful, switch to first storage
                        const storage = result.storages[0]
                        const newVolumeId = `${deviceId}:${String(storage.id)}`
                        const newPath = constructMtpPath(deviceId, storage.id)
                        log.info(
                            'MTP connected, switching to storage: {storageId}, newVolumeId: {newVolumeId}, hasOnVolumeChange: {hasCallback}',
                            {
                                storageId: storage.id,
                                newVolumeId,
                                hasCallback: !!onVolumeChange,
                            },
                        )
                        // Mark device as connected to prevent auto-connect re-triggering
                        // while waiting for the parent to update volumeId
                        mtpConnectedDeviceId = deviceId
                        if (onVolumeChange) {
                            onVolumeChange(newVolumeId, newPath, newPath)
                            log.info('onVolumeChange called successfully')
                        } else {
                            log.warn('onVolumeChange callback not provided!')
                        }
                    } else {
                        mtpConnectionError = 'Device has no accessible storage'
                        log.warn('Device has no storages')
                    }
                })
                .catch((err: unknown) => {
                    // Handle various error formats from Tauri
                    let msg: string

                    if (err instanceof Error) {
                        msg = err.message
                    } else if (typeof err === 'string') {
                        // Error might be a JSON string - try to parse it
                        try {
                            const parsed = JSON.parse(err) as Record<string, unknown>
                            const typeMsg = getMessageForType(parsed.type as string | undefined)
                            msg = typeMsg || (parsed.userMessage as string) || (parsed.message as string) || err
                        } catch {
                            msg = err
                        }
                    } else if (typeof err === 'object' && err !== null) {
                        // Tauri MTP errors come as objects with type field
                        const errObj = err as Record<string, unknown>
                        const typeMsg = getMessageForType(errObj.type as string | undefined)
                        msg =
                            typeMsg ||
                            (errObj.userMessage as string) ||
                            (errObj.message as string) ||
                            JSON.stringify(err)
                    } else {
                        msg = String(err)
                    }
                    log.error('MTP connection failed: {error}', { error: msg })
                    mtpConnectionError = msg
                })
                .finally(() => {
                    log.info('MTP connection finally block, setting mtpConnecting=false')
                    mtpConnecting = false
                })
        }
    })

    function handleRetry() {
        log.info(
            'MTP "Try again" clicked. Clearing error to trigger auto-connect. volumeId={volumeId}, isMtpDeviceOnly={isMtpDeviceOnly}, mtpConnectedDeviceId={mtpConnectedDeviceId}',
            { volumeId, isMtpDeviceOnly, mtpConnectedDeviceId },
        )
        mtpConnectionError = null
        // Also reset mtpConnectedDeviceId to allow re-triggering auto-connect
        // even if we previously "connected" to this device
        mtpConnectedDeviceId = null
    }
</script>

{#if isMtpDeviceOnly}
    <!-- MTP device selected but not yet connected -->
    <div class="mtp-connecting">
        {#if mtpConnecting}
            <div class="connecting-spinner">
                <div class="spinner"></div>
                <span>Connecting to device...</span>
            </div>
        {:else if mtpConnectionError}
            <div class="mtp-error">
                <span class="error-icon">⚠</span>
                <span class="error-message">{mtpConnectionError}</span>
                <button type="button" class="btn" onclick={handleRetry}>Try again</button>
            </div>
        {/if}
    </div>
{/if}

<style>
    .mtp-connecting {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        flex: 1;
        gap: 12px;
        padding: 24px;
    }

    .connecting-spinner {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: 12px;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    .connecting-spinner .spinner {
        width: 24px;
        height: 24px;
        border: 2px solid var(--color-border-strong);
        border-top-color: var(--color-accent);
        border-radius: var(--radius-full);
        animation: spin 0.8s linear infinite;
    }

    @keyframes spin {
        to {
            transform: rotate(360deg);
        }
    }

    .mtp-error {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: 12px;
        text-align: center;
    }

    .mtp-error .error-icon {
        font-size: 32px;
    }

    .mtp-error .error-message {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        height: auto;
        padding: 0;
    }

    .mtp-error .btn {
        padding: 8px 16px;
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-md);
        background-color: var(--color-bg-secondary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        cursor: pointer;
        transition: background-color var(--transition-base);
    }

    .mtp-error .btn:hover {
        background-color: var(--color-bg-tertiary);
    }
</style>
