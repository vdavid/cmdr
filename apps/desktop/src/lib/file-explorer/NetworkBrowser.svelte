<script lang="ts">
    /**
     * NetworkBrowser - displays discovered network hosts in a list view.
     * Rendered when user selects "Network" in the volume selector.
     * Uses the shared network-store for host data (initialized at app startup).
     */
    import { onMount } from 'svelte'
    import {
        getNetworkHosts,
        getDiscoveryState,
        isHostResolving,
        getShareState,
        getShareCount,
        isListingShares,
        isShareDataStale,
        refreshAllStaleShares,
        clearShareState,
        fetchShares,
        getCredentialStatus,
        checkCredentialsForHost,
    } from '$lib/network-store.svelte'
    import type { NetworkHost } from './types'
    import { updateLeftPaneState, updateRightPaneState, type PaneState, type PaneFileEntry } from '$lib/tauri-commands'
    import { handleNavigationShortcut } from './keyboard-shortcuts'

    /** Row height for host list (matches Full list) */
    const HOST_ROW_HEIGHT = 20

    interface Props {
        paneId?: 'left' | 'right'
        isFocused?: boolean
        onHostSelect?: (host: NetworkHost) => void
    }

    const { paneId, isFocused = false, onHostSelect }: Props = $props()

    // Get reactive state from the network store
    const hosts = $derived(getNetworkHosts())
    const discoveryState = $derived(getDiscoveryState())
    const isSearching = $derived(discoveryState === 'searching')

    // Local cursor state
    let cursorIndex = $state(0)

    // Container tracking for PageUp/PageDown
    let listContainer: HTMLDivElement | undefined = $state()
    let containerHeight = $state(0)

    // Refresh stale shares when component mounts (entering network view)
    onMount(() => {
        refreshAllStaleShares()
        // Check credentials for all hosts that need auth
        for (const host of hosts) {
            const state = getShareState(host.id)
            if (
                state?.status === 'error' &&
                (state.error.type === 'auth_required' || state.error.type === 'signing_required')
            ) {
                void checkCredentialsForHost(host.name)
            }
        }
        // Sync to MCP
        void syncPaneStateToMcp()
    })

    /**
     * Sync network hosts to MCP for context tools.
     * Represents hosts as "files" for simplicity.
     */
    async function syncPaneStateToMcp() {
        if (!paneId) return

        try {
            // Represent hosts as file entries (simple approach)
            const files: PaneFileEntry[] = hosts.map((host) => ({
                name: host.name,
                path: `network://${host.name}`,
                isDirectory: true, // Hosts act like directories (contain shares)
            }))

            const state: PaneState = {
                path: 'network://',
                volumeId: 'network',
                files,
                cursorIndex,
                viewMode: 'full',
                selectedIndices: [], // Network view doesn't support selection
            }

            if (paneId === 'left') {
                await updateLeftPaneState(state)
            } else {
                await updateRightPaneState(state)
            }
        } catch {
            // Silently ignore sync errors
        }
    }

    /** Scrolls to make the cursor visible */
    function scrollToIndex(index: number) {
        if (!listContainer) return
        const targetTop = index * HOST_ROW_HEIGHT
        const targetBottom = targetTop + HOST_ROW_HEIGHT
        const scrollTop = listContainer.scrollTop
        const viewportBottom = scrollTop + containerHeight

        if (targetTop < scrollTop) {
            listContainer.scrollTop = targetTop
        } else if (targetBottom > viewportBottom) {
            listContainer.scrollTop = targetBottom - containerHeight
        }
    }

    // Handle keyboard navigation
    export function handleKeyDown(e: KeyboardEvent): boolean {
        if (hosts.length === 0) return false

        // Try centralized navigation shortcuts first (PageUp, PageDown, Home, End, Option+arrows)
        const visibleItems = Math.max(1, Math.floor(containerHeight / HOST_ROW_HEIGHT))
        const navResult = handleNavigationShortcut(e, {
            currentIndex: cursorIndex,
            totalCount: hosts.length,
            visibleItems,
        })
        if (navResult?.handled) {
            e.preventDefault()
            cursorIndex = navResult.newIndex
            scrollToIndex(cursorIndex)
            return true
        }

        switch (e.key) {
            case 'ArrowDown':
                e.preventDefault()
                cursorIndex = Math.min(cursorIndex + 1, hosts.length - 1)
                scrollToIndex(cursorIndex)
                return true
            case 'ArrowUp':
                e.preventDefault()
                cursorIndex = Math.max(cursorIndex - 1, 0)
                scrollToIndex(cursorIndex)
                return true
            case 'ArrowLeft':
                e.preventDefault()
                cursorIndex = 0
                scrollToIndex(cursorIndex)
                return true
            case 'ArrowRight':
                e.preventDefault()
                cursorIndex = hosts.length - 1
                scrollToIndex(cursorIndex)
                return true
            case 'Enter':
                e.preventDefault()
                if (cursorIndex >= 0 && cursorIndex < hosts.length) {
                    onHostSelect?.(hosts[cursorIndex])
                }
                return true
        }
        return false
    }

    // Handle host clicks
    function handleHostClick(index: number) {
        cursorIndex = index
    }

    function handleHostDoubleClick(index: number) {
        if (index >= 0 && index < hosts.length) {
            onHostSelect?.(hosts[index])
        }
    }

    // Helper to get display text for IP/hostname column
    function getIpDisplay(host: NetworkHost): string {
        if (host.ipAddress) return host.ipAddress
        if (isHostResolving(host.id)) return 'fetching...'
        return '—'
    }

    function getHostnameDisplay(host: NetworkHost): string {
        if (host.hostname) return host.hostname
        if (isHostResolving(host.id)) return 'fetching...'
        return '—'
    }

    // Helper to get share count display - shows "{N}?" when stale, "(unknown)" when no data
    function getSharesDisplay(host: NetworkHost): string {
        const isStale = isShareDataStale(host.id)
        const count = getShareCount(host.id)
        if (count !== undefined) {
            return isStale ? `${String(count)}?` : String(count)
        }
        if (isListingShares(host.id)) return '...'
        return '(unknown)'
    }

    // Check if share data needs refresh indicator
    function needsRefreshIndicator(host: NetworkHost): boolean {
        return isShareDataStale(host.id) && getShareCount(host.id) !== undefined
    }

    // Helper to get error status display with icon
    function getErrorStatusDisplay(errorType: string, hostName: string, infoIcon: string): string {
        // Auth required - check if we have stored credentials
        if (errorType === 'auth_required' || errorType === 'signing_required') {
            const credStatus = getCredentialStatus(hostName)
            if (credStatus === 'has_creds') return `🔑 Logged in${infoIcon}`
            if (credStatus === 'failed') return `⚠️ Login failed${infoIcon}`
            return `🔒 Login needed${infoIcon}`
        }
        if (errorType === 'auth_failed') return `⚠️ Login failed${infoIcon}`
        if (errorType === 'timeout') return `⏱️ Timeout${infoIcon}`
        if (errorType === 'host_unreachable') return `❌ Unreachable${infoIcon}`
        return `⚠️ Error${infoIcon}`
    }

    // Helper to get status display - shows credential-aware status
    function getStatusDisplay(host: NetworkHost): string {
        const state = getShareState(host.id)

        // No state yet - show helpful status
        if (!state) {
            if (isHostResolving(host.id)) return 'Resolving...'
            if (!host.hostname) return 'Waiting for network...'
            return 'Not checked'
        }

        if (state.status === 'loading') return 'Connecting...'

        if (state.status === 'error') {
            const hasTooltip = !!getStatusTooltip(host)
            const infoIcon = hasTooltip ? ' ℹ️' : ''
            return getErrorStatusDisplay(state.error.type, host.name, infoIcon)
        }

        // status === 'loaded'
        const stale = isShareDataStale(host.id)
        const credStatus = getCredentialStatus(host.name)

        // If we have credentials stored, show "Logged in" regardless of auth mode
        if (credStatus === 'has_creds') {
            return stale ? '✓ Logged in 🔄' : '✓ Logged in'
        }

        // Guest access (no stored credentials)
        if (state.result.authMode === 'guest_allowed') {
            return stale ? '✓ Guest 🔄' : '✓ Guest'
        }
        return stale ? '✓ Connected 🔄' : '✓ Connected'
    }

    // Helper to check if status should be styled as an error
    function isStatusError(host: NetworkHost): boolean {
        const state = getShareState(host.id)
        if (!state || state.status !== 'error') return false

        // Auth required with no credentials is NOT an error, just needs action
        if (state.error.type === 'auth_required' || state.error.type === 'signing_required') {
            const credStatus = getCredentialStatus(host.name)
            // Only show as error if login actually failed
            return credStatus === 'failed'
        }

        // Other errors (timeout, unreachable, auth_failed) are real errors
        return true
    }

    // Helper to get error tooltip text with nuanced explanations
    function getStatusTooltip(host: NetworkHost): string | undefined {
        const state = getShareState(host.id)

        // No state - explain what's happening
        if (!state) {
            if (isHostResolving(host.id)) return 'Resolving hostname and IP address...'
            if (!host.hostname) return 'Waiting for network name resolution'
            return 'Double-click to connect and view shares'
        }

        if (state.status === 'error') {
            // Auth required with credentials context
            if (state.error.type === 'auth_required' || state.error.type === 'signing_required') {
                const credStatus = getCredentialStatus(host.name)
                if (credStatus === 'has_creds') {
                    return 'Credentials stored. Double-click to connect.'
                }
                if (credStatus === 'failed') {
                    return 'Stored credentials were rejected. Please log in with updated credentials.'
                }
                return 'This host requires login. Double-click to enter credentials.'
            }
            if (state.error.type === 'auth_failed') {
                return 'Authentication failed. Check your credentials and try again.'
            }
            return state.error.message || `Error: ${state.error.type}`
        }
        return undefined
    }

    // Refresh all shares (user-initiated)
    function handleRefreshClick() {
        // Clear all share states to force refetch
        for (const host of hosts) {
            clearShareState(host.id)
            if (host.hostname) {
                fetchShares(host).catch(() => {
                    // Errors are stored in shareStates, ignore here
                })
            }
        }
    }
