<script lang="ts">
    /**
     * The servers hub: every server the user saved, plus every host mDNS found,
     * in one table. Rendered when a pane is on the `network` volume.
     *
     * The merge, the ordering, and the MCP encoding are pure and live next door
     * (`servers-hub-rows.ts`, `servers-hub-mcp.ts`); this file is the table, the
     * cursor, and the keys.
     */
    import { onMount, onDestroy } from 'svelte'
    import Button from '$lib/ui/Button.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import DateLabel from '$lib/ui/DateLabel.svelte'
    import {
        getNetworkHosts,
        getDiscoveryState,
        getShareCount,
        clearShareState,
        fetchShares,
        refreshAllStaleShares,
        getCredentialStatus,
        checkCredentialsForHost,
        forgetCredentials,
    } from './network-store.svelte'
    import { getStatusTooltip } from './host-status'
    import { buildHubRows, type HubRow, type HubRowStatus } from './servers-hub-rows'
    import { hubMcpEntries } from './servers-hub-mcp'
    import { tooltip } from '$lib/tooltip/tooltip'
    import type { NetworkHost, VolumeInfo } from '../types'
    import {
        updateLeftPaneState,
        updateRightPaneState,
        removeManualServer,
        showNetworkHostContextMenu,
        onNetworkHostContextAction,
        disconnectNetworkHost,
        listSavedServers,
        type PaneState,
        type SavedServer,
    } from '$lib/tauri-commands'
    import { getVolumes } from '$lib/stores/volume-store.svelte'
    import { getNetworkEnabled } from '$lib/settings/reactive-settings.svelte'
    import { openSettingsWindow, settingAnchorId } from '$lib/settings/settings-window'
    import { handleNavigationShortcut } from '../navigation/keyboard-shortcuts'
    import { protocolLabel } from '../navigation/filesystem-label'
    import { forgetSavedServer, openServerRowMenu } from '../navigation/server-row-actions'
    import { confirmDialog } from '$lib/utils/confirm-dialog'
    import { addToast } from '$lib/ui/toast'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import { eventMatchesCommand } from '$lib/shortcuts'
    import { triggerNetworkDiscovery } from './lazy-trigger'
    import { tString } from '$lib/intl/messages.svelte'
    import type { MessageKey } from '$lib/intl/keys.gen'
    import Trans from '$lib/intl/Trans.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import { getAppLogger } from '$lib/logging/logger'

    const log = getAppLogger('servers')

    /** Row height (matches Full list). */
    const ROW_HEIGHT = 20

    /** The word each status wears in the Status column. */
    const STATUS_TEXT_KEY: Record<HubRowStatus, MessageKey> = {
        connected: 'servers.hub.status.connected',
        saved: 'servers.hub.status.saved',
        found_nearby: 'servers.hub.status.foundNearby',
        signed_out: 'servers.hub.status.signedOut',
        waiting_for_key: 'servers.hub.status.waitingForKey',
    }

    interface Props {
        paneId?: 'left' | 'right'
        isFocused?: boolean
        /** Enter on an SMB host: open its places list. */
        onHostSelect?: (host: NetworkHost) => void
        /** Enter on a one-place server: take the pane there. */
        onServerSelect?: (row: HubRow) => void
        /** Enter on the "Add server…" row. */
        onConnectToServer?: () => void
    }

    const { paneId, isFocused = false, onHostSelect, onServerSelect, onConnectToServer }: Props = $props()

    /** `listSavedServers()`, refreshed whenever the volume list is. */
    let savedServers = $state<SavedServer[]>([])

    const hosts = $derived(getNetworkHosts())
    const volumes = $derived(getVolumes())
    const isSearching = $derived(getDiscoveryState() === 'searching')
    const discoveryEnabled = $derived(getNetworkEnabled())
    const rows = $derived(buildHubRows({ saved: savedServers, hosts, volumes }))

    let cursorIndex = $state(0)
    let listContainer: HTMLDivElement | undefined = $state()
    let containerHeight = $state(0)
    let unlistenContextAction: (() => void) | undefined

    /**
     * Re-reads the saved list whenever the volume list changes.
     *
     * ❗ Subscribe, don't poll: pin, forget, connect, and disconnect all emit
     * `volumes-changed`, and the volume store's array is reassigned on each one.
     * Reading it here is the whole subscription.
     */
    $effect(() => {
        void volumes
        void refreshSavedServers()
    })

    onMount(() => {
        // Lazy-start mDNS the first time the user enters the hub. No-op when
        // discovery is already running or the setting is off.
        triggerNetworkDiscovery()
        refreshAllStaleShares()

        void onNetworkHostContextAction((payload) => {
            void handleHostContextAction(payload)
        }).then((fn) => {
            unlistenContextAction = fn
        })
    })

    onDestroy(() => {
        unlistenContextAction?.()
    })

    // Re-sync MCP state when the rows or the cursor change.
    $effect(() => {
        void rows
        void cursorIndex
        void syncPaneStateToMcp()
    })

    // Clamp the cursor when a row disappears (a host went quiet, a server was forgotten).
    $effect(() => {
        const maxIndex = totalNavigableItems - 1
        if (cursorIndex > maxIndex) {
            cursorIndex = Math.max(0, maxIndex)
        }
    })

    async function refreshSavedServers(): Promise<void> {
        try {
            // `Array.isArray` because this is an IPC boundary: a command that
            // answered with nothing would otherwise put `undefined` where the
            // merge iterates, and the hub would render nothing at all.
            const answer: unknown = await listSavedServers()
            savedServers = Array.isArray(answer) ? (answer as SavedServer[]) : []
        } catch (e) {
            // A store that didn't answer costs the hub its saved rows, never the
            // nearby ones: the list is still useful, and the next `volumes-changed`
            // tries again.
            log.warn('Reading the saved servers broke down: {error}', { error: String(e) })
        }
    }

    /** Every row plus the "Add server…" row. */
    const totalNavigableItems = $derived(rows.length + 1)

    /** Whether the cursor sits on the "Add server…" row. */
    const isCursorOnAddRow = $derived(cursorIndex === rows.length)

    /** The row under the cursor, or `null` on the add row. */
    function rowUnderCursor(): HubRow | null {
        if (isCursorOnAddRow) return null
        return rows[cursorIndex] ?? null
    }

    /**
     * Mirrors the hub into `cmdr://state`.
     *
     * The columns a person reads are encoded into each entry's `name`, because
     * MCP's `PaneFileEntry` has only `name` / `path` / `isDirectory`.
     */
    async function syncPaneStateToMcp() {
        if (!paneId) return
        try {
            const state: PaneState = {
                path: 'smb://',
                volumeId: 'network',
                volumeName: tString('fileExplorer.navigation.networkVolume'),
                files: hubMcpEntries(rows, {
                    appRootOf: (row) => row.saved?.places[0]?.appRoot ?? null,
                    shareCountOf: (row) => (row.host ? getShareCount(row.host.id) : undefined),
                }),
                cursorIndex,
                viewMode: 'full',
                selectedIndices: [],
                totalFiles: rows.length,
                loadedStart: 0,
                loadedEnd: rows.length,
            }
            await (paneId === 'left' ? updateLeftPaneState(state) : updateRightPaneState(state))
        } catch {
            // MCP mirroring is optional; a failed push must not touch the UI.
        }
    }

    function scrollToIndex(index: number) {
        if (!listContainer) return
        const targetTop = index * ROW_HEIGHT
        const targetBottom = targetTop + ROW_HEIGHT
        const scrollTop = listContainer.scrollTop
        const viewportBottom = scrollTop + containerHeight

        if (targetTop < scrollTop) {
            listContainer.scrollTop = targetTop
        } else if (targetBottom > viewportBottom) {
            listContainer.scrollTop = targetBottom - containerHeight
        }
    }

    // noinspection JSUnusedGlobalSymbols -- used dynamically by MCP move_cursor
    export function setCursorIndex(index: number) {
        cursorIndex = Math.max(0, Math.min(index, totalNavigableItems - 1))
        scrollToIndex(cursorIndex)
    }

    // noinspection JSUnusedGlobalSymbols -- used dynamically by MCP move_cursor's range check
    export function getItemCount(): number {
        return totalNavigableItems
    }

    /** Refresh everything the hub shows (⌘R). */
    export function refresh() {
        handleRefreshClick()
    }

    /** Find a row by name, returns its index or -1. */
    // noinspection JSUnusedGlobalSymbols -- used dynamically
    export function findItemIndex(name: string): number {
        return rows.findIndex((row) => row.name.toLowerCase() === name.toLowerCase())
    }

    /**
     * The SMB host under the cursor, or `null`. Consumed by "Copy path between
     * panes", which mirrors a host into the other pane.
     */
    // noinspection JSUnusedGlobalSymbols -- used dynamically by NetworkMountView
    export function getHostUnderCursor(): NetworkHost | null {
        return rowUnderCursor()?.host ?? null
    }

    /**
     * The whole row under the cursor, or `null` on the add row.
     *
     * ❗ How the palette commands ("Pin / unpin server", "Disconnect server",
     * "Forget saved password") reach what the user is looking at: the hub IS a
     * pane, so a command acting on "the focused pane's volume" would otherwise
     * act on the synthetic hub row.
     */
    // noinspection JSUnusedGlobalSymbols -- used dynamically by NetworkMountView / FilePane
    export function getRowUnderCursor(): HubRow | null {
        return rowUnderCursor()
    }

    /** Opens whatever the cursor is on — the same action Enter triggers. */
    // noinspection JSUnusedGlobalSymbols -- used dynamically by NetworkMountView / MCP
    export function openCursorItem(): void {
        if (isCursorOnAddRow) {
            onConnectToServer?.()
            return
        }
        const row = rowUnderCursor()
        if (row) openRow(row)
    }

    /**
     * What Enter does to a row.
     *
     * An SMB host opens its places list; a one-place server takes the pane to its
     * place, where the pane's own connect view does the dialing.
     */
    function openRow(row: HubRow): void {
        if (row.protocol === 'smb') {
            onHostSelect?.(row.host ?? savedHostFor(row))
            return
        }
        onServerSelect?.(row)
    }

    /**
     * A saved SMB host mDNS isn't seeing right now, as a host the places list can
     * take. Its address is the only spelling anything has for it.
     */
    function savedHostFor(row: HubRow): NetworkHost {
        return { id: row.id, name: row.name, hostname: row.address, port: 445, source: 'manual' }
    }

    /** Arrow keys and Enter. */
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
                openCursorItem()
                return true
            default:
                return false
        }
    }

    /**
     * Handle keyboard navigation. This runs BEFORE the document-level dispatcher in
     * `+page.svelte`, which is registered bubble-phase on `document` and this pane is
     * a descendant. So a branch that acts on a Tier 1 combo must `stopPropagation()`,
     * or the dispatcher runs the same command again (it has no `defaultPrevented`
     * guard). Nothing reads a return value: `preventDefault` + `stopPropagation` is
     * how a branch says it claimed the key.
     */
    // noinspection JSUnusedGlobalSymbols -- used dynamically
    export function handleKeyDown(e: KeyboardEvent): void {
        // The refresh key (⌘R by default) works regardless of row count. Read through
        // the registry so a rebind follows. `stopPropagation` keeps it to ONE refresh:
        // `pane.refresh` is centrally dispatched too, and `refreshPane` routes it back
        // into this component through `refreshNetworkHosts()` → `refresh()`, which is
        // this same `handleRefreshClick()`. Without it, every host got re-read twice.
        if (eventMatchesCommand(e, 'pane.refresh')) {
            e.preventDefault()
            e.stopPropagation()
            handleRefreshClick()
            return
        }

        // Try centralized navigation shortcuts first (PageUp, PageDown, Home, End, Option+arrows)
        const visibleItems = Math.max(1, Math.floor(containerHeight / ROW_HEIGHT))
        const navResult = handleNavigationShortcut(e, {
            currentIndex: cursorIndex,
            totalCount: totalNavigableItems,
            visibleItems,
        })
        if (navResult?.handled) {
            e.preventDefault()
            cursorIndex = navResult.newIndex
            scrollToIndex(cursorIndex)
            return
        }

        // Everything below is an unmodified key. Matching the whole combo (rather
        // than just `e.key`) keeps ⇧F8 (delete permanently), ⌘↑/⌘↓ (parent/open), and
        // ⌘←/⌘→ (copy path between panes) reaching the document dispatcher instead of
        // also moving this cursor.
        if (e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return

        // F8: forget the saved server under the cursor.
        if (e.key === 'F8') {
            const row = rowUnderCursor()
            if (row) {
                e.preventDefault()
                void handleForget(row)
            }
            return
        }

        if (handleArrowAndEnter(e.key)) {
            e.preventDefault()
        }
    }

    function handleRowClick(index: number) {
        cursorIndex = index
    }

    function handleRowDoubleClick(index: number) {
        const row = rows[index]
        if (row) openRow(row)
    }

    function handleAddRowClick() {
        cursorIndex = rows.length
    }

    /** The protocol name, from the same map the volume switcher's slot reads. */
    function typeLabel(row: HubRow): string {
        return protocolLabel(row.protocol) ?? row.protocol.toUpperCase()
    }

    /** `Last used`, as the seconds-based `DateLabel` takes it. */
    function lastUsedSeconds(row: HubRow): number | null {
        if (!row.lastConnectedAt) return null
        const parsed = Date.parse(row.lastConnectedAt)
        return Number.isNaN(parsed) ? null : Math.floor(parsed / 1000)
    }

    /**
     * F8. A saved server goes (after a confirmation); a host only mDNS knows about
     * has nothing to forget, and says so.
     */
    async function handleForget(row: HubRow): Promise<void> {
        if (!row.saved) {
            addToast(tString('fileExplorer.network.browser.cannotRemoveDiscovered'), { level: 'warn' })
            return
        }
        if (row.volumeId) {
            // A one-place server: the servers family owns the confirmation and the
            // toast, so the hub and the switcher's menu ask the same question.
            await forgetSavedServer(row.volumeId, row.name)
            return
        }
        await removeSavedSmbHost(row)
    }

    /** Forgets a saved SMB host, which is a manual-server entry rather than a place. */
    async function removeSavedSmbHost(row: HubRow): Promise<void> {
        const confirmed = await confirmDialog(
            tString('fileExplorer.network.browser.removeHostConfirm', { hostName: row.name }),
            tString('fileExplorer.network.browser.removeHostConfirmButton'),
        )
        if (!confirmed) return
        try {
            await removeManualServer(row.id)
            addToast(tString('fileExplorer.network.browser.hostRemoved', { hostName: row.name }), { level: 'success' })
            await refreshSavedServers()
        } catch {
            addToast(tString('fileExplorer.network.browser.hostRemoveFailed', { hostName: row.name }), {
                level: 'error',
            })
        }
    }

    /**
     * Right-click.
     *
     * A one-place row raises the SERVERS menu (Disconnect, Forget saved password,
     * Forget server), the same one the switcher row raises, so the two surfaces
     * can't drift. An SMB host keeps its own host menu, whose Disconnect unmounts
     * shares rather than dropping a session.
     */
    async function handleRowContextMenu(e: MouseEvent, row: HubRow): Promise<void> {
        e.preventDefault()
        if (row.volumeId) {
            await openServerRowMenu(volumeForRow(row))
            return
        }
        const host = row.host
        if (!host) return
        if (getCredentialStatus(host.name) === 'unknown') {
            await checkCredentialsForHost(host.name)
        }
        void showNetworkHostContextMenu(
            host.id,
            host.name,
            host.source === 'manual',
            getCredentialStatus(host.name) === 'has_creds',
        )
    }

    /**
     * The row's `VolumeInfo`, for the menu builder.
     *
     * The volume list is the source when it has the row; a saved server that is
     * neither pinned nor connected has no row there, and the stand-in carries the
     * three fields the menu actually reads.
     */
    function volumeForRow(row: HubRow): VolumeInfo {
        const known = volumes.find((volume) => volume.id === row.volumeId)
        if (known) return known
        return {
            id: row.volumeId ?? row.id,
            name: row.name,
            path: row.saved?.places[0]?.appRoot ?? '',
            category: 'network',
            isEjectable: false,
            fsType: row.protocol,
            connectionState: null,
        }
    }

    /** Actions dispatched from the native SMB-host context menu. */
    async function handleHostContextAction(payload: { action: string; hostId: string; hostName: string }) {
        switch (payload.action) {
            case 'forget-server': {
                const row = rows.find((r) => r.host?.id === payload.hostId || r.id === payload.hostId)
                if (row) await handleForget(row)
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
                const host = hosts.find((h) => h.id === payload.hostId)
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

    /** Re-read the saved list and re-fetch every host's shares (user-initiated). */
    function handleRefreshClick() {
        void refreshSavedServers()
        for (const host of hosts) {
            clearShareState(host.id)
            if (host.hostname) {
                fetchShares(host).catch(() => {
                    // Errors are stored in shareStates, ignore here
                })
            }
        }
    }

    /** Opens Settings at the switch that turns local network discovery back on. */
    function openDiscoverySetting() {
        void openSettingsWindow(
            'servers-hub',
            ['File systems', 'SMB/Network shares'],
            settingAnchorId('network.enabled'),
        )
    }

    // The keyboard-shortcut chip rendered inline in the refresh hint (`<key>` tag).
    // It reads the live `pane.refresh` binding, so a rebind moves the hint with it.
    // Non-clickable: the whole status bar is already a refresh button, and a nested
    // click target would double-activate. The snippet ignores the (empty) inner
    // content and renders the chip itself.
    const snippets = { key: refreshKeyChip }
</script>

{#snippet refreshKeyChip(_children: import('svelte').Snippet)}<ShortcutChip
        commandId="pane.refresh"
        clickable={false}
        size="sm"
    />{/snippet}

<div class="servers-hub" class:is-focused={isFocused}>
    <div class="header-row">
        <span class="col-name">{tString('servers.hub.colName')}</span>
        <span class="col-type">{tString('servers.hub.colType')}</span>
        <span class="col-address">{tString('servers.hub.colAddress')}</span>
        <span class="col-status">{tString('servers.hub.colStatus')}</span>
        <span class="col-last-used">{tString('servers.hub.colLastUsed')}</span>
    </div>
    <div class="row-list" bind:this={listContainer} bind:clientHeight={containerHeight}>
        {#each rows as row, index (row.id)}
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
            <div
                class="server-row"
                class:is-under-cursor={index === cursorIndex}
                class:is-focused-and-under-cursor={isFocused && index === cursorIndex}
                role="listitem"
                onclick={() => {
                    handleRowClick(index)
                }}
                ondblclick={() => {
                    handleRowDoubleClick(index)
                }}
                oncontextmenu={(e: MouseEvent) => {
                    void handleRowContextMenu(e, row)
                }}
                onkeydown={() => {}}
            >
                <span class="col-name" use:tooltip={{ text: row.name, overflowOnly: true }}>
                    <span class="row-icon"
                        ><Icon name={row.protocol === 'smb' ? 'monitor' : 'server'} size={16} aria-hidden="true" /></span
                    >
                    {row.name}
                </span>
                <span class="col-type">{typeLabel(row)}</span>
                <span class="col-address" use:tooltip={{ text: row.address, overflowOnly: true }}>{row.address}</span>
                <span
                    class="col-status"
                    class:needs-you={row.status === 'signed_out' || row.status === 'waiting_for_key'}
                    class:is-live={row.status === 'connected'}
                    use:tooltip={row.host ? getStatusTooltip(row.host) : ''}
                >
                    {tString(STATUS_TEXT_KEY[row.status])}
                </span>
                <span class="col-last-used">
                    {#if lastUsedSeconds(row) === null}
                        <span class="never-used">{tString('servers.hub.neverUsed')}</span>
                    {:else}
                        <DateLabel modifiedAt={lastUsedSeconds(row)} />
                    {/if}
                </span>
            </div>
        {/each}

        {#if isSearching}
            <div class="searching-indicator">
                <Spinner size="sm" />
                {tString('fileExplorer.network.browser.searching')}
            </div>
        {/if}

        {#if !discoveryEnabled}
            <!--
                Discovery off: the saved servers above are unaffected (SFTP and WebDAV
                never needed the macOS Local Network permission), so this line replaces
                the nearby hosts and nothing else.
            -->
            <div class="discovery-off">
                <span>{tString('servers.hub.discoveryOff')}</span>
                <LinkButton onclick={openDiscoverySetting}
                    >{tString('servers.hub.discoveryOffLink')}</LinkButton
                >
            </div>
        {/if}

        <!-- "Add server…" pseudo-row, always at the bottom, keyboard navigable -->
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <div
            class="server-row add-row"
            class:is-under-cursor={isCursorOnAddRow}
            class:is-focused-and-under-cursor={isFocused && isCursorOnAddRow}
            role="listitem"
            onclick={handleAddRowClick}
            ondblclick={() => onConnectToServer?.()}
            onkeydown={() => {}}
        >
            <span class="col-name add-label">
                <span class="add-icon">+</span>
                <span>{tString('servers.hub.addServer')}</span>
            </span>
        </div>

        {#if !isSearching && rows.length === 0}
            <div class="empty-state">
                <img class="empty-icon" src="/icons/network-no-hosts.svg" alt="" />
                <div class="empty-title">{tString('servers.hub.emptyTitle')}</div>
                <div class="empty-message">{tString('servers.hub.emptyMessage')}</div>
                <Button variant="secondary" onclick={handleRefreshClick}
                    >{tString('fileExplorer.network.browser.refresh')}</Button
                >
            </div>
        {/if}
    </div>

    {#if rows.length > 0}
        <button
            class="hub-status-bar"
            onclick={handleRefreshClick}
            aria-label={tString('fileExplorer.network.browser.refreshAriaLabel')}
        >
            <span class="status-text"
                >{tString('servers.hub.rowCount', {
                    count: rows.length,
                    countText: formatInteger(rows.length),
                })}</span
            >
            <span class="refresh-hint"><Trans key="fileExplorer.network.browser.refreshHint" {snippets} /></span>
        </button>
    {/if}
</div>

<style>
    .servers-hub {
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

    .row-list {
        flex: 1;
        overflow-y: auto;
    }

    .server-row {
        display: flex;
        height: 20px;
        padding: var(--spacing-xxs) var(--spacing-sm);
        cursor: default;
    }

    .server-row.is-under-cursor {
        background-color: var(--color-cursor-inactive);
    }

    .server-row.is-focused-and-under-cursor {
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

    .col-type {
        flex: 1;
        color: var(--color-text-secondary);
        white-space: nowrap;
    }

    .col-address {
        flex: 2;
        color: var(--color-text-secondary);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .col-status {
        flex: 2;
        display: flex;
        align-items: center;
        gap: var(--spacing-xxs);
        color: var(--color-text-tertiary);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .col-status.is-live {
        color: var(--color-text-secondary);
    }

    .col-status.needs-you {
        color: var(--color-warning);
    }

    .col-last-used {
        flex: 1.5;
        color: var(--color-text-tertiary);
        overflow: hidden;
        white-space: nowrap;
    }

    .never-used {
        color: var(--color-text-tertiary);
    }

    .row-icon {
        display: inline-flex;
        align-items: center;
        color: var(--color-text-secondary);
    }

    .add-row .add-label {
        color: var(--color-text-tertiary);
        font-style: italic;
    }

    .add-icon {
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

    .discovery-off {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-sm) var(--spacing-lg);
        color: var(--color-text-tertiary);
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

    .hub-status-bar {
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
