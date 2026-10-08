<script lang="ts">
    /**
     * The servers hub: every server the user saved, then the hosts mDNS found
     * that they didn't, folded into one group. Rendered when a pane is on the
     * `network` volume.
     *
     * The merge, the ordering, the grouping, and the MCP encoding are pure and
     * live next door (`servers-hub-rows.ts`, `servers-hub-items.ts`,
     * `servers-hub-mcp.ts`); this file is the table, the cursor, and the keys.
     */
    import { onMount, onDestroy, untrack } from 'svelte'
    import { dependOn } from '$lib/utils/reactivity'
    import Button from '$lib/ui/Button.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import DateLabel from '$lib/ui/DateLabel.svelte'
    import {
        getDiscoveryState,
        getShareCount,
        clearShareState,
        fetchShares,
        refreshAllStaleShares,
    } from './network-store.svelte'
    import { getStatusTooltip } from './host-status'
    import { hubRowIcon, lastUsedSeconds, openMoveFor, type HubRow, type HubRowStatus } from './servers-hub-rows'
    import { visibleIndexOf, type HubItem } from './servers-hub-items'
    import { createHubList } from './servers-hub-list.svelte'
    import { hubPaneState, NEARBY_GROUP_MCP_NAME } from './servers-hub-mcp'
    import { createHubActions, type HubRowMenuAPI } from './servers-hub-actions'
    import { cursorAcrossRebuild } from './servers-hub-keys'
    import { cursorAfterArrow } from './list-cursor'
    import { hubColumnsAt } from './servers-hub-columns'
    import { useInlineSize } from '$lib/utils/inline-size-action'
    import ServersHubRowMenu from './ServersHubRowMenu.svelte'
    import ServersHubStatusBar from './ServersHubStatusBar.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { rowAnchorIn } from '../pane/context-menu-anchor'
    import type { NetworkHost } from '../types'
    import { updateLeftPaneState, updateRightPaneState, onNetworkHostContextAction } from '$lib/tauri-commands'
    import { getNetworkEnabled } from '$lib/settings/reactive-settings.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import { openSettingsWindow, settingAnchorId } from '$lib/settings/settings-window'
    import { handleNavigationShortcut } from '../navigation/keyboard-shortcuts'
    import { protocolLabel } from '../navigation/filesystem-label'
    import { eventMatchesCommand } from '$lib/shortcuts'
    import { claimKey } from '$lib/shortcuts/claim-key'
    import { triggerNetworkDiscovery } from './lazy-trigger'
    import { signedInAsLabel } from './signed-in-as-label'
    import { tString } from '$lib/intl/messages.svelte'
    import { addToast } from '$lib/ui/toast'
    import type { MessageKey } from '$lib/intl/keys.gen'
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
        onHostSelect?: (host: NetworkHost, label?: string) => void
        /** Enter on a one-place server, or on a saved share the volume list has a place for: take the pane there. */
        onServerSelect?: (row: HubRow) => void
        /**
         * Enter on a saved share no mount went through yet: open its host's
         * share list and mount that one share, as the host's account.
         */
        onShareViaHost?: (host: NetworkHost, via: { share: string; label: string }) => void
        /** Enter on the "Add server…" row. */
        onConnectToServer?: () => void
    }

    const { paneId, isFocused = false, onHostSelect, onServerSelect, onShareViaHost, onConnectToServer }: Props =
        $props()

    /** What this hub lists: the saved servers it read, its rows and items, and whether the nearby group is open. */
    const list = createHubList()
    const volumes = $derived(list.volumes)
    const rows = $derived(list.rows)
    const items = $derived(list.items)
    const visibleItems = $derived(list.visibleItems)
    const nearbyGroup = $derived(items.find((item) => item.kind === 'nearby_group'))
    const isSearching = $derived(getDiscoveryState() === 'searching')
    const discoveryEnabled = $derived(getNetworkEnabled())

    /**
     * F8, the row menus, and the SMB host menu's answers. Live getters, ❌ never
     * snapshots: the rows change under a menu that is still open.
     */
    const actions = createHubActions({
        getRows: () => rows,
        getVolumes: () => volumes,
        refreshSaved: list.refreshSaved,
        openRow: (row) => {
            openRow(row)
        },
    })

    let rowMenu: HubRowMenuAPI | undefined = $state()

    /** ⌃⏎ (`file.contextMenu`): the cursor row's menu, just under the row as a file row's opens. None on "Add server…". */
    // noinspection JSUnusedGlobalSymbols -- used dynamically by NetworkMountView
    export async function openContextMenuAtCursor(): Promise<void> {
        const row = rowUnderCursor()
        const anchor = rowAnchorIn(listContainer ?? null, `[data-hub-row="${String(cursorIndex)}"]`)
        if (row) await rowMenu?.open(row, anchor ?? { x: 0, y: 0 }, anchor)
    }

    let cursorIndex = $state(0)
    /** The list's content width, which decides the columns that fit (`hubColumnsAt`). */
    let hubWidth = $state(0)
    const columns = $derived(hubColumnsAt(hubWidth))
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
        dependOn(volumes)
        void list.refreshSaved()
    })

    onMount(() => {
        // Lazy-start mDNS the first time the user enters the hub. No-op when
        // discovery is already running or the setting is off.
        triggerNetworkDiscovery()
        refreshAllStaleShares()

        void onNetworkHostContextAction((payload) => {
            void actions.runHostAction(payload)
        }).then((fn) => {
            unlistenContextAction = fn
        })
    })

    onDestroy(() => {
        unlistenContextAction?.()
    })

    // Re-sync MCP state when the items or the cursor change.
    $effect(() => {
        dependOn(items, cursorIndex)
        void syncPaneStateToMcp()
    })

    /** The items the cursor last sat over: a rebuild keeps it on the same one, clamped if it left. */
    let itemsBefore: HubItem[] = []
    $effect.pre(() => {
        const after = visibleItems
        untrack(() => {
            cursorIndex = cursorAcrossRebuild(itemsBefore, after, cursorIndex)
            itemsBefore = after
        })
    })

    /**
     * Servers, for the status bar: a share row is a place under one, not a server.
     * The ones a collapsed nearby group hides count too; its header says how many
     * of the total those are.
     */
    const serverCount = $derived(rows.filter((row) => row.kind === 'server').length)

    /** Every item on screen plus the "Add server…" row. */
    const totalNavigableItems = $derived(visibleItems.length + 1)

    /** Whether the cursor sits on the "Add server…" row. */
    const isCursorOnAddRow = $derived(cursorIndex === visibleItems.length)

    /** The item under the cursor, or `null` on the add row. */
    function itemUnderCursor(): HubItem | null {
        if (cursorIndex < 0 || cursorIndex >= visibleItems.length) return null
        return visibleItems[cursorIndex]
    }

    /** The row under the cursor, or `null` on the add row and on the nearby group's header. */
    function rowUnderCursor(): HubRow | null {
        const item = itemUnderCursor()
        return item?.kind === 'row' ? item.row : null
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
            const state = hubPaneState(items, cursorIndex, tString('fileExplorer.navigation.networkVolume'), {
                appRootOf: (row) => row.saved?.places[0]?.appRoot ?? null,
                shareCountOf: (row) => (row.host ? getShareCount(row.host.id) : undefined),
            })
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

    /**
     * Puts the cursor on the item at `index` of the FULL list, the one
     * `cmdr://state` publishes and `findItemIndex` answers in.
     *
     * ❗ A server a collapsed nearby group hides gets the group opened for it
     * (`revealNearby`): an agent that read a server's name in the state has to
     * be able to reach it, and the cursor never sits on a row nobody can see.
     */
    // noinspection JSUnusedGlobalSymbols -- used dynamically by MCP move_cursor
    export function setCursorIndex(index: number) {
        const fullIndex = Math.max(0, Math.min(index, list.items.length))
        if (visibleIndexOf(list.items, fullIndex) === null) {
            list.revealNearby()
            // The rebuild this causes follows the item the cursor WAS on. It's set
            // right below, against the opened list, so that list is its "before".
            itemsBefore = list.visibleItems
        }
        cursorIndex = visibleIndexOf(list.items, fullIndex) ?? list.visibleItems.length
        scrollToIndex(cursorIndex)
    }

    /** How many items the FULL list holds, "Add server…" included: what an index is range-checked against. */
    // noinspection JSUnusedGlobalSymbols -- used dynamically by MCP move_cursor's range check
    export function getItemCount(): number {
        return items.length + 1
    }

    /** Refresh everything the hub shows (⌘R). */
    export function refresh() {
        handleRefreshClick()
    }

    /** The server an "Add" just saved: the list learns of it a moment later, so this waits for the row. */
    let pendingSelection = $state<string | null>(null)

    $effect(() => {
        const id = pendingSelection
        if (id === null) return
        const index = items.findIndex(
            (item) => item.kind === 'row' && (item.row.id === id || item.row.host?.id === id),
        )
        if (index < 0) return
        pendingSelection = null
        untrack(() => {
            setCursorIndex(index)
        })
    })

    /** Selects the server `id` names (saved or discovery id), now or once listed: "Add"'s proof (cmdr-reports#6). */
    // noinspection JSUnusedGlobalSymbols -- used by NetworkMountView after an Add
    export function selectServer(id: string) {
        pendingSelection = id
        void list.refreshSaved()
    }

    /**
     * Finds a row by name, or the nearby group's header by the name `cmdr://state`
     * gives it. Answers its index in the FULL list, or -1.
     */
    // noinspection JSUnusedGlobalSymbols -- used dynamically
    export function findItemIndex(name: string): number {
        const wanted = name.toLowerCase()
        return items.findIndex(
            (item) => (item.kind === 'row' ? item.row.name : NEARBY_GROUP_MCP_NAME).toLowerCase() === wanted,
        )
    }

    /**
     * The SMB host under the cursor, or `null`. Consumed by "Copy path between
     * panes", which mirrors a host into the other pane.
     */
    // noinspection JSUnusedGlobalSymbols -- used dynamically by NetworkMountView
    export function getHostUnderCursor(): NetworkHost | null {
        // A share row reaches the palette as a `server` row instead: its host is
        // the row above it, and mirroring the host would drop the share.
        const row = rowUnderCursor()
        return row?.kind === 'server' ? row.host : null
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
        const item = itemUnderCursor()
        if (item?.kind === 'nearby_group') list.toggleNearbyGroup()
        else if (item) openRow(item.row)
    }

    /** What Enter does to a row: `openMoveFor` decides, this carries it out. */
    function openRow(row: HubRow): void {
        const move = openMoveFor(row, rows, volumes)
        if (!move) { log.warn('The hub row {server} has nowhere to open', { server: row.name }); return; }
        if (move.kind === 'host') onHostSelect?.(move.host, move.label)
        else if (move.kind === 'place') onServerSelect?.(move.row)
        // ❗ Says where to go rather than doing nothing: an S3 account is no place itself.
        else if (move.kind === 'account')
            addToast(tString('servers.hub.openAccountHint', { name: move.label }), {
                level: 'info',
                id: 'servers-open-account-hint',
            })
        else onShareViaHost?.(move.host, { share: move.share, label: move.label })
    }

    /**
     * Arrow keys and Enter, plus Space on the nearby group's header, which is a
     * disclosure: Space and Enter both toggle one. Left and Right stay the list's
     * first-and-last jump there too, as on every other row.
     */
    function handleArrowAndEnter(key: string): boolean {
        if (key === 'Enter') {
            openCursorItem()
            return true
        }
        if (key === ' ' && itemUnderCursor()?.kind === 'nearby_group') {
            list.toggleNearbyGroup()
            return true
        }
        const next = cursorAfterArrow(key, cursorIndex, totalNavigableItems)
        if (next === null) return false
        cursorIndex = next
        scrollToIndex(cursorIndex)
        return true
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
            claimKey(e)
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

        // F8: forget the saved server under the cursor. Claimed: it's `file.delete` to the dispatcher.
        if (e.key === 'F8') {
            const row = rowUnderCursor()
            if (row) {
                claimKey(e)
                void actions.forget(row)
            }
            return
        }

        // `stopPropagation` for the same reason ⌘R has it: Enter resolves to `nav.open`
        // centrally, whose handler posts Enter straight back to the focused pane, which
        // hands the network view every key. Without it, one Enter opened the row twice.
        if (handleArrowAndEnter(e.key)) {
            claimKey(e)
        }
    }

    function handleRowClick(index: number) {
        cursorIndex = index
    }

    function handleRowDoubleClick(row: HubRow) {
        openRow(row)
    }

    /** One click: a header has nothing to open, so there's no second click to wait for. */
    function handleGroupClick(index: number) {
        cursorIndex = index
        list.toggleNearbyGroup()
    }

    function handleAddRowClick() {
        cursorIndex = visibleItems.length
    }

    /** " as testuser" / " as guest", space first: a Svelte block trims the whitespace it opens with. */
    const accountSuffix = (row: HubRow) => (row.account === null ? '' : ` ${signedInAsLabel(row.account)}`)

    /** The protocol name, from the same map the volume switcher's slot reads. */
    function typeLabel(row: HubRow): string {
        return protocolLabel(row.protocol) ?? row.protocol.toUpperCase()
    }

    /**
     * Re-read the saved list and re-fetch the saved hosts' shares (user-initiated).
     *
     * ❗ A host the person never saved only has its list dropped, so opening it
     * lists afresh: listing means signing in to it, and refreshing a list of
     * servers is nobody asking to sign in to each machine on the network (#324).
     */
    function handleRefreshClick() {
        void list.refreshSaved()
        const savedHosts = list.savedHostIds()
        for (const host of list.hosts) {
            clearShareState(host.id)
            if (host.hostname && savedHosts.has(host.id)) {
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
</script>

<div
    class="servers-hub"
    class:is-focused={isFocused}
    class:without-address={!columns.address}
    class:without-status={!columns.status}
    use:useInlineSize={{ onResize: (px) => (hubWidth = px) }}
>
    <div class="header-row">
        <span class="col-name">{tString('servers.hub.colName')}</span>
        <span class="col-type">{tString('servers.hub.colType')}</span>
        <span class="col-address">{tString('servers.hub.colAddress')}</span>
        <span class="col-status">{tString('servers.hub.colStatus')}</span>
        <span class="col-last-used">{tString('servers.hub.colLastUsed')}</span>
    </div>
    <div class="row-list" bind:this={listContainer} bind:clientHeight={containerHeight}>
        {#each visibleItems as item, index (item.id)}
            {#if item.kind === 'nearby_group'}
                <!--
                    The nearby group's header. A row like the others to the cursor; the
                    button inside is what says "this opens and collapses" to a screen
                    reader, and it never takes the focus the pane keeps.
                -->
                <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
                <div
                    class="server-row nearby-group-row"
                    data-hub-row={index}
                    class:is-under-cursor={index === cursorIndex}
                    class:is-focused-and-under-cursor={isFocused && index === cursorIndex}
                    role="listitem"
                    onclick={() => {
                        handleGroupClick(index)
                    }}
                    onkeydown={() => {}}
                >
                    <button
                        type="button"
                        class="nearby-group-toggle"
                        tabindex="-1"
                        aria-expanded={item.expanded}
                        onmousedown={(e: MouseEvent) => {
                            e.preventDefault()
                        }}
                    >
                        <span class="row-icon"
                            ><Icon
                                name={item.expanded ? 'chevron-down' : 'chevron-right'}
                                size={16}
                                aria-hidden="true"
                            /></span
                        >
                        <span class="nearby-group-label"
                            >{tString('servers.hub.nearbyGroup', {
                                count: item.count,
                                countText: formatInteger(item.count),
                            })}</span
                        >
                        {#if isSearching && !item.expanded}
                            <Spinner size="sm" />
                        {/if}
                    </button>
                </div>
            {:else if item.kind === 'row'}
            {@const row = item.row}
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
            <div
                class="server-row"
                data-hub-row={index}
                class:is-under-cursor={index === cursorIndex}
                class:is-focused-and-under-cursor={isFocused && index === cursorIndex}
                role="listitem"
                onclick={() => {
                    handleRowClick(index)
                }}
                ondblclick={() => {
                    handleRowDoubleClick(row)
                }}
                oncontextmenu={(e: MouseEvent) => {
                    e.preventDefault()
                    void rowMenu?.open(row, { x: e.clientX, y: e.clientY }, null)
                }}
                onkeydown={() => {}}
            >
                <span class="col-name" class:is-place={row.kind === 'place'}>
                    <span class="row-icon"><Icon name={hubRowIcon(row)} size={16} aria-hidden="true" /></span>
                    <!-- The tooltip sits on the span that clips: an overflow check on the cell never fires. -->
                    <span class="name-text" use:tooltip={{ text: row.name + accountSuffix(row), overflowOnly: true }}
                        >{row.name}{#if row.account !== null}<span class="row-account">{accountSuffix(row)}</span
                            >{/if}</span
                    >
                </span>
                <span class="col-type">{typeLabel(row)}</span>
                <span class="col-address" use:tooltip={{ text: row.address, overflowOnly: true }}>{row.address}</span>
                <span
                    class="col-status"
                    class:needs-you={row.status === 'signed_out' || row.status === 'waiting_for_key'}
                    class:is-live={row.status === 'connected'}
                    use:tooltip={row.kind === 'server' && row.host ? getStatusTooltip(row.host) : ''}
                >
                    {tString(STATUS_TEXT_KEY[row.status])}
                </span>
                <span class="col-last-used">
                    {#if row.kind === 'place'}
                        <!-- The server row above says when it was last used. -->
                    {:else if lastUsedSeconds(row) === null}
                        <span class="never-used">{tString('servers.hub.neverUsed')}</span>
                    {:else}
                        <DateLabel modifiedAt={lastUsedSeconds(row)} />
                    {/if}
                </span>
            </div>
            {/if}
        {/each}

        <!-- The search is for nearby servers, so a collapsed group says it on its own header. -->
        {#if isSearching && (!nearbyGroup || list.nearbyExpanded)}
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

        {#if list.isKnown && !isSearching && rows.length === 0}
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
        <ServersHubStatusBar {serverCount} onRefresh={handleRefreshClick} />
    {/if}
</div>

<ServersHubRowMenu bind:this={rowMenu} {actions} />

<style>
    /* ONE grid for the header and every row (each a `subgrid`), so a column is as wide as
       its widest cell: Type, Address, Status, and Last used fit their content, and Name
       takes the rest. The name track keeps a floor, since the content-sized ones are
       maximized first and would otherwise squeeze it to nothing in a narrow pane.
       Address stops at 20%: a WebDAV URL is long, and it clips before a name does.
       Last used never clips (a cut-off date misreads), so it has no overflow of its own. */
    .servers-hub {
        display: grid;
        grid-template-columns: minmax(min(12em, 40%), 1fr) auto fit-content(20%) auto auto;
        grid-template-rows: auto minmax(0, 1fr) auto;
        column-gap: var(--spacing-lg);
        height: 100%;
        font-size: var(--font-size-sm);
        font-family: var(--font-system), sans-serif;
    }

    .header-row,
    .row-list,
    .server-row {
        grid-column: 1 / -1;
        display: grid;
        grid-template-columns: subgrid;
    }

    /* Anything in the list that isn't a row (the add row's label, the nearby group's
       header, the searching and discovery lines, the empty state) spans every column. */
    .row-list > :not(.server-row),
    .add-row .col-name,
    .nearby-group-toggle {
        grid-column: 1 / -1;
    }

    /* The nearby group's header reads as a label over its rows, quieter than a server's name. */
    .nearby-group-row {
        color: var(--color-text-secondary);
    }

    /* The header's button is the row's whole content and looks like none: the row
       carries the cursor, the button only the disclosure state. */
    .nearby-group-toggle {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        min-width: 0;
        padding: 0;
        border: none;
        background: none;
        font: inherit;
        color: inherit;
        text-align: left;
        cursor: default;
    }

    .nearby-group-label {
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .header-row {
        padding: var(--spacing-xs) var(--spacing-sm);
        background-color: var(--color-bg-secondary);
        border-bottom: 1px solid var(--color-border-strong);
        font-weight: 500;
        color: var(--color-text-secondary);
        white-space: nowrap;
    }

    .row-list {
        align-content: start;
        overflow-y: auto;
    }

    .server-row {
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
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .col-type {
        color: var(--color-text-secondary);
        white-space: nowrap;
    }

    .col-address {
        color: var(--color-text-secondary);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    /* A block, ❌ not flex: `text-overflow` doesn't reach a flex container's text, so a
       squeezed status clipped mid-word ("Found ne") instead of ending in an ellipsis. */
    .col-status {
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
        color: var(--color-text-tertiary);
        white-space: nowrap;
    }

    .never-used {
        color: var(--color-text-tertiary);
    }

    /* A share sits one icon plus the gap in. ❗ On the ICON: padding on the flex cell grew
       its base size and pushed Type, Address, and Status right on every share row. */
    .col-name.is-place .row-icon {
        margin-left: var(--spacing-xl);
    }

    /* The name and its account clip as one, with an ellipsis, short of the Type column. */
    .name-text {
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        padding-right: var(--spacing-sm);
    }

    .row-account {
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

    .header-row > span {
        overflow: hidden;
        text-overflow: ellipsis;
    }

    /* A narrow pane drops columns deliberately, Address first, then Status (`hubColumnsAt`),
       rather than collapsing them to zero width. */
    .servers-hub.without-address {
        grid-template-columns: minmax(min(10em, 45%), 1fr) auto auto auto;
    }

    .servers-hub.without-address.without-status {
        grid-template-columns: minmax(0, 1fr) auto auto;
    }

    .without-address .col-address,
    .without-status .col-status {
        display: none;
    }
</style>
