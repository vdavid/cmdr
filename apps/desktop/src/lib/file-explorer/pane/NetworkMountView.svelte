<script lang="ts">
    import { tick } from 'svelte'
    import type { MountError, NetworkHost, ShareInfo } from '../types'
    import { mountNetworkShare, resolvePathVolume } from '$lib/tauri-commands'
    import { getMountTimeoutMs } from '$lib/settings/network-settings'
    import { getAppLogger } from '$lib/logging/logger'
    import type { NetworkBrowserAPI, BrowserAPI } from './types'
    import NetworkBrowser from '../network/NetworkBrowser.svelte'
    import ShareBrowser from '../network/ShareBrowser.svelte'
    import ConnectToServerDialog from '../network/ConnectToServerDialog.svelte'
    import Button from '$lib/ui/Button.svelte'

    const log = getAppLogger('fileExplorer')

    interface Props {
        paneId?: 'left' | 'right'
        isFocused?: boolean
        /** Externally controlled network host (for history navigation) */
        initialNetworkHost?: NetworkHost | null
        onVolumeChange?: (volumeId: string, volumePath: string, targetPath: string) => void
        onNetworkHostChange?: (host: NetworkHost | null) => void
    }

    const {
        paneId,
        isFocused = false,
        initialNetworkHost = null,
        onVolumeChange,
        onNetworkHostChange,
    }: Props = $props()

    // Two-way synced state: parent drives it via initialNetworkHost (history nav),
    // child drives it locally (host selection, mount success, back). Can't be $derived.
    // eslint-disable-next-line svelte/prefer-writable-derived -- bidirectional sync with local overrides
    let currentNetworkHost = $state<NetworkHost | null>(initialNetworkHost)

    // Connect-to-server dialog
    let showConnectDialog = $state(false)
    let autoMountShare = $state<string | undefined>(undefined)

    // Mounting state
    let isMounting = $state(false)
    let mountError = $state<MountError | null>(null)

    // Track last mount attempt for retry
    let lastMountAttempt = $state<{
        share: ShareInfo
        credentials: { username: string; password: string } | null
    } | null>(null)

    // Component refs for keyboard navigation
    let networkBrowserRef: NetworkBrowserAPI | undefined = $state()
    let shareBrowserRef: BrowserAPI | undefined = $state()

    // Sync when parent changes the prop (for example, history navigation)
    $effect(() => {
        currentNetworkHost = initialNetworkHost
    })

    function handleNetworkHostSelect(host: NetworkHost) {
        currentNetworkHost = host
        onNetworkHostChange?.(host)
    }

    function handleConnectToServerSuccess(host: NetworkHost, sharePath: string | null) {
        showConnectDialog = false
        currentNetworkHost = host
        onNetworkHostChange?.(host)
        if (sharePath) {
            autoMountShare = sharePath
        }
    }

    async function handleConnectDialogClose() {
        showConnectDialog = false
        await tick()
        // Restore focus to the explorer container so keyboard navigation resumes
        document.querySelector<HTMLElement>('.dual-pane-explorer')?.focus()
    }

    function handleNetworkBack() {
        currentNetworkHost = null
        mountError = null
        lastMountAttempt = null
        onNetworkHostChange?.(null)
    }

    function handleMountErrorBack() {
        mountError = null
        // Stay on the share list (currentNetworkHost remains set)
    }

    /** Resolves the server address for mounting, preferring IP but falling back to hostname for loopback. */
    /** Returns the hostname or IP to connect to (without port — port is passed separately). */
    function resolveServerAddress(networkHost: NetworkHost): string {
        const ip = networkHost.ipAddress
        const isLoopback = ip === '127.0.0.1' || ip === '::1'
        return (isLoopback ? networkHost.hostname : ip) ?? networkHost.hostname ?? networkHost.name
    }

    async function handleShareSelect(share: ShareInfo, credentials: { username: string; password: string } | null) {
        if (!currentNetworkHost) return

        // Store for retry
        lastMountAttempt = { share, credentials }

        isMounting = true
        mountError = null

        try {
            const server = resolveServerAddress(currentNetworkHost)

            // Use provided credentials if available
            const result = await mountNetworkShare(
                server,
                share.name,
                credentials?.username ?? null,
                credentials?.password ?? null,
                currentNetworkHost.port,
                getMountTimeoutMs(),
            )

            // Navigate to the mounted share
            // Clear current network host first
            currentNetworkHost = null
            lastMountAttempt = null

            // The mount path is typically /Volumes/<ShareName>
            const mountPath = result.mountPath

            // Find the actual volume for the mounted path
            // This ensures proper breadcrumb display and volume context
            // (No need to refresh volume list — the mount event triggers a volumes-changed broadcast)
            const { volume: mountedVolume } = await resolvePathVolume(mountPath)

            if (mountedVolume) {
                // Use the real volume ID and path from the system
                onVolumeChange?.(mountedVolume.id, mountedVolume.path, mountPath)
            } else {
                // Fallback: use mount path as both volume path and target
                // This can happen if the volume list hasn't refreshed yet
                onVolumeChange?.(mountPath, mountPath, mountPath)
            }
        } catch (e) {
            mountError = e as MountError
            log.error('Mount failed: {error}', { error: mountError })
        } finally {
            isMounting = false
        }
    }

    function handleMountRetry() {
        if (lastMountAttempt) {
            void handleShareSelect(lastMountAttempt.share, lastMountAttempt.credentials)
        }
    }

    export function handleKeyDown(e: KeyboardEvent) {
        if (currentNetworkHost) {
            shareBrowserRef?.handleKeyDown(e)
        } else {
            networkBrowserRef?.handleKeyDown(e)
        }
    }

    /** Move cursor to a specific index (used by MCP move_cursor tool). */
    export function setCursorIndex(index: number) {
        if (currentNetworkHost) {
            shareBrowserRef?.setCursorIndex(index)
        } else {
            networkBrowserRef?.setCursorIndex(index)
        }
    }

    /** Find an item by name, returns its index or -1. */
    export function findItemIndex(name: string): number {
        if (currentNetworkHost) {
            return shareBrowserRef?.findItemIndex(name) ?? -1
        }
        return networkBrowserRef?.findItemIndex(name) ?? -1
    }

    /** Refresh network hosts (used by ⌘R shortcut). */
    export function refreshNetworkHosts() {
        networkBrowserRef?.refresh()
    }

    export function setNetworkHost(host: NetworkHost | null) {
        currentNetworkHost = host
        mountError = null
        lastMountAttempt = null
    }
