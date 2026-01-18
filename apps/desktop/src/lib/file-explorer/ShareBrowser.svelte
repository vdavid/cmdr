<script lang="ts">
    /**
     * ShareBrowser - displays shares on a network host.
     * Shows login form when authentication is required.
     */
    import { onMount } from 'svelte'
    import type { AuthMode, NetworkHost, ShareInfo, ShareListError } from './types'
    import {
        getShareState,
        fetchShares,
        clearShareState,
        setShareState,
        setCredentialStatus,
    } from '$lib/network-store.svelte'
    import {
        listSharesWithCredentials,
        saveSmbCredentials,
        getSmbCredentials,
        updateKnownShare,
    } from '$lib/tauri-commands'
    import NetworkLoginForm from './NetworkLoginForm.svelte'
    import { handleNavigationShortcut } from './keyboard-shortcuts'

    /** Row height for share list (matches Full list) */
    const SHARE_ROW_HEIGHT = 20

    interface Props {
        /** The host we're browsing */
        host: NetworkHost
        /** Whether this pane is focused */
        isFocused?: boolean
        /** Callback when user selects a share, includes credentials if auth was used */
        onShareSelect?: (share: ShareInfo, credentials: { username: string; password: string } | null) => void
        /** Callback to go back to host list */
        onBack?: () => void
    }

    const { host, isFocused = false, onShareSelect, onBack }: Props = $props()

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

    // Login form state
    let showLoginForm = $state(false)
    let loginError = $state<string | undefined>()
    let isConnecting = $state(false)

    // Track authenticated credentials for mounting
    let authenticatedCredentials = $state<{ username: string; password: string } | null>(null)

    // Container tracking for PageUp/PageDown
    let listContainer: HTMLDivElement | undefined = $state()
    let containerHeight = $state(0)

    // Load shares on mount
    onMount(async () => {
        await loadShares()
    })

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
            // If auth required, try stored credentials first (keep loading indicator)
            if (cachedState.error.type === 'auth_required' || cachedState.error.type === 'signing_required') {
                const success = await tryStoredCredentials()
                if (success) {
                    loading = false
                    return
                }
                // No stored credentials, show login form
                showLoginForm = true
            }
            // No stored credentials or they didn't work
            error = cachedState.error
            loading = false
            return
        }

        // Fetch shares
        try {
            const result = await fetchShares(host)
            shares = result.shares
            authMode = result.authMode
        } catch (e) {
            const shareError = e as ShareListError

            // If auth required, try stored credentials first (keep loading indicator)
            if (shareError.type === 'auth_required' || shareError.type === 'signing_required') {
                const success = await tryStoredCredentials()
                if (success) {
                    loading = false
                    return
                }
                // No stored credentials, show login form
                showLoginForm = true
            }
            error = shareError
        } finally {
            loading = false
        }
    }

    /** Try to use stored credentials. Returns true if successful. */
    async function tryStoredCredentials(): Promise<boolean> {
        const serverName = host.name

        // Try to get credentials directly - don't check hasSmbCredentials first
        // as that causes an extra Keychain dialog (each Keychain access = 1 dialog)
        try {
            const creds = await getSmbCredentials(serverName, null)
            // Store credentials in memory for mounting later
            authenticatedCredentials = { username: creds.username, password: creds.password }
            await connectWithCredentials(creds.username, creds.password, false)
            return true
        } catch {
            // No stored credentials or retrieval failed
            return false
        }
    }

    async function connectWithCredentials(
        username: string | null,
        password: string | null,
        rememberInKeychain: boolean,
    ) {
        isConnecting = true
        loginError = undefined

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
            )

            shares = result.shares
            authMode = result.authMode
            error = null
            showLoginForm = false

            // Update global share state so NetworkBrowser shows correct info
            setShareState(host.id, result)

            // Update credential status
            setCredentialStatus(host.name, username ? 'has_creds' : 'no_creds')

            // Store credentials for mounting
            if (username && password) {
                authenticatedCredentials = { username, password }
            } else {
                authenticatedCredentials = null
            }

            // Save credentials to Keychain if requested
            if (rememberInKeychain && username && password) {
                await saveSmbCredentials(host.name, null, username, password)
            }

            // Update known shares store
            await updateKnownShare(
                host.name,
                '', // Server-level, not share-specific
                username ? 'credentials' : 'guest',
                authMode === 'guest_allowed' ? 'guest_or_credentials' : 'credentials_only',
                username,
            )
        } catch (e) {
            const shareError = e as ShareListError
            if (shareError.type === 'auth_failed') {
                loginError = 'Invalid username or password. Please try again.'
                // Mark credentials as failed
                setCredentialStatus(host.name, 'failed')
            } else if (shareError.type === 'auth_required' || shareError.type === 'signing_required') {
                loginError = 'Authentication required. Please enter your credentials.'
            } else {
                loginError = shareError.message || `Connection failed: ${shareError.type}`
            }
        } finally {
            isConnecting = false
        }
    }

    function handleConnect(username: string | null, password: string | null, rememberInKeychain: boolean) {
        void connectWithCredentials(username, password, rememberInKeychain)
    }

    function handleCancel() {
        showLoginForm = false
        onBack?.()
    }

    function handleShareClick(index: number) {
        cursorIndex = index
    }

    function handleShareDoubleClick(index: number) {
        if (index >= 0 && index < sortedShares.length) {
            onShareSelect?.(sortedShares[index], authenticatedCredentials)
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

    export function handleKeyDown(e: KeyboardEvent): boolean {
        if (showLoginForm) {
            // Login form handles its own keyboard events
            if (e.key === 'Escape') {
                handleCancel()
                return true
            }
            return false
        }

        if (sortedShares.length === 0) return false

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
            return true
        }

        switch (e.key) {
            case 'ArrowDown':
                e.preventDefault()
                cursorIndex = Math.min(cursorIndex + 1, sortedShares.length - 1)
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
                cursorIndex = sortedShares.length - 1
                scrollToIndex(cursorIndex)
                return true
            case 'Enter':
                e.preventDefault()
                if (cursorIndex >= 0 && cursorIndex < sortedShares.length) {
                    onShareSelect?.(sortedShares[cursorIndex], authenticatedCredentials)
                }
                return true
            case 'Escape':
            case 'Backspace':
                e.preventDefault()
                onBack?.()
                return true
        }
        return false
    }

    function handleRetry() {
        error = null
        showLoginForm = false
        void loadShares()
    }
</script>

<div class="share-browser" class:is-focused={isFocused}>
    {#if showLoginForm}
        <NetworkLoginForm
            {host}
            {authMode}
            errorMessage={loginError}
            {isConnecting}
            onConnect={handleConnect}
            onCancel={handleCancel}
        />
    {:else if loading}
        <div class="loading-state">
            <span class="spinner"></span>
            Connecting to {host.name}...
        </div>
    {:else if error && !showLoginForm}
        <div class="error-state">
            <div class="error-icon">❌</div>
            <div class="error-title">Couldn't connect to {host.name}</div>
            <div class="error-message">{error.message || error.type}</div>
            <div class="error-actions">
                <button type="button" class="btn" onclick={handleRetry}>Retry</button>
                <button type="button" class="btn" onclick={() => (showLoginForm = true)}>Sign in</button>
                <button type="button" class="btn" onclick={onBack}>Back</button>
            </div>
        </div>
    {:else if sortedShares.length === 0}
        <div class="empty-state">
            <div class="empty-icon">📁</div>
            <div class="empty-title">No shares available</div>
            <div class="empty-message">This host has no accessible shares.</div>
            <button type="button" class="btn" onclick={onBack}>Back</button>
        </div>
    {:else}
        <div class="header-row">
            <button type="button" class="back-button" onclick={onBack}>← Back</button>
            <span class="host-name">{host.name}</span>
            <span class="share-count">{sortedShares.length} {sortedShares.length === 1 ? 'share' : 'shares'}</span>
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
                    <span class="share-icon">📁</span>
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
        padding: 24px;
        gap: 12px;
        color: var(--color-text-secondary);
    }

    .spinner {
        width: 24px;
        height: 24px;
        border: 3px solid var(--color-border-primary);
        border-top-color: var(--color-accent);
        border-radius: 50%;
        animation: spin 1s linear infinite;
    }

    @keyframes spin {
        to {
            transform: rotate(360deg);
        }
    }

    .error-icon,
    .empty-icon {
        font-size: 32px;
    }

    .error-title,
    .empty-title {
        font-size: 16px;
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
        gap: 8px;
        margin-top: 8px;
    }

    .btn {
        padding: 8px 16px;
        border: 1px solid var(--color-border-primary);
        border-radius: 6px;
        background-color: var(--color-bg-secondary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        cursor: pointer;
        transition: background-color 0.15s ease;
    }

    .btn:hover {
        background-color: var(--color-button-hover);
    }

    .header-row {
        display: flex;
        align-items: center;
        gap: 12px;
        padding: 8px 12px;
        background-color: var(--color-bg-secondary);
        border-bottom: 1px solid var(--color-border-primary);
    }

    .back-button {
        padding: 4px 8px;
        border: 1px solid var(--color-border-primary);
        border-radius: 4px;
        background-color: transparent;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        cursor: pointer;
    }

    .back-button:hover {
        background-color: var(--color-button-hover);
    }

    .host-name {
        font-weight: 500;
        color: var(--color-text-primary);
    }

    .share-count {
        color: var(--color-text-muted);
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
        background-color: var(--color-cursor-unfocused-bg);
    }

    .share-row.is-focused-and-under-cursor {
        background-color: var(--color-cursor-focused-bg);
        color: var(--color-cursor-focused-fg);
    }

    .share-icon {
        font-size: 16px;
    }

    .share-name {
        font-weight: 500;
    }

    .share-comment {
        color: var(--color-text-tertiary);
        margin-left: auto;
        font-size: var(--font-size-xs);
    }

    .share-row.is-focused-and-under-cursor .share-comment {
        color: var(--color-cursor-focused-fg);
        opacity: 0.8;
    }
</style>
