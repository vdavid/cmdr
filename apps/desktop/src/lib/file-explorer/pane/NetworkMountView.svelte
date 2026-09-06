<script lang="ts">
    import { tick } from 'svelte'
    import type { MountError, NetworkHost, NetworkLoginSubmitPayload, PlacesAccount, ShareInfo } from '../types'
    import {
        mountNetworkShare,
        resolvePathVolume,
        saveSmbCredentials,
        updateLeftPaneState,
        updateRightPaneState,
    } from '$lib/tauri-commands'
    import { getMountTimeoutMs } from '$lib/settings/network-settings'
    import { getAppLogger } from '$lib/logging/logger'
    import type { ServersHubAPI, PlacesBrowserAPI, NetworkCursorEntry } from './types'
    import ServersHub from '../network/ServersHub.svelte'
    import type { HubRow } from '../network/servers-hub-rows'
    import PlacesBrowser from '../network/PlacesBrowser.svelte'
    import NetworkLoginForm from '../network/NetworkLoginForm.svelte'
    import ConnectToServerDialog from '../network/ConnectToServerDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { eventMatchesCommand } from '$lib/shortcuts'
    import type { VolumeChangePayload } from './types'

    const log = getAppLogger('fileExplorer')

    /**
     * The hub row's own name, for the `volumeName` this pane pushes to MCP.
     *
     * ❗ One source with the switcher's label AND with Rust's
     * `volume_listing::SERVERS_VOLUME_NAME`: `mcp/executor/nav.rs` waits for this
     * pushed name to equal that const before it reports a `select_volume` done, so
     * a second spelling here is a 30 s timeout in the MCP tool.
     */
    function serversVolumeName(): string {
        return tString('fileExplorer.navigation.networkVolume')
    }

    interface Props {
        paneId?: 'left' | 'right'
        isFocused?: boolean
        /** Externally controlled network host (for history navigation) */
        initialNetworkHost?: NetworkHost | null
        /**
         * Share name to auto-mount on this host. Used by "Copy path between
         * panes" when the source pane has the cursor on a share. Reactive:
         * a new non-empty value re-arms the auto-mount gate.
         */
        initialAutoMountShare?: string | undefined
        onVolumeChange?: (change: VolumeChangePayload) => void
        onNetworkHostChange?: (host: NetworkHost | null) => void
    }

    const {
        paneId,
        isFocused = false,
        initialNetworkHost = null,
        initialAutoMountShare,
        onVolumeChange,
        onNetworkHostChange,
    }: Props = $props()

    // Two-way synced state: parent drives it via initialNetworkHost (history nav),
    // child drives it locally (host selection, mount success, back). Can't be $derived.
    // eslint-disable-next-line svelte/prefer-writable-derived -- bidirectional sync with local overrides
    let currentNetworkHost = $state<NetworkHost | null>(initialNetworkHost)

    /** The open host as an account, which is what `PlacesBrowser` takes. */
    const currentAccount = $derived<PlacesAccount | null>(
        currentNetworkHost ? { protocol: 'smb', host: currentNetworkHost } : null,
    )

    // Connect-to-server dialog
    let showConnectDialog = $state(false)
    let autoMountShare = $state<string | undefined>(initialAutoMountShare)

    // Mounting state
    let isMounting = $state(false)
    let mountError = $state<MountError | null>(null)

    // Track last mount attempt for retry
    let lastMountAttempt = $state<{
        share: ShareInfo
        credentials: { username: string; password: string } | null
    } | null>(null)

    // Auth-class mount failures render the login form instead of the dead-end error
    // pane: the user can fix the credentials right there, and the retry mounts with
    // what they entered. Non-auth failures (unreachable, timeout, ...) keep the error
    // pane with "Try again".
    const showMountLoginForm = $derived(
        (mountError?.type === 'auth_failed' || mountError?.type === 'auth_required') && currentNetworkHost !== null,
    )

    // Component refs for keyboard navigation
    let serversHubRef: ServersHubAPI | undefined = $state()
    let placesBrowserRef: PlacesBrowserAPI | undefined = $state()

    // Whether the last state we pushed carried a `mountError`, so the clear below
    // fires exactly once. Deliberately not `$state`: writing it inside the effect
    // that reads it would re-run the effect forever.
    let mirroredMountError = false

    /**
     * Mirrors a mount that didn't go through into `cmdr://state` — the error pane and
     * the login form an auth failure routes to alike.
     *
     * Without it a failed mount is invisible there: the pane's `path` and `files`
     * still describe the share list either view replaced, so a reader sees a pane
     * that simply didn't move and no reason anywhere in the resource.
     *
     * The clear is explicit, ❌ not left to whichever view comes next. `PlacesBrowser`
     * only pushes once it has a share list, so a host that has since gone quiet
     * pushes nothing at all — and a `mountError` outliving its pane misleads a reader
     * worse than the silence it replaced.
     */
    $effect(() => {
        if (!paneId) return
        const error = mountError
        const share = lastMountAttempt?.share.name
        if (!error || share === undefined) {
            if (!mirroredMountError) return
            mirroredMountError = false
        } else {
            mirroredMountError = true
        }
        const update = paneId === 'left' ? updateLeftPaneState : updateRightPaneState
        void update({
            path: currentNetworkHost ? `smb://${currentNetworkHost.ipAddress ?? currentNetworkHost.name}/` : 'smb://',
            volumeId: 'network',
            volumeName: currentNetworkHost ? `${serversVolumeName()} > ${currentNetworkHost.name}` : serversVolumeName(),
            files: [],
            cursorIndex: 0,
            viewMode: 'full',
            mountError: error && share !== undefined ? { share, message: error.message } : null,
        }).catch(() => {
            // MCP mirroring is optional; a failed push must not touch the UI.
        })
    })

    // Sync when parent changes the prop (for example, history navigation)
    $effect(() => {
        currentNetworkHost = initialNetworkHost
    })

    // Push a new auto-mount target down into PlacesBrowser. Used by "Copy path
    // between panes" with cursor on a share. PlacesBrowser dedupes repeat values,
    // so re-passing the same name is harmless.
    $effect(() => {
        if (initialAutoMountShare && initialAutoMountShare !== autoMountShare) {
            autoMountShare = initialAutoMountShare
        }
    })

    function handleNetworkHostSelect(host: NetworkHost) {
        currentNetworkHost = host
        onNetworkHostChange?.(host)
    }

    /**
     * Enter on a one-place server in the hub: take the pane to its place.
     *
     * ❗ The pane does the dialing, not the hub. Landing on a `saved` volume is
     * what `place-connect` watches for, so the connecting view and its Cancel
     * render where every other wait does.
     */
    function handleServerSelect(row: HubRow) {
        const path = row.saved?.places[0]?.appRoot
        if (!row.volumeId || !path) {
            log.warn('The hub row {name} has no place to open', { name: row.name })
            return
        }
        onVolumeChange?.({ volumeId: row.volumeId, volumePath: path, targetPath: path })
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
    /** Returns the hostname or IP to connect to (without port; port is passed separately). */
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
            // Clear current network host first (also propagate up so the parent
            // pane's state doesn't hold onto a stale host; otherwise the next
            // time the user switches back to Network, PlacesBrowser for the old
            // host would render instead of the ServersHub list).
            currentNetworkHost = null
            lastMountAttempt = null
            onNetworkHostChange?.(null)

            // The mount path is typically /Volumes/<ShareName>
            const mountPath = result.mountPath

            // Find the actual volume for the mounted path
            // This ensures proper breadcrumb display and volume context
            // (No need to refresh volume list; the mount event triggers a volumes-changed broadcast)
            const { volume: mountedVolume } = await resolvePathVolume(mountPath)

            if (mountedVolume) {
                // Use the real volume ID and path from the system
                onVolumeChange?.({ volumeId: mountedVolume.id, volumePath: mountedVolume.path, targetPath: mountPath })
            } else {
                // Fallback: use mount path as both volume path and target
                // This can happen if the volume list hasn't refreshed yet
                onVolumeChange?.({ volumeId: mountPath, volumePath: mountPath, targetPath: mountPath })
            }
        } catch (e) {
            mountError = e as MountError
            // WARN, not ERROR: the pane below renders this failure with a retry, so
            // it's an outcome the person is looking at, not a defect to report. At
            // error level every unreachable NAS captured a backtrace and a state
            // snapshot into the error-report bundle.
            log.warn('Mount of {share} on {host} did not go through: {error}', {
                share: share.name,
                host: currentNetworkHost?.name ?? 'unknown host',
                error: mountError,
            })
        } finally {
            isMounting = false
        }
    }

    function handleMountRetry() {
        if (lastMountAttempt) {
            void handleShareSelect(lastMountAttempt.share, lastMountAttempt.credentials)
        }
    }

    /** Sync form-submit wrapper around the async retry (the form's `onConnect` returns void). */
    function handleMountLoginConnect({ username, password, rememberInKeychain }: NetworkLoginSubmitPayload) {
        void retryMountWithCredentials(username, password, rememberInKeychain)
    }

    /** Retries the failed mount with credentials from the login form; saves them on success. */
    async function retryMountWithCredentials(
        username: string | null,
        password: string | null,
        rememberInKeychain: boolean,
    ) {
        if (!lastMountAttempt || !currentNetworkHost) return
        const hostName = currentNetworkHost.name
        const credentials = username === null ? null : { username, password: password ?? '' }

        await handleShareSelect(lastMountAttempt.share, credentials)

        // handleShareSelect cleared mountError on success; persist the working credentials.
        if (mountError === null && credentials && rememberInKeychain && password !== null) {
            try {
                await saveSmbCredentials(hostName, null, credentials.username, password)
            } catch (e) {
                log.warn('Mount succeeded but saving credentials failed: {error}', { error: e })
            }
        }
    }

    export function handleKeyDown(e: KeyboardEvent) {
        if (showMountLoginForm) {
            // The form's own inputs handle their keys; Escape returns to the share list.
            if (e.key === 'Escape') {
                e.preventDefault()
                handleMountErrorBack()
            }
            return
        }
        if (mountError) {
            // The error pane replaced the share list, so neither browser is mounted and
            // the delegation below would land on nothing: without this branch the whole
            // keyboard went dead and the two buttons were the only way out.
            // `share.back` (Escape / Backspace / ⌘↑) does what "Back" does.
            //
            // `stopPropagation` keeps it to ONE step: `⌘↑` is also `nav.parent` in the
            // document dispatch map, which would step out of the host as well and skip
            // the share list entirely (the same hazard `pane-key-router.ts`'s
            // `handleOpenOrParentKey` stops for).
            if (eventMatchesCommand(e, 'share.back')) {
                e.preventDefault()
                e.stopPropagation()
                handleMountErrorBack()
            }
            return
        }
        if (currentNetworkHost) {
            placesBrowserRef?.handleKeyDown(e)
        } else {
            serversHubRef?.handleKeyDown(e)
        }
    }

    /** Move cursor to a specific index (used by MCP move_cursor tool). */
    export function setCursorIndex(index: number) {
        if (currentNetworkHost) {
            placesBrowserRef?.setCursorIndex(index)
        } else {
            serversHubRef?.setCursorIndex(index)
        }
    }

    /**
     * Rows the cursor can sit on right now. `0` while a mount runs or its failure is
     * on screen: neither browser is mounted then, so a cursor move would land nowhere
     * and the MCP tool would report a move that never happened.
     */
    export function getItemCount(): number {
        if (isMounting || mountError) return 0
        if (currentNetworkHost) return placesBrowserRef?.getItemCount() ?? 0
        return serversHubRef?.getItemCount() ?? 0
    }

    /** Find an item by name, returns its index or -1. */
    export function findItemIndex(name: string): number {
        if (currentNetworkHost) {
            return placesBrowserRef?.findItemIndex(name) ?? -1
        }
        return serversHubRef?.findItemIndex(name) ?? -1
    }

    /**
     * Returns what's under the cursor in the network browser stack:
     * a host (server list), a share (share list), or `null` (connect row,
     * login form, mounting state, mount error, empty list).
     */
    // noinspection JSUnusedGlobalSymbols -- used by FilePane.getNetworkCursorEntry
    export function getNetworkCursorEntry(): NetworkCursorEntry | null {
        if (isMounting || mountError) return null
        if (currentNetworkHost) {
            const share = placesBrowserRef?.getShareUnderCursor() ?? null
            return share ? { kind: 'share', share } : null
        }
        const host = serversHubRef?.getHostUnderCursor() ?? null
        if (host) return { kind: 'host', host }
        const row = serversHubRef?.getRowUnderCursor() ?? null
        return row ? { kind: 'server', row } : null
    }

    /** Opens the host or share under the cursor — same action Enter triggers. */
    // noinspection JSUnusedGlobalSymbols -- used dynamically by FilePane / MCP
    export function openCursorItem(): void {
        if (currentNetworkHost) {
            placesBrowserRef?.openCursorItem()
        } else {
            serversHubRef?.openCursorItem()
        }
    }

    /** Refresh network hosts (used by ⌘R shortcut). */
    export function refreshNetworkHosts() {
        serversHubRef?.refresh()
    }

    export function setNetworkHost(host: NetworkHost | null) {
        currentNetworkHost = host
        mountError = null
        lastMountAttempt = null
    }