</script>

{#if isMounting}
    <div class="mounting-state">
        <span class="spinner spinner-md"></span>
        <span class="mounting-text">Mounting {currentNetworkHost?.name ?? 'share'}...</span>
    </div>
{:else if mountError}
    <div class="mount-error-state">
        <div class="error-icon">&#x274C;</div>
        <div class="error-title">Couldn't mount share</div>
        <div class="error-message">{mountError.message}</div>
        <div class="error-actions">
            <Button variant="secondary" onclick={handleMountRetry}>Try again</Button>
            <Button variant="secondary" onclick={handleMountErrorBack}>Back</Button>
        </div>
    </div>
{:else if currentNetworkHost}
    <ShareBrowser
        bind:this={shareBrowserRef}
        host={currentNetworkHost}
        {paneId}
        {isFocused}
        {autoMountShare}
        onShareSelect={handleShareSelect}
        onBack={handleNetworkBack}
    />
{:else}
    <NetworkBrowser
        bind:this={networkBrowserRef}
        {paneId}
        {isFocused}
        onHostSelect={handleNetworkHostSelect}
        onConnectToServer={() => (showConnectDialog = true)}
    />
{/if}

{#if showConnectDialog}
    <ConnectToServerDialog
        onConnect={handleConnectToServerSuccess}
        onClose={handleConnectDialogClose}
    />
{/if}

<style>
    .mounting-state {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        height: 100%;
        gap: var(--spacing-md);
        color: var(--color-text-secondary);
    }

    .mounting-text {
        font-size: var(--font-size-sm);
    }

    .mount-error-state {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        height: 100%;
        padding: var(--spacing-xl);
        gap: var(--spacing-md);
        color: var(--color-text-secondary);
    }

    .mount-error-state .error-icon {
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- emoji icon, outside type scale */
        font-size: 32px;
    }

    .mount-error-state .error-title {
        font-size: var(--font-size-lg);
        font-weight: 500;
        color: var(--color-text-primary);
    }

    .mount-error-state .error-message {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        text-align: center;
        height: auto;
        padding: 0;
    }

    .mount-error-state .error-actions {
        display: flex;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-sm);
    }
</style>
