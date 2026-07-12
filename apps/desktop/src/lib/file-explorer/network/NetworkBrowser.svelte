<script lang="ts">
    /**
     * NetworkBrowser - displays discovered network hosts in a list view.
     * Rendered when user selects "Network" in the volume selector.
     * Uses the shared network-store for host data (initialized at app startup).
     */
    import { onMount, onDestroy } from 'svelte'
    import Button from '$lib/ui/Button.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
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
        forgetCredentials,
    } from './network-store.svelte'
    import {
        getHostStatus,
        getStatusTooltip,
        isStatusError,
        STATUS_ICON,
        STATUS_MCP_LABEL,
        STATUS_TEXT_KEY,
    } from './host-status'
    import { tooltip } from '$lib/tooltip/tooltip'
    import type { NetworkHost } from '../types'
    import {
        updateLeftPaneState,
        updateRightPaneState,
        removeManualServer,
        showNetworkHostContextMenu,
        onNetworkHostContextAction,
        disconnectNetworkHost,
        type PaneState,
        type PaneFileEntry,
    } from '$lib/tauri-commands'
    import { handleNavigationShortcut } from '../navigation/keyboard-shortcuts'
    import { confirmDialog } from '$lib/utils/confirm-dialog'
    import { addToast } from '$lib/ui/toast'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import { triggerNetworkDiscovery } from './lazy-trigger'
    import { tString } from '$lib/intl/messages.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import { formatInteger } from '$lib/intl/number-format'

    /** Row height for host list (matches Full list) */
    const HOST_ROW_HEIGHT = 20

    interface Props {
        paneId?: 'left' | 'right'
        isFocused?: boolean
        onHostSelect?: (host: NetworkHost) => void
        onConnectToServer?: () => void
    }

    const { paneId, isFocused = false, onHostSelect, onConnectToServer }: Props = $props()

    // Get reactive state from the network store
    const hosts = $derived(getNetworkHosts())
    const discoveryState = $derived(getDiscoveryState())
    const isSearching = $derived(discoveryState === 'searching')

    // Local cursor state
    let cursorIndex = $state(0)

    // Container tracking for PageUp/PageDown
    let listContainer: HTMLDivElement | undefined = $state()
    let containerHeight = $state(0)

    // Event listener cleanup for network host context menu
    let unlistenContextAction: (() => void) | undefined

    // Refresh stale shares when component mounts (entering network view)
    onMount(() => {
        // Lazy-start mDNS the first time the user enters Network. Triggers the macOS
        // Local Network prompt on first call after a fresh install. No-op if discovery
        // is already running or networking is disabled.
        triggerNetworkDiscovery()

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

        // Listen for network host context menu actions
        void onNetworkHostContextAction((payload) => {
            void handleContextAction(payload)
        }).then((fn) => {
            unlistenContextAction = fn
        })
    })

    onDestroy(() => {
        unlistenContextAction?.()
    })

    // Re-sync MCP state when hosts or cursor change
    $effect(() => {
        // Touch reactive deps
        void hosts.length
        void cursorIndex
        void syncPaneStateToMcp()
    })

    // Clamp cursor when hosts change (e.g. a host is removed)
    $effect(() => {
        const maxIndex = totalNavigableItems - 1
        if (cursorIndex > maxIndex) {
            cursorIndex = Math.max(0, maxIndex)
        }
    })

    /**
     * Sync network hosts to MCP for context tools.
     * Encodes host details (IP, hostname, shares, status) into file entry names
     * so MCP agents can see the same info as the UI.
     */
    async function syncPaneStateToMcp() {
        if (!paneId) return

        try {
            const files: PaneFileEntry[] = hosts.map((host) => {
                const ip = getIpDisplay(host)
                const hostname = getHostnameDisplay(host)
                const shares = getSharesDisplay(host)
                // Stable, locale-independent status token (not the localized UI label).
                const status = STATUS_MCP_LABEL[getHostStatus(host).kind]
                return {
                    name: `${host.name}  ip=${ip}  hostname=${hostname}  source=${host.source ?? 'discovered'}  shares=${shares}  status="${status}"`,
                    path: `smb://${host.ipAddress ?? host.name}`,
                    isDirectory: true,
                    size: null,
                    recursiveSize: null,
                    modified: null,
                    recursiveSizePending: null,                }
            })

            // Add the "Connect to server..." pseudo-row for MCP visibility
            files.push({
                name: '+ Connect to server...',
                path: 'smb://connect',
                isDirectory: false,
                size: null,
                recursiveSize: null,
                modified: null,
                recursiveSizePending: null,            })

            const state: PaneState = {
                path: 'smb://',
                volumeId: 'network',
                volumeName: 'Network',
                files,
                cursorIndex,
                viewMode: 'full',
                selectedIndices: [],
                totalFiles: hosts.length,
                loadedStart: 0,
                loadedEnd: hosts.length,
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

    /** Move cursor to a specific index. */
    /** Total navigable items: hosts + the "Connect to server..." pseudo-row. */
    const totalNavigableItems = $derived(hosts.length + 1)

    // noinspection JSUnusedGlobalSymbols -- used dynamically by MCP move_cursor tool
    export function setCursorIndex(index: number) {
        cursorIndex = Math.max(0, Math.min(index, totalNavigableItems - 1))
        scrollToIndex(cursorIndex)
    }

    /** Refresh all shares (used by ⌘R shortcut). */
    export function refresh() {
        handleRefreshClick()
    }

    /** Find a host by name, returns its index or -1. */
    // noinspection JSUnusedGlobalSymbols -- used dynamically
    export function findItemIndex(name: string): number {
        return hosts.findIndex((h) => h.name.toLowerCase() === name.toLowerCase())
    }

    /**
     * Returns the host under the cursor, or `null` when the cursor sits on the
     * "Connect to server…" pseudo-row or the list is empty. Consumed by the
     * "Copy path between panes" command so cursor-on-server mirrors that server.
     */
    // noinspection JSUnusedGlobalSymbols -- used dynamically by NetworkMountView
    export function getHostUnderCursor(): NetworkHost | null {
        if (isCursorOnConnectRow) return null
        if (cursorIndex < 0 || cursorIndex >= hosts.length) return null
        return hosts[cursorIndex]
    }

    /** Opens the host (or "Connect to server…" row) under the cursor — same action Enter triggers. */
    // noinspection JSUnusedGlobalSymbols -- used dynamically by NetworkMountView / MCP
    export function openCursorItem(): void {
        if (isCursorOnConnectRow) {
            onConnectToServer?.()
        } else if (cursorIndex >= 0 && cursorIndex < hosts.length) {
            onHostSelect?.(hosts[cursorIndex])
        }
    }

    /** Check for ⌘R refresh shortcut */
    function isRefreshShortcut(e: KeyboardEvent): boolean {
        return e.key === 'r' && e.metaKey && !e.shiftKey && !e.altKey && !e.ctrlKey
    }

    /** Whether the cursor is on the "Connect to server..." pseudo-row. */
    const isCursorOnConnectRow = $derived(cursorIndex === hosts.length)

    /** Handle arrow keys and Enter for host list navigation. */
    function handleArrowAndEnter(key: string): boolean {
        switch (key) {
            case 'ArrowDown':
                cursorIndex = Math.min(cursorIndex + 1, totalNavigableItems - 1)
                scrollToIndex(cursorIndex)
                return true
            case 'ArrowUp':
                cursorIndex = Math.max(cursorIndex - 1, 0)
                scrollToIndex(cursorIndex)
                return true
            case 'ArrowLeft':
                cursorIndex = 0
                scrollToIndex(cursorIndex)
                return true
            case 'ArrowRight':
                cursorIndex = totalNavigableItems - 1
                scrollToIndex(cursorIndex)
                return true
            case 'Enter':
                if (isCursorOnConnectRow) {
                    onConnectToServer?.()
                } else if (cursorIndex >= 0 && cursorIndex < hosts.length) {
                    onHostSelect?.(hosts[cursorIndex])
                }
                return true
            default:
                return false
        }
    }

    // Handle keyboard navigation
    // noinspection JSUnusedGlobalSymbols -- used dynamically
    export function handleKeyDown(e: KeyboardEvent): boolean {
        // ⌘R to refresh, works regardless of host count
        if (isRefreshShortcut(e)) {
            e.preventDefault()
            handleRefreshClick()
            return true
        }

        // The connect row is always present, so totalNavigableItems >= 1
        if (totalNavigableItems === 0) return false

        // Try centralized navigation shortcuts first (PageUp, PageDown, Home, End, Option+arrows)
        const visibleItems = Math.max(1, Math.floor(containerHeight / HOST_ROW_HEIGHT))
        const navResult = handleNavigationShortcut(e, {
            currentIndex: cursorIndex,
            totalCount: totalNavigableItems,
            visibleItems,
        })
        if (navResult?.handled) {
            e.preventDefault()
            cursorIndex = navResult.newIndex
            scrollToIndex(cursorIndex)
            return true
        }

        // F8: remove manual host
        if (e.key === 'F8' && !isCursorOnConnectRow && cursorIndex < hosts.length) {
            e.preventDefault()
            const host = hosts[cursorIndex]
            void handleRemoveHost(host)
            return true
        }

        // ⌘← / ⌘→ are reserved for "Copy path between panes" (document-level
        // dispatch), so let them bubble instead of jumping the cursor.
        if (e.metaKey && (e.key === 'ArrowLeft' || e.key === 'ArrowRight')) {
            return false
        }
        if (handleArrowAndEnter(e.key)) {
            e.preventDefault()
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

    function handleConnectRowClick() {
        cursorIndex = hosts.length
    }

    function handleConnectRowDoubleClick() {
        onConnectToServer?.()
    }

    // Helper to get display text for IP/hostname column
    function getIpDisplay(host: NetworkHost): string {
        if (host.ipAddress) return host.ipAddress
        if (isHostResolving(host.id)) return tString('fileExplorer.network.browser.fetching')
        return '—'
    }

    function getHostnameDisplay(host: NetworkHost): string {
        if (host.hostname) return host.hostname
        if (isHostResolving(host.id)) return tString('fileExplorer.network.browser.fetching')
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
        return tString('fileExplorer.network.browser.unknown')
    }

    // Check if share data needs refresh indicator
    function needsRefreshIndicator(host: NetworkHost): boolean {
        return isShareDataStale(host.id) && getShareCount(host.id) !== undefined
    }

    /** Remove a manual host after confirmation. For discovered hosts, show a toast. */
    async function handleRemoveHost(host: NetworkHost) {
        if (host.source !== 'manual') {
            addToast(tString('fileExplorer.network.browser.cannotRemoveDiscovered'), { level: 'warn' })
            return
        }

        const confirmed = await confirmDialog(
            tString('fileExplorer.network.browser.removeHostConfirm', { hostName: host.name }),
            tString('fileExplorer.network.browser.removeHostConfirmButton'),
        )
        if (!confirmed) return

        try {
            await removeManualServer(host.id)
            addToast(tString('fileExplorer.network.browser.hostRemoved', { hostName: host.name }), { level: 'success' })
        } catch {
            addToast(tString('fileExplorer.network.browser.hostRemoveFailed', { hostName: host.name }), {
                level: 'error',
            })
        }
    }

    /** Show native context menu for a network host. */
    async function handleHostContextMenu(e: MouseEvent, host: NetworkHost) {
        e.preventDefault()

        const isManual = host.source === 'manual'

        // Ensure we have current credential status (may need Keychain lookup)
        if (getCredentialStatus(host.name) === 'unknown') {
            await checkCredentialsForHost(host.name)
        }

        const hasCredentials = getCredentialStatus(host.name) === 'has_creds'

        void showNetworkHostContextMenu(host.id, host.name, isManual, hasCredentials)
    }

    /** Handle actions dispatched from the native network host context menu. */
    async function handleContextAction(payload: { action: string; hostId: string; hostName: string }) {
        switch (payload.action) {
            case 'forget-server': {
                const host = hosts.find((h: NetworkHost) => h.id === payload.hostId)
                if (host) void handleRemoveHost(host)
                break
            }
            case 'forget-password': {
                try {
                    await forgetCredentials(payload.hostName)
                    addToast(tString('fileExplorer.network.forgotPassword', { hostName: payload.hostName }), {
                        level: 'success',
                    })
                } catch {
                    addToast(tString('fileExplorer.network.deletePasswordFailed'), { level: 'error' })
                }
                break
            }
            case 'disconnect': {
                const host = hosts.find((h: NetworkHost) => h.id === payload.hostId)
                if (!host) break
                try {
                    const unmounted = await disconnectNetworkHost(host.id, host.name, host.ipAddress)
                    if (unmounted.length > 0) {
                        addToast(tString('fileExplorer.network.browser.disconnected', { hostName: payload.hostName }), {
                            level: 'success',
                        })
                    } else {
                        addToast(tString('fileExplorer.network.browser.noMountedShares', { hostName: payload.hostName }))
                    }
                } catch (e) {
                    addToast(tString('fileExplorer.network.browser.disconnectFailed', { message: String(e) }), {
                        level: 'error',
                    })
                }
                break
            }
        }
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

    // The keyboard-shortcut chip rendered inline in the refresh hint (`<key>` tag).
    // The chip key is a fixed combo, not translatable, so the snippet ignores the
    // (empty) inner content and renders the chip itself.
    const snippets = { key: refreshKeyChip }
</script>

{#snippet refreshKeyChip(_children: import('svelte').Snippet)}<ShortcutChip key="⌘R" size="sm" />{/snippet}

<div class="network-browser" class:is-focused={isFocused}>
    <div class="header-row">
        <span class="col-name">{tString('fileExplorer.network.browser.colName')}</span>
        <span class="col-ip">{tString('fileExplorer.network.browser.colIp')}</span>
        <span class="col-hostname">{tString('fileExplorer.network.browser.colHostname')}</span>
        <span class="col-shares">{tString('fileExplorer.network.browser.colShares')}</span>
        <span class="col-status">{tString('fileExplorer.network.browser.colStatus')}</span>
    </div>
    <div class="host-list" bind:this={listContainer} bind:clientHeight={containerHeight}>
        {#each hosts as host, index (host.id)}
            {@const hostStatus = getHostStatus(host)}
            {@const statusIcon = STATUS_ICON[hostStatus.kind]}
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
                oncontextmenu={(e: MouseEvent) => {
                    void handleHostContextMenu(e, host)
                }}
                onkeydown={() => {}}
            >
                <span class="col-name">
                    <span class="host-icon"><Icon name="monitor" size={16} aria-hidden="true" /></span>
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
                    use:tooltip={getStatusTooltip(host)}
                >
                    {#if statusIcon}<Icon name={statusIcon} size={13} aria-hidden="true" />{/if}
                    <span class="status-text">{tString(STATUS_TEXT_KEY[hostStatus.kind])}</span>
                    {#if hostStatus.stale}<Icon name="rotate-cw" size={12} aria-hidden="true" />{/if}
                    {#if hostStatus.hasInfo}<Icon name="info" size={12} aria-hidden="true" />{/if}
                </span>
            </div>
        {/each}

        {#if isSearching}
            <div class="searching-indicator">
                <Spinner size="sm" />
                {tString('fileExplorer.network.browser.searching')}
            </div>
        {/if}

        <!-- "Connect to server..." pseudo-row, always at the bottom, keyboard navigable -->
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <div
            class="host-row connect-row"
            class:is-under-cursor={isCursorOnConnectRow}
            class:is-focused-and-under-cursor={isFocused && isCursorOnConnectRow}
            role="listitem"
            onclick={handleConnectRowClick}
            ondblclick={handleConnectRowDoubleClick}
            onkeydown={() => {}}
        >
            <span class="col-name connect-label">
                <span class="connect-icon">+</span>
                <span>{tString('fileExplorer.network.browser.connectToServerRow')}</span>
            </span>
        </div>

        {#if !isSearching && hosts.length === 0}
            <div class="empty-state">
                <img class="empty-icon" src="/icons/network-no-hosts.svg" alt="" />
                <div class="empty-title">{tString('fileExplorer.network.browser.noHostsTitle')}</div>
                <div class="empty-message">{tString('fileExplorer.network.browser.noHostsMessage')}</div>
                <Button variant="secondary" onclick={handleRefreshClick}
                    >{tString('fileExplorer.network.browser.refresh')}</Button
                >
            </div>
        {/if}
    </div>

    {#if hosts.length > 0}
        <button
            class="network-status-bar"
            onclick={handleRefreshClick}
            aria-label={tString('fileExplorer.network.browser.refreshAriaLabel')}
        >
            <span class="status-text"
                >{tString('fileExplorer.network.browser.hostCount', {
                    count: hosts.length,
                    countText: formatInteger(hosts.length),
                })}</span
            >
            <span class="refresh-hint"><Trans key="fileExplorer.network.browser.refreshHint" {snippets} /></span>
        </button>
    {/if}
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
        padding: var(--spacing-xs) var(--spacing-sm);
        background-color: var(--color-bg-secondary);
        border-bottom: 1px solid var(--color-border-strong);
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
        background-color: var(--color-cursor-inactive);
    }

    .host-row.is-focused-and-under-cursor {
        background-color: var(--color-cursor-active);
    }

    .col-name {
        flex: 2;
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
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
        color: var(--color-text-tertiary);
    }

    .col-shares {
        flex: 1;
        color: var(--color-text-tertiary);
        text-align: center;
    }

    .col-status {
        flex: 2.5;
        display: flex;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-xxs);
        color: var(--color-text-tertiary);
    }

    .host-icon {
        display: inline-flex;
        align-items: center;
        color: var(--color-text-secondary);
    }

    .connect-row .connect-label {
        color: var(--color-text-tertiary);
        font-style: italic;
    }

    .connect-icon {
        font-style: normal;
        font-weight: 600;
        font-size: var(--font-size-md);
        color: var(--color-text-tertiary);
    }

    .searching-indicator {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-md) var(--spacing-lg);
        color: var(--color-text-tertiary);
        font-style: italic;
    }

    .empty-state {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        height: 100%;
        padding: var(--spacing-xl);
        gap: var(--spacing-md);
        color: var(--color-text-secondary);
    }

    .empty-icon {
        width: 96px;
        height: 96px;
    }

    .empty-title {
        font-size: var(--font-size-lg);
        font-weight: 500;
        color: var(--color-text-primary);
    }

    .empty-message {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        text-align: center;
    }

    .col-shares.is-fetching {
        font-style: italic;
        color: var(--color-text-tertiary);
    }

    .col-shares.is-stale {
        color: var(--color-text-tertiary);
    }

    .col-status.is-error {
        color: var(--color-error);
        cursor: help;
    }

    .col-status.needs-login {
        color: var(--color-warning);
        cursor: help;
    }

    .network-status-bar {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        width: 100%;
        padding: var(--spacing-xs) var(--spacing-sm);
        font-family: var(--font-system), sans-serif;
        font-size: calc(var(--font-size-sm) * 0.95);
        color: var(--color-text-secondary);
        background-color: var(--color-bg-secondary);
        border: none;
        border-top: 1px solid var(--color-border-strong);
        min-height: 1.5em;
        text-align: left;
    }

    .status-text {
        flex: 1 1 0;
        min-width: 0;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .refresh-hint {
        flex-shrink: 0;
        margin-left: auto;
        padding-left: var(--spacing-md);
        color: var(--color-text-tertiary);
        white-space: nowrap;
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xxs);
    }
</style>
