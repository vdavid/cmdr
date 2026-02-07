<script lang="ts">
    import { onMount, tick } from 'svelte'
    import { activateLicense, validateLicenseWithServer, getLicenseInfo, type LicenseInfo } from '$lib/tauri-commands'
    import { loadLicenseStatus, getCachedStatus } from './licensing-store.svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'

    interface Props {
        onClose: () => void
        onSuccess: () => void
    }

    const { onClose, onSuccess }: Props = $props()

    let licenseKey = $state('')
    let error = $state('')
    let isActivating = $state(false)
    let hasError = $state(false)
    let inputElement: HTMLInputElement | undefined = $state()

    // Existing license info (if any)
    let existingLicense = $state<LicenseInfo | null>(null)
    let isLoading = $state(true)

    const SUPPORT_EMAIL = 'hello@getcmdr.com'

    // Get the cached status to check license type
    const status = getCachedStatus()

    // Determine if this is an existing license view
    const hasExistingLicense = $derived(existingLicense !== null)

    // Get the organization name for display
    const organizationName = $derived(existingLicense?.organizationName || 'your organization')

    // Format expiration date
    function formatDate(dateStr: string | null | undefined): string {
        if (!dateStr) return ''
        try {
            return new Date(dateStr).toLocaleDateString(undefined, {
                year: 'numeric',
                month: 'long',
                day: 'numeric',
            })
        } catch {
            return dateStr
        }
    }

    // Get the expiration date from status
    const expiresAt = $derived(status?.type === 'commercial' ? formatDate(status.expiresAt) : null)

    // Determine if this is a perpetual license
    const isPerpetual = $derived(status?.type === 'commercial' && status.licenseType === 'commercial_perpetual')

    function getErrorHelpText(errorMessage: string): string {
        if (errorMessage.includes('not found') || errorMessage.includes('expired')) {
            return '\n\nPlease verify that you pasted the correct license key from your purchase email.'
        }
        return '\n\nPlease check your internet connection and try activating your license again.'
    }

    onMount(async () => {
        // Check if there's an existing license
        try {
            existingLicense = await getLicenseInfo()
            if (existingLicense?.shortCode) {
                licenseKey = existingLicense.shortCode
            }
        } catch {
            // No existing license
        } finally {
            isLoading = false
        }

        // Only focus input if entering a new license
        if (!existingLicense) {
            await tick()
            inputElement?.focus()
        }
    })

    async function handleActivate() {
        if (!licenseKey.trim()) {
            error = 'Please enter a license key'
            hasError = true
            return
        }

        isActivating = true
        error = ''
        hasError = false

        try {
            // First validate the license key locally (signature verification)
            await activateLicense(licenseKey.trim())

            // Then validate with the server to check expiration
            await validateLicenseWithServer()

            // Reload the license status in the store
            await loadLicenseStatus()

            // Success - close this dialog and show About
            onSuccess()
        } catch (e) {
            const errorMessage = String(e)
            error = errorMessage + getErrorHelpText(errorMessage)
            hasError = true
        } finally {
            isActivating = false
        }
    }

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter' && !isActivating && !hasExistingLicense) {
            void handleActivate()
        }
    }

    function handleInputKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter' && !isActivating && !hasExistingLicense) {
            event.preventDefault()
            event.stopPropagation()
            void handleActivate()
        }
    }
</script>

<ModalDialog
    titleId="dialog-title"
    onkeydown={handleKeydown}
    blur
    dialogId="license"
    onclose={onClose}
    containerStyle="min-width: 400px; max-width: 500px"
