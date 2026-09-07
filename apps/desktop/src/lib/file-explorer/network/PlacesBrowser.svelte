<script lang="ts">
    /**
     * The places under one account: an SMB host's shares today, a storage
     * account's buckets later. Raises the sign-in form when the listing needs one.
     */
    import { onMount } from 'svelte'
    import Button from '$lib/ui/Button.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import CopyBox from '$lib/ui/CopyBox.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import type { AuthMode, PlacesAccount, ShareInfo, ShareListError } from '../types'
    import {
        getShareState,
        fetchShares,
        clearShareState,
        setShareState,
        setCredentialStatus,
        forgetCredentials,
    } from './network-store.svelte'
    import {
        listSharesWithCredentials,
        saveSmbCredentials,
        getSmbCredentials,
        isUsingCredentialFileFallback,
        updateKnownShare,
    } from '$lib/tauri-commands'
    import { addToast } from '$lib/ui/toast'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { tString } from '$lib/intl/messages.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import { getNetworkTimeoutMs, getShareCacheTtlMs } from '$lib/settings/network-settings'
    import { isListingAuthError, openSmbSignInSheet, refusalForShareError } from './smb-sign-in'
    import type { ConnectRefusalKind } from '$lib/servers/connect-refusals'
    import type { SignInAttemptOutcome } from '$lib/servers/sign-in-contract'
    import { handleNavigationShortcut } from '../navigation/keyboard-shortcuts'
    import { eventMatchesCommand } from '$lib/shortcuts'
    import { updateLeftPaneState, updateRightPaneState, type PaneState, type PaneFileEntry } from '$lib/tauri-commands'

    async function notifyIfUsingFileFallback(): Promise<void> {
        if (await isUsingCredentialFileFallback()) {
            addToast(tString('fileExplorer.network.share.credentialsStoredLocally'), {
                level: 'info',
                id: 'credential-file-fallback',
            })
        }
    }

    /** Row height for share list (matches Full list) */
    const SHARE_ROW_HEIGHT = 20

    interface Props {
        /** Whose places these are. One SMB arm today; see `PlacesAccount`. */
        account: PlacesAccount
        /** Which pane this browser lives in (for MCP state sync) */
        paneId?: 'left' | 'right'
        /** Whether this pane is focused */
        isFocused?: boolean
        /** Auto-mount this share name after loading (from smb://host/share URL) */
        autoMountShare?: string
        /** Callback when user selects a share, includes credentials if auth was used */
        onShareSelect?: (share: ShareInfo, credentials: { username: string; password: string } | null) => void
        /** Callback to go back to host list */
        onBack?: () => void
    }

    const { account, paneId, isFocused = false, autoMountShare, onShareSelect, onBack }: Props = $props()

    // The SMB host behind the account. Everything below still speaks SMB; the
    // account is the seam a second protocol arrives through, not a rewrite.
    const host = $derived(account.host)

    // Local state
    let shares = $state<ShareInfo[]>([])
    let authMode = $state<AuthMode>('unknown')
    let loading = $state(true)
    let error = $state<ShareListError | null>(null)
    let cursorIndex = $state(0)

    // Sorted shares for display (case-insensitive alphabetical)
    const sortedShares = $derived(
        [...shares].sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' })),
    )

    /**
     * True while the sign-in sheet is up for this host. The pane behind it keeps
     * whatever it was showing; this only stops the cursor exports from offering
     * rows nobody can reach past a modal.
     */
    let signingIn = $state(false)

    // Track authenticated credentials for mounting
    let authenticatedCredentials = $state<{ username: string; password: string } | null>(null)

    // Auto-mount tracking: track the last share we tried so the same prop value
    // doesn't re-fire, but a new value (for example via "Copy path between panes"
    // with cursor on a different share) does.
    let lastAutoMountAttempt = $state<string | undefined>(undefined)

    // Container tracking for PageUp/PageDown
    let listContainer: HTMLDivElement | undefined = $state()
    let containerHeight = $state(0)

    // Load shares on mount
    onMount(async () => {
        await loadShares()
    })

    // Sync share list to MCP when shares or cursor change
    $effect(() => {
        void sortedShares.length
        void cursorIndex
        void loading
        void syncPaneStateToMcp()
    })

    // Auto-mount a share if requested (from smb://host/share URL, or "Copy path
    // between panes" with cursor on a share). Fires once per distinct prop value.
    $effect(() => {
        const shareName = autoMountShare
        if (!shareName || shareName === lastAutoMountAttempt || loading || sortedShares.length === 0) return
        lastAutoMountAttempt = shareName

        const match = sortedShares.find(
            (s) => s.name.localeCompare(shareName, undefined, { sensitivity: 'base' }) === 0,
        )
        if (match) {
            void activateShare(match)
        } else {
            addToast(tString('fileExplorer.network.share.notFound', { shareName, hostName: host.name }), {
                level: 'warn',
            })
        }
    })

    /**
     * Activates a share (Enter, double-click, or auto-mount). When the listing reported
     * `creds_required` but we hold no credentials, route through the login form first
     * instead of firing a doomed guest mount. This combination is real: on macOS the
     * share-listing fallback (`smbutil view -N`) authenticates via the SYSTEM Keychain,
     * which Cmdr can't reuse for mounting, so the list renders fine while
     * `authenticatedCredentials` stays null.
     */
    async function activateShare(share: ShareInfo) {
        // When creds are required but none are in memory, try Cmdr's stored password
        // first. The share list often loads via the system Keychain (smbutil) without
        // ever exercising our own creds, so a working password may already be saved.
        if (authMode === 'creds_required' && !authenticatedCredentials) {
            const stored = await loadStoredCredentials()
            if (stored) {
                authenticatedCredentials = stored
            }
        }

        // Attempt the mount with whatever credentials we have (possibly none). Two
        // cases this handles without ever pre-prompting:
        //   - The share is already mounted → the backend short-circuits to the existing
        //     mount (no auth), so reaching an already-mounted share just navigates.
        //   - It genuinely needs auth we don't have → the mount fails and
        //     `NetworkMountView` surfaces the login form (its mount-failure handler),
        //     a single login surface with no dead end.
        // Don't pre-prompt here on `creds_required`: that re-prompts for shares that are
        // already mounted (the listing's `creds_required` says nothing about whether
        // THIS share is currently mounted).
        onShareSelect?.(share, authenticatedCredentials)
    }

    /** Reads Cmdr's stored credentials for this host, or null if none are saved. */
    async function loadStoredCredentials(): Promise<{ username: string; password: string } | null> {
        try {
            const creds = await getSmbCredentials(host.name, null)
            return { username: creds.username, password: creds.password }
        } catch {
            return null
        }
    }

    /** Sync share list to MCP so agents see the same data as the UI. */
    async function syncPaneStateToMcp() {
        if (!paneId) return

        try {
            const files: PaneFileEntry[] = sortedShares.map((share) => ({
                name: share.comment ? `${share.name}  comment="${share.comment}"` : share.name,
                path: `smb://${host.ipAddress ?? host.name}/${share.name}`,
                isDirectory: true,
                size: null,
                recursiveSize: null,
                modified: null,
                recursiveSizePending: null,
            }))

            const state: PaneState = {
                path: `smb://${host.ipAddress ?? host.name}/`,
                volumeId: 'network',
                volumeName: `Network > ${host.name}`,
                files,
                cursorIndex,
                viewMode: 'full',
                selectedIndices: [],
                totalFiles: sortedShares.length,
                loadedStart: 0,
                loadedEnd: sortedShares.length,
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

    async function loadShares() {
        loading = true
        error = null

        // Check if we have cached share state
        const cachedState = getShareState(host.id)
        if (cachedState?.status === 'loaded') {
            shares = cachedState.result.shares
            authMode = cachedState.result.authMode
            loading = false
            return
        }
        if (cachedState?.status === 'error') {
            if (isListingAuthError(cachedState.error)) {
                await settleListingAuth(cachedState.error)
                return
            }
            // Non-auth error (host_unreachable, timeout, etc.): the user is
            // explicitly opening the host, so retry. The initial background
            // prefetch may have run before the host was ready.
        }

        // Fetch shares
        try {
            const result = await fetchShares(host)
            shares = result.shares
            authMode = result.authMode
        } catch (e) {
            const shareError = e as ShareListError
            if (isListingAuthError(shareError)) {
                await settleListingAuth(shareError)
                return
            }
            error = shareError
        } finally {
            loading = false
        }
    }

    /**
     * A listing that needs a sign-in: try the stored credential silently first
     * (the loading indicator stays up, because nothing is being asked yet), and
     * only ask when that doesn't land.
     */
    async function settleListingAuth(shareError: ShareListError) {
        const stored = await tryStoredCredentials()
        loading = false
        if (stored) return
        error = shareError
        await askForCredentials(refusalForShareError(shareError))
    }

    /**
     * Hands the listing's credential question to the one sign-in sheet.
     *
     * ❗ Cancelling goes BACK to the host list. This sheet only ever opens
     * because the share LISTING itself needs a sign-in, so "not now" means the
     * user has no business on this host — leaving them on a list they can't read
     * would be an empty room with no exit. (Share-activation auth is the mount's
     * question, and `NetworkMountView` asks it.)
     */
    async function askForCredentials(refusal: ConnectRefusalKind) {
        // ❗ One ask at a time. A second `openSignInSheet` closes the first as
        // `cancelled`, and this caller reads that as "the user said no" and walks
        // back to the host list from under a sheet that is still up.
        if (signingIn) return
        signingIn = true
        try {
            const result = await openSmbSignInSheet({
                host,
                guestAllowed: authMode === 'guest_allowed',
                refusal,
                attempt: (answer) => listWithCredentials(answer.username, answer.password, answer.remember),
            })
            if (result.kind === 'cancelled') onBack?.()
        } finally {
            signingIn = false
        }
    }

    /** Try to use stored credentials. Returns true if shares were loaded. */
    async function tryStoredCredentials(): Promise<boolean> {
        const serverName = host.name

        // Try to get credentials directly - don't check hasSmbCredentials first
        // as that causes an extra Keychain dialog (each Keychain access = 1 dialog)
        try {
            const creds = await getSmbCredentials(serverName, null)
            // Store credentials in memory for mounting later
            authenticatedCredentials = { username: creds.username, password: creds.password }
            await listWithCredentials(creds.username, creds.password, false)
            // `listWithCredentials` never throws; it answers an outcome. Only
            // report success if shares were actually loaded.
            return shares.length > 0
        } catch {
            // No stored credentials or retrieval failed
            return false
        }
    }

    /**
     * One listing round-trip with what the user offered.
     *
     * ❗ Answers `handed_off` on success: nothing here connects a VOLUME, so the
     * sheet has nothing more to say and the share list it just loaded is the
     * result. A refusal about the credential keeps the sheet open; anything else
     * closes it onto the pane's own error state, which has the retry.
     */
    async function listWithCredentials(
        username: string | null,
        password: string | null,
        rememberInKeychain: boolean,
    ): Promise<SignInAttemptOutcome> {
        try {
            // Clear cached state to force refetch
            clearShareState(host.id)

            const result = await listSharesWithCredentials(
                host.id,
                host.hostname ?? host.name,
                host.ipAddress,
                host.port,
                username,
                password,
                getNetworkTimeoutMs(),
                getShareCacheTtlMs(),
            )

            shares = result.shares
            authMode = result.authMode
            error = null

            // Update global share state so ServersHub shows correct info
            setShareState(host.id, result)

            // Update credential status
            setCredentialStatus(host.name, username ? 'has_creds' : 'no_creds')

            // Store credentials for mounting (empty password is valid for SMB)
            if (username !== null) {
                authenticatedCredentials = { username, password: password ?? '' }
            } else {
                authenticatedCredentials = null
            }

            // Save credentials to Keychain if requested
            if (rememberInKeychain && username !== null && password !== null) {
                await saveSmbCredentials(host.name, null, username, password)
                await notifyIfUsingFileFallback()
            }

            // Update known shares store
            await updateKnownShare(
                host.name,
                '', // Server-level, not share-specific
                username ? 'credentials' : 'guest',
                authMode === 'guest_allowed' ? 'guest_or_credentials' : 'credentials_only',
                username,
            )
            return { kind: 'handed_off' }
        } catch (e) {
            const shareError = e as ShareListError
            if (shareError.type === 'auth_failed') {
                // Mark credentials as failed
                setCredentialStatus(host.name, 'failed')
            }
            const refusal = refusalForShareError(shareError)
            if (refusal === 'authentication_rejected' || refusal === 'needs_credentials') {
                return { kind: 'refused', refusal }
            }
            error = shareError
            return { kind: 'handed_off' }
        }
    }

    /** Move cursor to a specific index (used by MCP move_cursor tool). */
    export function setCursorIndex(index: number) {
        cursorIndex = Math.max(0, Math.min(index, sortedShares.length - 1))
        scrollToIndex(cursorIndex)
    }

    /** Find a share by name, returns its index or -1. */
    export function findItemIndex(name: string): number {
        return sortedShares.findIndex((s) => s.name.toLowerCase() === name.toLowerCase())
    }

    /** The shares on offer; `0` while the sign-in sheet is up or the host listed nothing. */
    // noinspection JSUnusedGlobalSymbols -- used dynamically by MCP move_cursor's range check
    export function getItemCount(): number {
        return signingIn ? 0 : sortedShares.length
    }

    /**
     * Returns the share under the cursor, or `null` when nothing valid is highlighted
     * (sign-in sheet up, empty list, out-of-range index). Consumed by the
     * "Copy path between panes" command so cursor-on-share mounts that share on the
     * target pane.
     */
    // noinspection JSUnusedGlobalSymbols -- used dynamically by NetworkMountView
    export function getShareUnderCursor(): ShareInfo | null {
        if (signingIn) return null
        if (cursorIndex < 0 || cursorIndex >= sortedShares.length) return null
        return sortedShares[cursorIndex]
    }

    /** Opens the share under the cursor — same action Enter triggers. */
    // noinspection JSUnusedGlobalSymbols -- used dynamically by NetworkMountView / MCP
    export function openCursorItem(): void {
        if (cursorIndex >= 0 && cursorIndex < sortedShares.length) {
            void activateShare(sortedShares[cursorIndex])
        }
    }

    function handleShareClick(index: number) {
        cursorIndex = index
    }

    function handleShareDoubleClick(index: number) {
        if (index >= 0 && index < sortedShares.length) {
            void activateShare(sortedShares[index])
        }
    }

    /** Scrolls to make the cursor visible */
    function scrollToIndex(index: number) {
        if (!listContainer) return
        const targetTop = index * SHARE_ROW_HEIGHT
        const targetBottom = targetTop + SHARE_ROW_HEIGHT
        const scrollTop = listContainer.scrollTop
        const viewportBottom = scrollTop + containerHeight

        if (targetTop < scrollTop) {
            listContainer.scrollTop = targetTop
        } else if (targetBottom > viewportBottom) {
            listContainer.scrollTop = targetBottom - containerHeight
        }
    }

    function handleArrowKey(key: string): boolean {
        const lastIndex = sortedShares.length - 1
        const newIndex =
            key === 'ArrowDown'
                ? Math.min(cursorIndex + 1, lastIndex)
                : key === 'ArrowUp'
                  ? Math.max(cursorIndex - 1, 0)
                  : key === 'ArrowLeft'
                    ? 0
                    : key === 'ArrowRight'
                      ? lastIndex
                      : null
        if (newIndex === null) return false
        cursorIndex = newIndex
        scrollToIndex(cursorIndex)
        return true
    }

    /**
     * Escape, Backspace, and `⌘↑` all return to the host list (`share.back`'s
     * registry keys; `⌘↑` mirrors the file list's `⌘↑` = parent). Matched on the
     * whole combo and handled before the arrow handler, which would otherwise treat
     * `⌘↑` as a cursor move.
     */
    function handleBackToHostKey(e: KeyboardEvent): boolean {
        if (eventMatchesCommand(e, 'share.back')) {
            e.preventDefault()
            onBack?.()
            return true
        }
        return false
    }

    /**
     * Handle keyboard navigation. Runs before the document-level dispatcher in
     * `+page.svelte` (bubble-phase on `document`, this pane is a descendant), so a
     * branch acting on a centrally dispatched combo must `stopPropagation()` or the
     * command runs twice. Nothing reads a return value: `preventDefault` +
     * `stopPropagation` is how a branch says it claimed the key.
     */
    export function handleKeyDown(e: KeyboardEvent): void {
        // The sign-in sheet is modal and owns the keyboard while it is up.
        if (signingIn) return

        if (sortedShares.length === 0) return

        // Escape / Backspace / ⌘↑ → back to the host list (before the arrow handler).
        if (handleBackToHostKey(e)) return

        // Try centralized navigation shortcuts first (PageUp, PageDown, Home, End, Option+arrows)
        const visibleItems = Math.max(1, Math.floor(containerHeight / SHARE_ROW_HEIGHT))
        const navResult = handleNavigationShortcut(e, {
            currentIndex: cursorIndex,
            totalCount: sortedShares.length,
            visibleItems,
        })
        if (navResult?.handled) {
            e.preventDefault()
            cursorIndex = navResult.newIndex
            scrollToIndex(cursorIndex)
            return
        }

        // Everything below is an unmodified key: `⌘←` / `⌘→` (copy path between
        // panes) and other modifier supersets belong to the document dispatcher.
        if (e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return

        if (['ArrowDown', 'ArrowUp', 'ArrowLeft', 'ArrowRight'].includes(e.key)) {
            e.preventDefault()
            handleArrowKey(e.key)
            return
        }

        // Handle action keys
        if (eventMatchesCommand(e, 'share.selectShare')) {
            e.preventDefault()
            if (cursorIndex >= 0 && cursorIndex < sortedShares.length) {
                void activateShare(sortedShares[cursorIndex])
            }
        }
    }

    async function handleForgetPassword() {
        try {
            await forgetCredentials(host.name)
            authenticatedCredentials = null
            addToast(tString('fileExplorer.network.forgotPassword', { hostName: host.name }), { level: 'success' })
        } catch {
            addToast(tString('fileExplorer.network.deletePasswordFailed'), { level: 'error' })
        }
    }

    function handleRetry() {
        error = null
        clearShareState(host.id)
        void loadShares()
    }
</script>

<div class="share-browser" class:is-focused={isFocused}>
    {#if loading}
        <div class="loading-state">
            <Spinner size="md" />
            {tString('fileExplorer.network.share.connecting', { hostName: host.name })}
        </div>
    {:else if error}
        <div class="error-state">
            <div class="error-icon"><Icon name="circle-alert" size={32} aria-hidden="true" /></div>
            <div class="error-title">
                {tString('fileExplorer.network.share.connectFailedTitle', { hostName: host.name })}
            </div>
            <div class="error-message">{error.message || error.type}</div>
            {#if error.type === 'missing_dependency' && error.installCommand}
                <CopyBox text={error.installCommand} />
                <div class="error-actions">
                    <Button variant="secondary" onclick={handleRetry}>{tString('fileExplorer.network.retry')}</Button>
                    <Button variant="secondary" onclick={onBack}>{tString('fileExplorer.network.back')}</Button>
                </div>
            {:else}
                <div class="error-actions">
                    <Button variant="secondary" onclick={handleRetry}>{tString('fileExplorer.network.retry')}</Button>
                    <Button variant="secondary" onclick={() => void askForCredentials('needs_credentials')}
                        >{tString('fileExplorer.network.signIn')}</Button
                    >
                    <Button variant="secondary" onclick={onBack}>{tString('fileExplorer.network.back')}</Button>
                </div>
            {/if}
        </div>
    {:else if sortedShares.length === 0}
        <div class="empty-state">
            <div class="empty-icon"><Icon name="folder" size={32} aria-hidden="true" /></div>
            <div class="empty-title">{tString('fileExplorer.network.share.noSharesTitle')}</div>
            <div class="empty-message">{tString('fileExplorer.network.share.noSharesMessage')}</div>
            <div class="error-actions">
                <Button variant="secondary" onclick={() => void askForCredentials('needs_credentials')}
                    >{tString('fileExplorer.network.signIn')}</Button
                >
                <Button variant="secondary" onclick={onBack}>{tString('fileExplorer.network.back')}</Button>
            </div>
        </div>
    {:else}
        <div class="header-row">
            <Button variant="secondary" size="mini" onclick={onBack}>
                <span class="btn-icon-label">
                    <Icon name="arrow-left" size={14} aria-hidden="true" />
                    {tString('fileExplorer.network.share.backArrow')}
                </span>
            </Button>
            <span class="host-name">{host.name}</span>
            {#if authenticatedCredentials}
                <button
                    class="forget-password-btn"
                    onclick={handleForgetPassword}
                    use:tooltip={tString('fileExplorer.network.share.forgetPasswordTooltip')}
                >
                    <Icon name="key" size={12} aria-hidden="true" />
                    {tString('fileExplorer.network.share.forgetPassword')}
                </button>
            {/if}
            <span class="share-count"
                >{tString('fileExplorer.network.share.shareCount', {
                    count: sortedShares.length,
                    countText: formatInteger(sortedShares.length),
                })}</span
            >
        </div>
        <div class="share-list" bind:this={listContainer} bind:clientHeight={containerHeight}>
            {#each sortedShares as share, index (share.name)}
                <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
                <div
                    class="share-row"
                    class:is-under-cursor={index === cursorIndex}
                    class:is-focused-and-under-cursor={isFocused && index === cursorIndex}
                    role="listitem"
                    onclick={() => {
                        handleShareClick(index)
                    }}
                    ondblclick={() => {
                        handleShareDoubleClick(index)
                    }}
                    onkeydown={() => {}}
                >
                    <span class="share-icon"><Icon name="folder" size={16} aria-hidden="true" /></span>
                    <span class="share-name">{share.name}</span>
                    {#if share.comment}
                        <span class="share-comment">{share.comment}</span>
                    {/if}
                </div>
            {/each}
        </div>
    {/if}
</div>

<style>
    .share-browser {
        display: flex;
        flex-direction: column;
        height: 100%;
        font-size: var(--font-size-sm);
        font-family: var(--font-system), sans-serif;
    }

    .loading-state,
    .error-state,
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

    .error-icon,
    .empty-icon {
        display: inline-flex;
        align-items: center;
        justify-content: center;
    }
    .error-icon {
        color: var(--color-error);
    }
    .empty-icon {
        color: var(--color-text-tertiary);
    }

    .error-title,
    .empty-title {
        font-size: var(--font-size-lg);
        font-weight: 500;
        color: var(--color-text-primary);
    }

    .error-message,
    .empty-message {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        text-align: center;
    }

    .error-actions {
        display: flex;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-sm);
    }

    .header-row {
        display: flex;
        align-items: center;
        gap: var(--spacing-md);
        padding: var(--spacing-sm) var(--spacing-md);
        background-color: var(--color-bg-secondary);
        border-bottom: 1px solid var(--color-border-strong);
    }
    .btn-icon-label {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .host-name {
        font-weight: 500;
        color: var(--color-text-primary);
    }

    .forget-password-btn {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: 1px var(--spacing-sm);
        font-family: var(--font-system), sans-serif;
        font-size: calc(var(--font-size-sm) * 0.9);
        color: var(--color-text-tertiary);
        background: none;
        border: 1px solid transparent;
        border-radius: var(--radius-sm);
    }

    .forget-password-btn:hover {
        color: var(--color-text-secondary);
        border-color: var(--color-border);
        background-color: var(--color-bg-tertiary);
    }

    .share-count {
        color: var(--color-text-tertiary);
        margin-left: auto;
    }

    .share-list {
        flex: 1;
        overflow-y: auto;
    }

    .share-row {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        height: 20px;
        padding: var(--spacing-xxs) var(--spacing-sm);
        cursor: default;
    }

    .share-row.is-under-cursor {
        background-color: var(--color-cursor-inactive);
    }

    .share-row.is-focused-and-under-cursor {
        background-color: var(--color-cursor-active);
    }

    .share-icon {
        display: inline-flex;
        align-items: center;
        color: var(--color-text-secondary);
    }

    .share-name {
        font-weight: 500;
    }

    .share-comment {
        color: var(--color-text-tertiary);
        margin-left: auto;
        font-size: var(--font-size-sm);
    }

    .share-row.is-focused-and-under-cursor .share-comment {
        color: var(--color-text-secondary);
    }
</style>
