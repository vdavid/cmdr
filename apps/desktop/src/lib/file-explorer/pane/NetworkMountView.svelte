<script lang="ts">
    import type { MountError, NetworkHost, ShareInfo } from '../types'
    import { mountNetworkShare, listVolumes, findContainingVolume } from '$lib/tauri-commands'
    import { getMountTimeoutMs } from '$lib/settings/network-settings'
    import { getAppLogger } from '$lib/logger'
    import NetworkBrowser from '../network/NetworkBrowser.svelte'
    import ShareBrowser from '../network/ShareBrowser.svelte'

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

    // Mounting state
    let isMounting = $state(false)
    let mountError = $state<MountError | null>(null)

    // Track last mount attempt for retry
    let lastMountAttempt = $state<{
        share: ShareInfo
        credentials: { username: string; password: string } | null
    } | null>(null)

    // Component refs for keyboard navigation
    let networkBrowserRef: NetworkBrowser | undefined = $state()
    let shareBrowserRef: ShareBrowser | undefined = $state()

    // Sync when parent changes the prop (for example, history navigation)
    $effect(() => {
        currentNetworkHost = initialNetworkHost
    })

    function handleNetworkHostSelect(host: NetworkHost) {
        currentNetworkHost = host
        onNetworkHostChange?.(host)
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

    async function handleShareSelect(share: ShareInfo, credentials: { username: string; password: string } | null) {
        if (!currentNetworkHost) return

        // Store for retry
        lastMountAttempt = { share, credentials }

        isMounting = true
        mountError = null

        try {
            // Get server address - prefer IP, fall back to hostname
            const server = currentNetworkHost.ipAddress ?? currentNetworkHost.hostname ?? currentNetworkHost.name

            // Use provided credentials if available
            const result = await mountNetworkShare(
                server,
                share.name,
                credentials?.username ?? null,
                credentials?.password ?? null,
                getMountTimeoutMs(),
            )

            // Navigate to the mounted share
            // Clear current network host first
            currentNetworkHost = null
            lastMountAttempt = null

            // The mount path is typically /Volumes/<ShareName>
            const mountPath = result.mountPath

            // Refresh the volume list first - the new mount needs to be recognized
            await listVolumes()

            // Find the actual volume for the mounted path
            // This ensures proper breadcrumb display and volume context
            const mountedVolume = await findContainingVolume(mountPath)

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
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            shareBrowserRef?.handleKeyDown(e)
        } else {
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call
            networkBrowserRef?.handleKeyDown(e)
        }
    }

    /** Move cursor to a specific index (used by MCP move_cursor tool). */
    export function setCursorIndex(index: number) {
        if (currentNetworkHost) {
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call -- Svelte bind:this ref
            shareBrowserRef?.setCursorIndex(index)
        } else {
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call -- Svelte bind:this ref
            networkBrowserRef?.setCursorIndex(index)
        }
    }

    /** Find an item by name, returns its index or -1. */
    export function findItemIndex(name: string): number {
        if (currentNetworkHost) {
            // eslint-disable-next-line @typescript-eslint/no-unsafe-call -- Svelte bind:this ref
            return (shareBrowserRef?.findItemIndex(name) as number | undefined) ?? -1
        }
        // eslint-disable-next-line @typescript-eslint/no-unsafe-call -- Svelte bind:this ref
        return (networkBrowserRef?.findItemIndex(name) as number | undefined) ?? -1
    }

    export function setNetworkHost(host: NetworkHost | null) {
        currentNetworkHost = host
        mountError = null
        lastMountAttempt = null
    }
</script>

{#if isMounting}
    <div class="mounting-state">
        <span class="spinner"></span>
        <span class="mounting-text">Mounting {currentNetworkHost?.name ?? 'share'}...</span>
    </div>
{:else if mountError}
    <div class="mount-error-state">
        <div class="error-icon">&#x274C;</div>
        <div class="error-title">Couldn't mount share</div>
        <div class="error-message">{mountError.message}</div>
        <div class="error-actions">
            <button type="button" class="btn" onclick={handleMountRetry}>Try again</button>
            <button type="button" class="btn" onclick={handleMountErrorBack}>Back</button>
        </div>
    </div>
{:else if currentNetworkHost}
    <ShareBrowser
        bind:this={shareBrowserRef}
        host={currentNetworkHost}
        {paneId}
        {isFocused}
        onShareSelect={handleShareSelect}
        onBack={handleNetworkBack}
    />
{:else}
    <NetworkBrowser bind:this={networkBrowserRef} {paneId} {isFocused} onHostSelect={handleNetworkHostSelect} />
{/if}

<style>
    .mounting-state {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        height: 100%;
        gap: 12px;
        color: var(--color-text-secondary);
    }

    .mounting-state .spinner {
        width: 24px;
        height: 24px;
        border: 3px solid var(--color-border-strong);
        border-top-color: var(--color-accent);
        border-radius: var(--radius-full);
        animation: spin 1s linear infinite;
    }

    @keyframes spin {
        to {
            transform: rotate(360deg);
        }
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
        padding: 24px;
        gap: 12px;
        color: var(--color-text-secondary);
    }

    .mount-error-state .error-icon {
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
        gap: 8px;
        margin-top: 8px;
    }

    .mount-error-state .btn {
        padding: 8px 16px;
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-md);
        background-color: var(--color-bg-secondary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        cursor: pointer;
        transition: background-color var(--transition-base);
    }

    .mount-error-state .btn:hover {
        background-color: var(--color-bg-tertiary);
    }
</style>
