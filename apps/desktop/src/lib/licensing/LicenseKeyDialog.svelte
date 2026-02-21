<script lang="ts">
    import { onMount, tick } from 'svelte'
    import { activateLicense, validateLicenseWithServer, getLicenseInfo, type LicenseInfo } from '$lib/tauri-commands'
    import { loadLicenseStatus, getCachedStatus } from './licensing-store.svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'

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
                <Button variant="primary" onclick={onClose}>Close</Button>
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
                <Button variant="secondary" onclick={onClose} disabled={isActivating}>Cancel</Button>
                <Button variant="primary" onclick={handleActivate} disabled={isActivating || !licenseKey.trim()}>
                    {isActivating ? 'Activating...' : hasError ? 'Try again' : 'Activate'}
                </Button>
            </div>
        {/if}
    </div>
</ModalDialog>

<style>
    .dialog-body {
        padding: 0 var(--spacing-2xl) var(--spacing-xl);
    }

    .description {
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: 1.5;
    }

    .input-group {
        margin-bottom: 16px;
    }

    .license-input {
        width: 100%;
        padding: var(--spacing-md) 14px;
        font-size: var(--font-size-md);
        font-family: var(--font-system) sans-serif;
        background: var(--color-bg-primary);
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-lg);
        color: var(--color-text-primary);
        box-sizing: border-box;
    }

    .license-input::placeholder {
        color: var(--color-text-tertiary);
    }

    .license-input:focus {
        outline: none;
        border-color: var(--color-accent);
        box-shadow: var(--shadow-focus);
    }

    .license-input.has-error {
        border-color: var(--color-error);
    }

    .license-input:disabled {
        opacity: 0.6;
        cursor: not-allowed;
    }

    .error-message {
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-md);
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
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: 1.6;
    }

    .actions {
        display: flex;
        gap: 12px;
        justify-content: flex-end;
    }
</style>