>
    {#snippet title()}
        {#if isLoading}
            Loading...
        {:else if hasExistingLicense}
            License details
        {:else}
            Enter license key
        {/if}
    {/snippet}

    <div class="dialog-body">
        {#if !isLoading && hasExistingLicense}
            <p class="description">Your software is registered to {organizationName} with the license key:</p>

            <div class="input-group">
                <input
                    bind:value={licenseKey}
                    type="text"
                    class="license-input"
                    spellcheck="false"
                    autocomplete="off"
                    disabled={true}
                    onkeydown={handleInputKeydown}
                />
            </div>

            <p class="license-validity">
                {#if isPerpetual}
                    You can use this software forever for work.
                    {#if expiresAt}
                        <br />You're also entitled to all upgrades until {expiresAt}.
                    {/if}
                {:else if expiresAt}
                    Your license is valid until {expiresAt}. We'll email you in time to let you extend it conveniently.
                {:else}
                    Your license is active.
                {/if}
            </p>

            <div class="actions">
                <button class="primary" onclick={onClose}>Close</button>
            </div>
        {:else if !isLoading}
            <p class="description">Paste your license key from the email you received after purchase.</p>

            <div class="input-group">
                <input
                    bind:this={inputElement}
                    bind:value={licenseKey}
                    type="text"
                    class="license-input"
                    class:has-error={error}
                    placeholder="Example: CMDR-ABCD-EFGH-1234"
                    spellcheck="false"
                    autocomplete="off"
                    autocapitalize="off"
                    disabled={isActivating}
                    onkeydown={handleInputKeydown}
                />
            </div>

            {#if error}
                <p class="error-message">
                    {error}
                    <br /><br />
                    If you need help, contact us at
                    <a href="mailto:{SUPPORT_EMAIL}" class="support-link">{SUPPORT_EMAIL}</a>.
                </p>
            {/if}

            <div class="actions">
                <button class="secondary" onclick={onClose} disabled={isActivating}>Cancel</button>
                <button class="primary" onclick={handleActivate} disabled={isActivating || !licenseKey.trim()}>
                    {isActivating ? 'Activating...' : hasError ? 'Try again' : 'Activate'}
                </button>
            </div>
        {/if}
    </div>
</ModalDialog>

<style>
    .dialog-body {
        padding: 0 32px 24px;
    }

    .description {
        margin: 0 0 16px;
        font-size: 14px;
        color: var(--color-text-secondary);
        line-height: 1.5;
    }

    .input-group {
        margin-bottom: 16px;
    }

    .license-input {
        width: 100%;
        padding: 12px 14px;
        font-size: 14px;
        font-family: var(--font-system) sans-serif;
        background: var(--color-bg-primary);
        border: 1px solid var(--color-border-primary);
        border-radius: 8px;
        color: var(--color-text-primary);
        box-sizing: border-box;
    }

    .license-input::placeholder {
        color: var(--color-text-muted);
    }

    .license-input:focus {
        outline: none;
        border-color: var(--color-accent);
        box-shadow: 0 0 0 2px rgba(77, 163, 255, 0.2);
    }

    .license-input.has-error {
        border-color: var(--color-error);
    }

    .license-input:disabled {
        opacity: 0.6;
        cursor: not-allowed;
    }

    .error-message {
        margin: 0 0 16px;
        font-size: 13px;
        color: var(--color-error);
        line-height: 1.5;
        white-space: pre-wrap;
    }

    .support-link {
        color: var(--color-accent);
        text-decoration: underline;
    }

    .support-link:hover {
        color: var(--color-accent-hover);
    }

    .license-validity {
        margin: 0 0 16px;
        font-size: 14px;
        color: var(--color-text-secondary);
        line-height: 1.6;
    }

    .actions {
        display: flex;
        gap: 12px;
        justify-content: flex-end;
    }

    button {
        padding: 10px 18px;
        border-radius: 6px;
        font-size: 14px;
        font-weight: 500;
        cursor: pointer;
        transition: all 0.15s ease;
    }

    button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .primary {
        background: var(--color-accent);
        color: white;
        border: none;
    }

    .primary:hover:not(:disabled) {
        filter: brightness(1.1);
    }

    .secondary {
        background: transparent;
        color: var(--color-text-secondary);
        border: 1px solid var(--color-border-primary);
    }

    .secondary:hover:not(:disabled) {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }
</style>