</script>

<div class="network-browser" class:is-focused={isFocused}>
    <div class="header-row">
        <span class="col-name">Name</span>
        <span class="col-ip">IP address</span>
        <span class="col-hostname">Hostname</span>
        <span class="col-shares">Shares</span>
        <span class="col-status">Status</span>
    </div>
    <div class="host-list" bind:this={listContainer} bind:clientHeight={containerHeight}>
        {#each hosts as host, index (host.id)}
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
            <div
                class="host-row"
                class:is-under-cursor={index === cursorIndex}
                class:is-focused-and-under-cursor={isFocused && index === cursorIndex}
                role="listitem"
                onclick={() => {
                    handleHostClick(index)
                }}
                ondblclick={() => {
                    handleHostDoubleClick(index)
                }}
                onkeydown={() => {}}
            >
                <span class="col-name">
                    <span class="host-icon">🖥️</span>
                    {host.name}
                </span>
                <span class="col-ip" class:is-fetching={isHostResolving(host.id) && !host.ipAddress}
                    >{getIpDisplay(host)}</span
                >
                <span class="col-hostname" class:is-fetching={isHostResolving(host.id) && !host.hostname}
                    >{getHostnameDisplay(host)}</span
                >
                <span
                    class="col-shares"
                    class:is-fetching={isListingShares(host.id)}
                    class:is-stale={needsRefreshIndicator(host)}>{getSharesDisplay(host)}</span
                >
                <span
                    class="col-status"
                    class:is-error={isStatusError(host)}
                    class:needs-login={!isStatusError(host) && getShareState(host.id)?.status === 'error'}
                    title={getStatusTooltip(host)}>{getStatusDisplay(host)}</span
                >
            </div>
        {/each}

        {#if isSearching}
            <div class="searching-indicator">
                <span class="searching-spinner"></span>
                Searching...
            </div>
        {:else if hosts.length === 0}
            <div class="empty-state">No network hosts found.</div>
        {/if}
    </div>

    <div class="refresh-section">
        <button type="button" class="refresh-button" onclick={handleRefreshClick}> 🔄 Refresh </button>
    </div>
</div>

<style>
    .network-browser {
        display: flex;
        flex-direction: column;
        height: 100%;
        font-size: var(--font-size-sm);
        font-family: var(--font-system), sans-serif;
    }

    .header-row {
        display: flex;
        padding: 4px 8px;
        background-color: var(--color-bg-secondary);
        border-bottom: 1px solid var(--color-border-primary);
        font-weight: 500;
        color: var(--color-text-secondary);
    }

    .host-list {
        flex: 1;
        overflow-y: auto;
    }

    .host-row {
        display: flex;
        height: 20px;
        padding: var(--spacing-xxs) var(--spacing-sm);
        cursor: default;
    }

    .host-row.is-under-cursor {
        background-color: var(--color-cursor-unfocused-bg);
    }

    .host-row.is-focused-and-under-cursor {
        background-color: var(--color-cursor-focused-bg);
        color: var(--color-cursor-focused-fg);
    }

    .col-name {
        flex: 2;
        display: flex;
        align-items: center;
        gap: 6px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .col-ip,
    .col-hostname {
        flex: 1.5;
        color: var(--color-text-secondary);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .col-ip.is-fetching,
    .col-hostname.is-fetching {
        font-style: italic;
        color: var(--color-text-muted);
    }

    .col-shares {
        flex: 1;
        color: var(--color-text-tertiary);
        text-align: center;
    }

    .col-status {
        flex: 2.5;
        color: var(--color-text-tertiary);
        text-align: center;
    }

    .host-icon {
        font-size: 14px;
    }

    .searching-indicator {
        display: flex;
        align-items: center;
        gap: 8px;
        padding: 12px 16px;
        color: var(--color-text-tertiary);
        font-style: italic;
    }

    .searching-spinner {
        width: 12px;
        height: 12px;
        border: 2px solid var(--color-border-primary);
        border-top-color: var(--color-accent);
        border-radius: 50%;
        animation: spin 1s linear infinite;
    }

    @keyframes spin {
        to {
            transform: rotate(360deg);
        }
    }

    .empty-state {
        display: flex;
        align-items: center;
        justify-content: center;
        padding: 48px 16px;
        color: var(--color-text-tertiary);
        font-style: italic;
    }

    .col-shares.is-fetching {
        font-style: italic;
        color: var(--color-text-muted);
    }

    .col-shares.is-stale {
        color: var(--color-text-muted);
    }

    .col-status.is-error {
        color: var(--color-error);
        cursor: help;
    }

    .col-status.needs-login {
        color: var(--color-warning);
        cursor: help;
    }

    .refresh-section {
        display: flex;
        justify-content: center;
        padding: 16px 8px;
        border-top: 1px solid var(--color-border-secondary);
    }

    .refresh-button {
        display: flex;
        align-items: center;
        gap: 6px;
        padding: 8px 16px;
        border: 1px solid var(--color-border-primary);
        border-radius: 6px;
        background-color: var(--color-bg-secondary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        cursor: pointer;
        transition: background-color 0.15s ease;
    }

    .refresh-button:hover {
        background-color: var(--color-button-hover);
    }

    .refresh-button:active {
        background-color: var(--color-bg-tertiary);
    }
</style>
