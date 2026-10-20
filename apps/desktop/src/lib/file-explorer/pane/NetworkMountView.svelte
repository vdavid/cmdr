<script lang="ts">
    import { tick } from 'svelte'
    import { openAddServerSheet } from '#lib/servers/open-sign-in.js'
    import { focusHome } from '#lib/ui/focus-restore.js'
    import type { MountError, NetworkHost, PlacesAccount, ShareInfo } from '../types'
    import {
        cancelServerConnect,
        mountNetworkShare,
        newServerAttemptId,
        resolvePathVolume,
        saveSmbCredentials,
        setSmbAccountPreference,
        updateLeftPaneState,
        updateRightPaneState,
    } from '#lib/tauri-commands/index.js'
    import { getMountTimeoutMs } from '#lib/settings/network-settings.js'
    import { getVolumes } from '#lib/stores/volume-store.svelte.js'
    import { pathForPickedVolume } from '../navigation/picked-volume-path'
    import { getAppLogger } from '#lib/logging/logger.js'
    import type { ServersHubAPI, PlacesBrowserAPI, NetworkCursorEntry } from './types'
    import ServersHub from '../network/ServersHub.svelte'
    import type { HubRow } from '../network/servers-hub-rows'
    import PlacesBrowser from '../network/PlacesBrowser.svelte'
    import { holdDiscoveryForServersView } from '../network/network-store.svelte'
    import { isMountSignInQuestion, openSmbSignInSheet, refusalForMountError } from '../network/smb-sign-in'
    import { asMountError } from '../network/mount-error'
    import { renderMountError } from '../network/mount-error-messages'
    import type { SignInAttemptOutcome } from '#lib/servers/sign-in-contract.js'
    import Button from '#lib/ui/Button.svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import Spinner from '#lib/ui/Spinner.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { eventMatchesCommand } from '#lib/shortcuts/index.js'
    import { claimKey } from '#lib/shortcuts/claim-key.js'
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
        /**
         * The queued share is spent: the visit it was queued with ended (a mount
         * went through, or the person left the host). The parent clears its copy,
         * so the same share name can be queued again later.
         */
        onAutoMountConsumed?: () => void
        onVolumeChange?: (change: VolumeChangePayload) => void
        onNetworkHostChange?: (host: NetworkHost | null) => void
    }

    const {
        paneId,
        isFocused = false,
        initialNetworkHost = null,
        initialAutoMountShare,
        onAutoMountConsumed,
        onVolumeChange,
        onNetworkHostChange,
    }: Props = $props()

    // Two-way synced state: parent drives it via initialNetworkHost (history nav),
    // child drives it locally (host selection, mount success, back). Can't be $derived.
    // eslint-disable-next-line svelte/prefer-writable-derived -- bidirectional sync with local overrides
    let currentNetworkHost = $state<NetworkHost | null>(initialNetworkHost)
    /** What the hub called the open host (a name the person gave it), for the share list's words. */
    let currentHostLabel = $state<string | undefined>(undefined)

    /** The open host as an account, which is what `PlacesBrowser` takes. */
    const currentAccount = $derived<PlacesAccount | null>(
        currentNetworkHost ? { protocol: 'smb', host: currentNetworkHost, label: currentHostLabel } : null,
    )

    /**
     * The share to mount as soon as this visit's share list is ready.
     *
     * ❗ **A one-shot for the host it was queued with**, ❌ never a standing
     * wish. `PlacesBrowser` fires it once per INSTANCE, and a new instance
     * mounts on every host visit, so a value that outlived its visit mounted a
     * share nobody picked each time the person opened that host again, and
     * toasted "not found" on every other host (cmdr-reports#7: "Cmdr tried to
     * open the share on that server"). `retireAutoMount` ends it on every exit.
     */
    let autoMountShare = $state<string | undefined>(initialAutoMountShare)
    /** The parent's value this view last adopted, so the same one isn't adopted twice. */
    let adoptedAutoMount: string | undefined = initialAutoMountShare

    // Mounting state
    let isMounting = $state(false)
    let mountError = $state<MountError | null>(null)

    /**
     * How long a mount runs before the pane says it's still waiting. Matches the
     * backend's `netfs_wedge::SUSPECT_AFTER`: a healthy mount takes a second or two.
     */
    const MOUNT_SLOW_AFTER_MS = 10_000
    /** The running mount's id, which Cancel calls it off by. `null` when none runs. */
    let mountAttemptId: string | null = null
    /** Whether the person pressed Cancel on the running mount, so its `cancelled` answer goes back quietly. */
    let mountCancelRequested = false
    /** Set once the running mount outlasted `MOUNT_SLOW_AFTER_MS`. */
    let mountingSlow = $state(false)

    // Track last mount attempt for retry
    let lastMountAttempt = $state<{
        share: ShareInfo
        credentials: { username: string; password: string } | null
    } | null>(null)

    /**
     * True while the sign-in sheet is up over this pane. The error pane behind it
     * still holds the mount that didn't go through, which is what the MCP mirror
     * reads and what cancelling lands back on.
     */
    let signingIn = $state(false)

    // ❗ Credential-class mount failures ASK (the one sign-in sheet) instead of
    // dead-ending in the error pane: "Try again" there replayed the identical
    // credentials, which is the "Naspolya dead end". That covers a signed-in
    // account the share turned away too, since another account is the fix.
    // `isMountSignInQuestion` and the refusal vocabulary live in `../network/smb-sign-in`.

    // Hosts arrive and leave live while this view is on screen: the backend browses mDNS
    // exactly as long as some Servers view holds it. The release is the effect's cleanup.
    $effect(() => holdDiscoveryForServersView())

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
            mountError:
                error && share !== undefined
                    ? { share, reason: error.type, message: renderMountError(error, currentNetworkHost?.name) }
                    : null,
        }).catch(() => {
            // MCP mirroring is optional; a failed push must not touch the UI.
        })
    })

    // Sync when parent changes the prop (for example, history navigation)
    $effect(() => {
        currentNetworkHost = initialNetworkHost
    })

    // Push a new auto-mount target down into PlacesBrowser. Used by "Copy path
    // between panes" with cursor on a share. ❗ Compared with what was last
    // ADOPTED, ❌ not with the local value: a retired target is `undefined` here
    // while the parent may still hold the name, and re-adopting it is the
    // stale auto-mount this exists to prevent.
    $effect(() => {
        if (initialAutoMountShare && initialAutoMountShare !== adoptedAutoMount) {
            adoptedAutoMount = initialAutoMountShare
            autoMountShare = initialAutoMountShare
        } else if (!initialAutoMountShare) {
            adoptedAutoMount = undefined
        }
    })

    /** Ends the queued auto-mount, here and in the parent. See `autoMountShare`. */
    function retireAutoMount() {
        if (autoMountShare === undefined && adoptedAutoMount === undefined) return
        autoMountShare = undefined
        onAutoMountConsumed?.()
    }

    /**
     * A host picked from the servers list: its shares, ❌ never a share mounted
     * on the person's behalf. They picked a HOST.
     */
    function handleNetworkHostSelect(host: NetworkHost, label?: string) {
        retireAutoMount()
        currentHostLabel = label
        currentNetworkHost = host
        onNetworkHostChange?.(host)
    }

    /**
     * Enter on a one-place server in the hub: take the pane to its place, a saved
     * one on its start folder (`pathForPickedVolume`).
     *
     * ❗ The pane does the dialing, not the hub. Landing on a `saved` volume is
     * what `place-connect` watches for, so the connecting view and its Cancel
     * render where every other wait does.
     */
    function handleServerSelect(row: HubRow) {
        const volume = getVolumes().find((v) => v.id === row.volumeId)
        const root = volume?.path ?? row.place?.appRoot ?? row.saved?.places[0]?.appRoot
        if (!row.volumeId || !root) {
            log.warn('The hub row {server} has no place to open', { server: row.name })
            return
        }
        const targetPath = volume ? pathForPickedVolume(volume) : root
        onVolumeChange?.({ volumeId: row.volumeId, volumePath: root, targetPath })
    }

    /**
     * The Add row opens the ONE sign-in sheet. An SMB address lands back here as
     * a hand-off: its connect is a share MOUNT rather than a session, so the
     * host is injected and this view opens its places list.
     */
    async function openAddServer() {
        await openAddServerSheet({
            onSmbHandOff: (handOff) => {
                retireAutoMount()
                currentNetworkHost = handOff.host
                onNetworkHostChange?.(handOff.host)
                // The one share the ADDRESS named, for this visit only.
                if (handOff.sharePath) autoMountShare = handOff.sharePath
            },
            // The hub's own pane goes to the place it just added, the way picking
            // its row would take it there.
            onConnected: ({ volumeId, root }) => {
                onVolumeChange?.({ volumeId, volumePath: root, targetPath: root })
            },
            // "Add" saved it without opening: the new row, selected, is the proof.
            onAdded: ({ serverId }) => {
                serversHubRef?.selectServer(serverId)
            },
        })
        await tick()
        // Focus goes back to the explorer so keyboard navigation resumes.
        focusHome()
    }

    /**
     * A saved share no mount went through yet: its host's share list, mounting
     * that one share for this visit (the account comes from the host's typed
     * one, which keeps the listing from answering as guest).
     */
    function handleShareViaHost(host: NetworkHost, { share, label }: { share: string; label: string }) {
        handleNetworkHostSelect(host, label)
        autoMountShare = share
    }

    function handleNetworkBack() {
        const cameFrom = currentNetworkHost
        retireAutoMount()
        currentNetworkHost = null
        mountError = null
        lastMountAttempt = null
        onNetworkHostChange?.(null)
        // The cursor goes back to the host the person came from, by its identity (a saved
        // row carries the host's id, a discovered one its `host`), ❌ not the first row.
        if (cameFrom) void tick().then(() => serversHubRef?.selectServer(cameFrom.id))
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

    /**
     * Mounts `share` and takes the pane to it. Answers the failure rather than
     * throwing, so a caller inside the sign-in sheet can offer a retry and a
     * caller outside it can render the error pane.
     */
    async function mountShare(
        share: ShareInfo,
        credentials: { username: string; password: string } | null,
    ): Promise<MountError | null> {
        if (!currentNetworkHost) return null

        // Store for retry
        lastMountAttempt = { share, credentials }

        isMounting = true
        mountError = null
        const server = resolveServerAddress(currentNetworkHost)
        const { attemptId, finish } = startMountAttempt()

        try {
            // Use provided credentials if available
            // The host's own name rides along: a mount that goes through is saved
            // as a share place under the host the person knows (the Servers list).
            const result = await mountNetworkShare(
                server,
                share.name,
                credentials?.username ?? null,
                credentials?.password ?? null,
                currentNetworkHost.port,
                getMountTimeoutMs(),
                currentNetworkHost.name,
                attemptId,
            )

            // Navigate to the mounted share
            // Clear current network host first (also propagate up so the parent
            // pane's state doesn't hold onto a stale host; otherwise the next
            // time the user switches back to Network, PlacesBrowser for the old
            // host would render instead of the ServersHub list).
            currentNetworkHost = null
            lastMountAttempt = null
            retireAutoMount()
            onNetworkHostChange?.(null)

            // The mount path is typically /Volumes/<ShareName>
            await landOnMount(result.mountPath)
            return null
        } catch (e) {
            const refusal = asMountError(e)
            // The person called it off: back to the share list, ❌ no error pane. A cancel
            // after a long wait on a server that answers comes back `sign_in_helper_stuck`
            // instead, and that one the pane explains.
            if (mountCancelRequested && refusal?.type === 'cancelled') return null
            // A value that isn't a typed refusal means the IPC call itself broke down,
            // which the catch-all words honestly; its text goes to the log below.
            mountError = refusal ?? { type: 'unexpected', server, share: share.name, detail: String(e) }
            // WARN, not ERROR: the pane below renders this failure with a retry, so
            // it's an outcome the person is looking at, not a defect to report. At
            // error level every unreachable NAS captured a backtrace and a state
            // snapshot into the error-report bundle.
            log.warn('Mount of {share} on {host} did not go through: {error}', {
                share: share.name,
                host: currentNetworkHost?.name ?? 'unknown host',
                // Pre-stringified: the log bridge renders a property with `String()`.
                error: JSON.stringify(mountError),
            })
            return mountError
        } finally {
            finish()
            isMounting = false
        }
    }

    /** Takes the pane to the share mounted at `mountPath`. */
    async function landOnMount(mountPath: string) {
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
    }

    /**
     * Files a new mount as the running one: its id for Cancel, and the timer that
     * says it's still waiting. `finish` undoes both once the mount answers.
     */
    function startMountAttempt(): { attemptId: string; finish: () => void } {
        const attemptId = newServerAttemptId()
        mountAttemptId = attemptId
        mountCancelRequested = false
        mountingSlow = false
        const slowTimer = setTimeout(() => {
            if (mountAttemptId === attemptId) mountingSlow = true
        }, MOUNT_SLOW_AFTER_MS)
        const finish = () => {
            clearTimeout(slowTimer)
            if (mountAttemptId === attemptId) mountAttemptId = null
            mountingSlow = false
        }
        return { attemptId, finish }
    }

    /**
     * Calls off the running mount. The backend stops WAITING and answers; the NetFS
     * call itself can't be taken back (`src-tauri/src/network/netfs_wedge.rs`).
     */
    function cancelMount() {
        if (!mountAttemptId) return
        mountCancelRequested = true
        void cancelServerConnect(mountAttemptId).catch((e: unknown) => {
            log.warn('Calling off the mount did not go through: {error}', { error: e })
        })
    }

    /** Mounts a share the user picked, and asks for a credential if that is what's missing. */
    async function handleShareSelect(share: ShareInfo, credentials: { username: string; password: string } | null) {
        const error = await mountShare(share, credentials)
        if (error && isMountSignInQuestion(error)) await askForMountCredentials(share, error)
    }

    /**
     * Hands a mount's credential question to the one sign-in sheet, retrying the
     * mount inside it for as long as the user keeps answering.
     *
     * ❗ Cancelling goes back to the SHARE LIST (`handleMountErrorBack`), which is
     * where the user was: they picked a share, and the one they wanted is still on
     * screen behind the sheet.
     */
    async function askForMountCredentials(share: ShareInfo, firstError: MountError) {
        const host = currentNetworkHost
        // ❗ One ask at a time. A second `openSignInSheet` closes the first as
        // `cancelled`, and this caller reads that as "the user said no" and clears
        // the failure behind a sheet that is still up.
        if (!host || signingIn) return
        signingIn = true
        try {
            const result = await openSmbSignInSheet({
                host,
                label: currentHostLabel,
                shareName: share.name,
                // ❗ No guest option: an unauthenticated mount is exactly what just
                // came back refused, so offering it again would be inert.
                guestAllowed: false,
                initialUsername: lastMountAttempt?.credentials?.username,
                refusal: refusalForMountError(firstError),
                attempt: (answer) => remountWith(share, host.name, answer),
            })
            if (result.kind === 'cancelled') handleMountErrorBack()
        } finally {
            signingIn = false
        }
    }

    /**
     * One mount round-trip with what the user offered, remembering the credential
     * ❗ only once the mount has actually gone through.
     */
    async function remountWith(
        share: ShareInfo,
        hostName: string,
        answer: { username: string | null; password: string | null; remember: boolean },
    ): Promise<SignInAttemptOutcome> {
        const credentials = answer.username === null ? null : { username: answer.username, password: answer.password ?? '' }
        const error = await mountShare(share, credentials)
        if (!error) {
            // ❗ The account that got in becomes the host's account, as "Sign in as…" does, so
            // the next visit lists and mounts as it. Without this, a share guests can list but
            // not open asked for the password on every mount (cmdr-reports#27).
            if (credentials) void setSmbAccountPreference(hostName, credentials.username).catch(() => {})
            if (credentials && answer.remember && answer.password !== null) {
                try {
                    await saveSmbCredentials(hostName, null, credentials.username, answer.password)
                } catch (e) {
                    log.warn('Mount succeeded but saving credentials failed: {error}', { error: e })
                }
            }
            return { kind: 'handed_off' }
        }
        // ❗ Only a refusal another credential can answer keeps the sheet open.
        // Anything else is about the SHARE or the server, and the pane's error
        // state is what has the words and the retry.
        if (!isMountSignInQuestion(error)) return { kind: 'handed_off' }
        return { kind: 'refused', refusal: refusalForMountError(error) }
    }

    /** The calm sign-in pane's button: asks again, for the share that wanted an account. */
    function handleMountSignIn() {
        if (lastMountAttempt && mountError) void askForMountCredentials(lastMountAttempt.share, mountError)
    }

    function handleMountRetry() {
        if (lastMountAttempt) {
            void handleShareSelect(lastMountAttempt.share, lastMountAttempt.credentials)
        }
    }

    export function handleKeyDown(e: KeyboardEvent) {
        // The sign-in sheet is modal and owns the keyboard while it is up.
        if (signingIn) return
        if (isMounting) {
            // Neither browser is mounted while a mount runs; `share.back` is Cancel.
            if (eventMatchesCommand(e, 'share.back')) {
                claimKey(e)
                cancelMount()
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
                claimKey(e)
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
        const row = serversHubRef?.getRowUnderCursor() ?? null
        const host = serversHubRef?.getHostUnderCursor() ?? null
        if (host && row) return { kind: 'host', host, row }
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

    /** ⌃⏎: the servers hub's cursor row menu. A host's places list has no row menu, so nothing there. */
    // noinspection JSUnusedGlobalSymbols -- used dynamically by FilePane
    export async function openContextMenuAtCursor(): Promise<void> {
        if (currentNetworkHost) return
        await serversHubRef?.openContextMenuAtCursor()
    }

    /** Refresh network hosts (used by ⌘R shortcut). */
    export function refreshNetworkHosts() {
        serversHubRef?.refresh()
    }

    export function setNetworkHost(host: NetworkHost | null) {
        if (host?.id !== currentNetworkHost?.id) {
            retireAutoMount()
            currentHostLabel = undefined
        }
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
        {#if mountingSlow}
            <span class="mounting-hint">{tString('fileExplorer.networkMount.mountingSlow')}</span>
        {/if}
        <!-- ❗ Every wait with a process behind it has a cancel: a NetFS mount can hang
             behind a stuck macOS sign-in helper for as long as the timeout allows (#604). -->
        <Button variant="secondary" size="mini" onclick={cancelMount}>
            {tString('fileExplorer.networkMount.cancel')}
        </Button>
    </div>
{:else if mountError?.type === 'auth_required'}
    <!-- ❗ A share that wants an account is a normal step, not a failure: no ❌, no
         "Couldn't mount share", and no Try again, which without an account gets
         the same answer (cmdr-reports#10). The sheet sits on top of this. -->
    <div class="mount-sign-in-state">
        <div class="sign-in-icon"><Icon name="lock" size={32} aria-hidden="true" /></div>
        <div class="sign-in-title">{tString('fileExplorer.networkMount.signInTitle', { share: mountError.share })}</div>
        <div class="sign-in-message">
            {tString('fileExplorer.networkMount.signInMessage', {
                server: currentHostLabel ?? currentNetworkHost?.name ?? mountError.server,
            })}
        </div>
        <div class="error-actions">
            <Button variant="primary" onclick={handleMountSignIn}>{tString('fileExplorer.network.signIn')}</Button>
            <Button variant="secondary" onclick={handleMountErrorBack}>{tString('fileExplorer.networkMount.back')}</Button>
        </div>
    </div>
{:else if mountError}
    <div class="mount-error-state">
        <div class="error-icon">&#x274C;</div>
        <div class="error-title">{tString('fileExplorer.networkMount.mountFailedTitle')}</div>
        <div class="error-message">{renderMountError(mountError, currentNetworkHost?.name)}</div>
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
        onShareViaHost={handleShareViaHost}
        onConnectToServer={() => void openAddServer()}
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

    .mounting-hint {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        text-align: center;
    }

    .mount-error-state,
    .mount-sign-in-state {
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

    .mount-sign-in-state .sign-in-icon {
        display: inline-flex;
        color: var(--color-text-tertiary);
    }

    .mount-error-state .error-title,
    .mount-sign-in-state .sign-in-title {
        font-size: var(--font-size-lg);
        font-weight: 500;
        color: var(--color-text-primary);
    }

    .mount-error-state .error-message,
    .mount-sign-in-state .sign-in-message {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        text-align: center;
        height: auto;
        padding: 0;
    }

    .mount-error-state .error-actions,
    .mount-sign-in-state .error-actions {
        display: flex;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-sm);
    }
</style>
