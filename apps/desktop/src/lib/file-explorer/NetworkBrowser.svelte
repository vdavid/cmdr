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

    interface Props {
        isFocused?: boolean
        onHostSelect?: (host: NetworkHost) => void
    }

    const { isFocused = false, onHostSelect }: Props = $props()

    // Get reactive state from the network store
    const hosts = $derived(getNetworkHosts())
    const discoveryState = $derived(getDiscoveryState())
    const isSearching = $derived(discoveryState === 'searching')

    // Local selection state
    let selectedIndex = $state(0)

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
    })

    // Handle keyboard navigation
    export function handleKeyDown(e: KeyboardEvent): boolean {
        if (hosts.length === 0) return false

        switch (e.key) {
            case 'ArrowDown':
                e.preventDefault()
                selectedIndex = Math.min(selectedIndex + 1, hosts.length - 1)
                return true
            case 'ArrowUp':
                e.preventDefault()
                selectedIndex = Math.max(selectedIndex - 1, 0)
                return true
            case 'Home':
                e.preventDefault()
                selectedIndex = 0
                return true
            case 'End':
                e.preventDefault()
                selectedIndex = hosts.length - 1
                return true
            case 'Enter':
                e.preventDefault()
                if (selectedIndex >= 0 && selectedIndex < hosts.length) {
                    onHostSelect?.(hosts[selectedIndex])
                }
                return true
        }
        return false
    }

    // Handle host selection via click
    function handleHostClick(index: number) {
        selectedIndex = index
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
    <div class="host-list">
        {#each hosts as host, index (host.id)}
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
            <div
                class="host-row"
                class:is-selected={index === selectedIndex}
                class:is-highlighted={isFocused && index === selectedIndex}
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
        padding: 4px 8px;
        cursor: default;
        border-bottom: 1px solid var(--color-border-secondary);
    }

    .host-row:hover {
        background-color: var(--color-bg-hover);
    }

    .host-row.is-selected {
        background-color: var(--color-bg-selected-unfocused);
    }

    .host-row.is-highlighted {
        background-color: var(--color-bg-selected);
        color: var(--color-text-selected);
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
        color: var(--color-warning, #f5a623);
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
        background-color: var(--color-bg-hover);
    }

    .refresh-button:active {
        background-color: var(--color-bg-tertiary);
    }
</style>
