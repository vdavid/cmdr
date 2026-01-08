<script lang="ts">
    /**
     * NetworkLoginForm - Login form for SMB authentication.
     * Displayed in the file pane when credentials are required to connect to a network share.
     */
    import { onMount } from 'svelte'
    import type { AuthMode, ConnectionMode, KnownNetworkShare, NetworkHost } from './types'
    import { getUsernameHints, getKnownShareByName } from '$lib/tauri-commands'

    interface Props {
        /** The network host we're connecting to */
        host: NetworkHost
        /** Optional share name if connecting to a specific share */
        shareName?: string
        /** Current auth mode detected */
        authMode: AuthMode
        /** Error message to display, if any */
        errorMessage?: string
        /** Whether we're currently attempting to connect */
        isConnecting?: boolean
        /** Callback when user submits credentials */
        onConnect: (username: string | null, password: string | null, rememberInKeychain: boolean) => void
        /** Callback when user cancels */
        onCancel: () => void
    }

    const { host, shareName, authMode, errorMessage, isConnecting = false, onConnect, onCancel }: Props = $props()

    // Form state - writable derived that syncs with authMode prop
    let connectionMode = $derived.by<ConnectionMode>(() => (authMode === 'guest_allowed' ? 'guest' : 'credentials'))
    let username = $state('')
    let password = $state('')
    let rememberInKeychain = $state(true)

    // Cached data for contextual messaging
    let knownShare = $state<KnownNetworkShare | null>(null)
    let usernameHint = $state<string | null>(null)

    // Detect if auth options have changed from last known
    const authOptionsChanged = $derived(() => {
        if (!knownShare) return null

        const current = authMode
        const stored = knownShare.lastKnownAuthOptions

        // Was guest-only, now can use credentials
        if (stored === 'guest_only' && current === 'guest_allowed') {
            return 'guest_can_now_auth' as const
        }
        // Was credentials required/both, now guest-only
        if ((stored === 'credentials_only' || stored === 'guest_or_credentials') && current === 'creds_required') {
            return null // Still requires creds, no change
        }

        return null
    })

    // Load known share data and username hints on mount
    onMount(async () => {
        // Get username hints for pre-filling
        const hints = await getUsernameHints()
        const serverKey = host.name.toLowerCase()
        if (hints[serverKey]) {
            usernameHint = hints[serverKey]
            username = hints[serverKey]
        }

        // Get known share data if we have a share name
        if (shareName) {
            knownShare = await getKnownShareByName(host.name, shareName)
            if (knownShare?.username) {
                username = knownShare.username
            }
        }
    })

    // Computed display values
    const title = $derived(shareName ? `Sign in to "${host.name}/${shareName}"` : `Sign in to "${host.name}"`)
    const showGuestOption = $derived(authMode === 'guest_allowed')
    const canSubmit = $derived(connectionMode === 'guest' || (username.trim() !== '' && password !== ''))

    function handleSubmit(e: Event) {
        e.preventDefault()
        if (!canSubmit || isConnecting) return

        if (connectionMode === 'guest') {
            onConnect(null, null, false)
        } else {
            onConnect(username.trim(), password, rememberInKeychain)
        }
    }

    function handleKeyDown(e: KeyboardEvent) {
        if (e.key === 'Escape') {
            e.preventDefault()
            onCancel()
        }
        // Allow Tab/Shift+Tab to navigate between form fields normally
        // by preventing the event from bubbling to parent handlers (which switch panes)
        if (e.key === 'Tab') {
            e.stopPropagation()
        }
    }
</script>

