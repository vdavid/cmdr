<script lang="ts">
    /**
     * NetworkBrowser - displays discovered network hosts in a list view.
     * Rendered when user selects "Network" in the volume selector.
     * Uses the shared network-store for host data (initialized at app startup).
     */
    import { getNetworkHosts, getDiscoveryState, isHostResolving } from '$lib/network-store.svelte'
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
                <span class="col-shares">—</span>
                <span class="col-status">—</span>
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

    .col-shares,
    .col-status {
        flex: 1;
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
</style>
