<script lang="ts">
    /**
     * Sign-in prompt shown when an in-place SMB reconnect gave up on an auth failure
     * (the saved password went stale — `needs-auth`). Reuses `NetworkLoginForm` and
     * reconnects with the entered credentials via `reconnectSmbVolumeWithCredentials`,
     * which persists the new password so future reconnects are silent. On success the
     * backend emits `smb-connection-changed { state: "direct" }`, which the reconnect
     * manager turns into the pane's reload — so this view just needs to fire the command
     * and surface any error.
     */
    import NetworkLoginForm from '../network/NetworkLoginForm.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import type { NetworkHost } from '../types'
    import { reconnectSmbVolumeWithCredentials } from '$lib/tauri-commands'
    import { getAppLogger } from '$lib/logging/logger'

    interface Props {
        volumeId: string
        /** Display name for the share/server, shown in the form title. */
        serverLabel: string
        /** Leave the sign-in view without reconnecting (drops the dead connection). */
        onCancel: () => void
    }

    const { volumeId, serverLabel, onCancel }: Props = $props()
    const log = getAppLogger('smbReconnect')

    let isConnecting = $state(false)
    let errorMessage = $state<string | undefined>(tString('fileExplorer.smbReauth.savedPasswordFailed'))

    // `NetworkLoginForm` is host-shaped; synthesize a minimal host from the volume. The
    // reauth path only needs the display name (title) and the credential fields — there's
    // no guest option (a reconnect that reached `needs-auth` requires real credentials).
    const host = $derived<NetworkHost>({
        id: volumeId,
        name: serverLabel,
        hostname: undefined,
        ipAddress: undefined,
        port: 445,
        source: 'discovered',
    })

    function handleConnect(username: string | null, password: string | null): void {
        if (username === null) return // guest isn't offered in creds_required mode
        void doReconnect(username, password ?? '')
    }

    async function doReconnect(username: string, password: string): Promise<void> {
        isConnecting = true
        errorMessage = undefined
        try {
            await reconnectSmbVolumeWithCredentials(volumeId, username, password)
            // Success flows back as a `direct` event → reconnect manager → pane reload.
        } catch (e) {
            errorMessage = tString('fileExplorer.smbReauth.passwordFailed')
            log.info('Reauth reconnect failed for {volumeId}: {error}', { volumeId, error: String(e) })
        } finally {
            isConnecting = false
        }
    }
</script>

<NetworkLoginForm
    {host}
    authMode="creds_required"
    {errorMessage}
    {isConnecting}
    onConnect={handleConnect}
    {onCancel}
/>