<div class="login-container" role="dialog" aria-labelledby="login-title" onkeydown={handleKeyDown} tabindex="-1">
    <div class="login-card">
        <h2 id="login-title" class="login-title">
            <span class="lock-icon">🔒</span>
            {title}
        </h2>

        {#if authOptionsChanged()}
            <div class="auth-changed-message">
                {#if authOptionsChanged() === 'guest_can_now_auth'}
                    <span class="info-icon">ℹ️</span>
                    You connected as guest before. You can now sign in for more access.
                {/if}
            </div>
        {/if}

        {#if errorMessage}
            <div class="error-message" role="alert">
                <span class="error-icon">⚠️</span>
                {errorMessage}
            </div>
        {/if}

        <form onsubmit={handleSubmit}>
            {#if showGuestOption}
                <fieldset class="connection-mode">
                    <legend class="sr-only">Connection mode</legend>

                    <label class="radio-option">
                        <input
                            type="radio"
                            name="connectionMode"
                            value="guest"
                            bind:group={connectionMode}
                            disabled={isConnecting}
                        />
                        <span class="radio-label">Connect as guest</span>
                    </label>

                    <label class="radio-option">
                        <input
                            type="radio"
                            name="connectionMode"
                            value="credentials"
                            bind:group={connectionMode}
                            disabled={isConnecting}
                        />
                        <span class="radio-label">Sign in with credentials</span>
                    </label>
                </fieldset>
            {/if}

            <div class="credentials-fields" class:disabled={connectionMode === 'guest'}>
                <div class="field">
                    <label for="username" class="field-label">Username</label>
                    <input
                        id="username"
                        type="text"
                        class="field-input"
                        bind:value={username}
                        disabled={connectionMode === 'guest' || isConnecting}
                        placeholder={usernameHint ?? 'Example: david'}
                        autocomplete="username"
                        autocapitalize="off"
                        spellcheck="false"
                    />
                </div>

                <div class="field">
                    <label for="password" class="field-label">Password</label>
                    <input
                        id="password"
                        type="password"
                        class="field-input"
                        bind:value={password}
                        disabled={connectionMode === 'guest' || isConnecting}
                        placeholder="Enter password"
                        autocomplete="current-password"
                    />
                </div>

                <label class="checkbox-option">
                    <input
                        type="checkbox"
                        bind:checked={rememberInKeychain}
                        disabled={connectionMode === 'guest' || isConnecting}
                    />
                    <span class="checkbox-label">Remember in Keychain</span>
                </label>
            </div>

            <div class="button-row">
                <button type="button" class="btn btn-secondary" onclick={onCancel} disabled={isConnecting}>
                    Cancel
                </button>
                <button type="submit" class="btn btn-primary" disabled={!canSubmit || isConnecting}>
                    {#if isConnecting}
                        <span class="spinner"></span>
                        Connecting...
                    {:else}
                        Connect
                    {/if}
                </button>
            </div>
        </form>
    </div>
</div>

<style>
    .login-container {
        display: flex;
        align-items: center;
        justify-content: center;
        height: 100%;
        padding: var(--spacing-md);
        background-color: var(--color-bg-primary);
    }

    .login-card {
        max-width: 400px;
        width: 100%;
        padding: 24px;
        background-color: var(--color-bg-secondary);
        border: 1px solid var(--color-border-primary);
        border-radius: 12px;
        box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1);
    }

    .login-title {
        display: flex;
        align-items: center;
        gap: 8px;
        margin-bottom: 16px;
        font-size: 18px;
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .lock-icon {
        font-size: 20px;
    }

    .auth-changed-message {
        display: flex;
        align-items: flex-start;
        gap: 8px;
        padding: 12px;
        margin-bottom: 16px;
        background-color: color-mix(in srgb, var(--color-accent) 15%, transparent);
        border: 1px solid var(--color-accent);
        border-radius: 8px;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .info-icon {
        flex-shrink: 0;
    }

    .error-message {
        display: flex;
        align-items: flex-start;
        gap: 8px;
        padding: 12px;
        margin-bottom: 16px;
        background-color: color-mix(in srgb, var(--color-error) 15%, transparent);
        border: 1px solid var(--color-error);
        border-radius: 8px;
        font-size: var(--font-size-sm);
        color: var(--color-error);
    }

    .error-icon {
        flex-shrink: 0;
    }

    .connection-mode {
        border: none;
        margin-bottom: 16px;
        padding: 0;
    }

    .radio-option {
        display: flex;
        align-items: center;
        gap: 8px;
        padding: 8px 0;
        cursor: pointer;
    }

    .radio-option input[type='radio'] {
        width: 16px;
        height: 16px;
        accent-color: var(--color-accent);
        cursor: pointer;
    }

    .radio-label {
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
    }

    .credentials-fields {
        transition: opacity 0.2s ease;
    }

    .credentials-fields.disabled {
        opacity: 0.5;
        pointer-events: none;
    }

    .field {
        margin-bottom: 12px;
    }

    .field-label {
        display: block;
        margin-bottom: 4px;
        font-size: var(--font-size-sm);
        font-weight: 500;
        color: var(--color-text-secondary);
    }

    .field-input {
        width: 100%;
        padding: 10px 12px;
        border: 1px solid var(--color-border-primary);
        border-radius: 6px;
        background-color: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        transition:
            border-color 0.15s ease,
            box-shadow 0.15s ease;
    }

    .field-input:focus {
        outline: none;
        border-color: var(--color-accent);
        box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-accent) 25%, transparent);
    }

    .field-input::placeholder {
        color: var(--color-text-muted);
    }

    .field-input:disabled {
        background-color: var(--color-bg-tertiary);
        cursor: not-allowed;
    }

    .checkbox-option {
        display: flex;
        align-items: center;
        gap: 8px;
        padding: 8px 0;
        cursor: pointer;
    }

    .checkbox-option input[type='checkbox'] {
        width: 16px;
        height: 16px;
        accent-color: var(--color-accent);
        cursor: pointer;
    }

    .checkbox-label {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .button-row {
        display: flex;
        justify-content: flex-end;
        gap: 12px;
        margin-top: 20px;
    }

    .btn {
        padding: 10px 20px;
        border-radius: 6px;
        font-size: var(--font-size-sm);
        font-weight: 500;
        cursor: pointer;
        transition:
            background-color 0.15s ease,
            opacity 0.15s ease;
    }

    .btn:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .btn-secondary {
        background-color: var(--color-bg-tertiary);
        border: 1px solid var(--color-border-primary);
        color: var(--color-text-primary);
    }

    .btn-secondary:hover:not(:disabled) {
        background-color: var(--color-bg-hover);
    }

    .btn-primary {
        display: flex;
        align-items: center;
        gap: 8px;
        background-color: var(--color-accent);
        border: none;
        color: #ffffff;
    }

    .btn-primary:hover:not(:disabled) {
        filter: brightness(1.1);
    }

    .spinner {
        width: 14px;
        height: 14px;
        border: 2px solid rgba(255, 255, 255, 0.3);
        border-top-color: #ffffff;
        border-radius: 50%;
        animation: spin 0.8s linear infinite;
    }

    @keyframes spin {
        to {
            transform: rotate(360deg);
        }
    }

    /* Screen reader only */
    .sr-only {
        position: absolute;
        width: 1px;
        height: 1px;
        padding: 0;
        margin: -1px;
        overflow: hidden;
        clip-path: inset(50%);
        white-space: nowrap;
        border: 0;
    }
</style>