</script>

{#if isMounting}
    <div class="mounting-state">
        <Spinner size="md" />
        <span class="mounting-text"
            >{tString('fileExplorer.networkMount.mounting', {
                target: currentNetworkHost?.name ?? tString('fileExplorer.networkMount.shareFallback'),
            })}</span
        >
    </div>
{:else if showMountLoginForm && currentNetworkHost}
    <NetworkLoginForm
        host={currentNetworkHost}
        shareName={lastMountAttempt?.share.name}
        authMode="creds_required"
        initialUsername={lastMountAttempt?.credentials?.username}
        errorMessage={mountError?.message}
        onConnect={handleMountLoginConnect}
        onCancel={handleMountErrorBack}
    />
{:else if mountError}
    <div class="mount-error-state">
        <div class="error-icon">&#x274C;</div>
        <div class="error-title">{tString('fileExplorer.networkMount.mountFailedTitle')}</div>
        <div class="error-message">{mountError.message}</div>
        <div class="error-actions">
            <Button variant="secondary" onclick={handleMountRetry}>{tString('fileExplorer.networkMount.tryAgain')}</Button>
            <Button variant="secondary" onclick={handleMountErrorBack}>{tString('fileExplorer.networkMount.back')}</Button>
        </div>
    </div>
{:else if currentAccount}
    <PlacesBrowser
        bind:this={placesBrowserRef}
        account={currentAccount}
        {paneId}
        {isFocused}
        {autoMountShare}
        onShareSelect={handleShareSelect}
        onBack={handleNetworkBack}
    />
{:else}
    <ServersHub
        bind:this={serversHubRef}
        {paneId}
        {isFocused}
        onHostSelect={handleNetworkHostSelect}
        onServerSelect={handleServerSelect}
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
