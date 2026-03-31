<script lang="ts">
    import { onMount, tick } from 'svelte'
    import {
        verifyLicense,
        commitLicense,
        validateLicenseWithServer,
        getLicenseInfo,
        resetLicense,
        openExternalUrl,
        parseActivationError,
        type LicenseInfo,
        type LicenseStatus,
    } from '$lib/tauri-commands'
    import {
        loadLicenseStatus,
        getCachedStatus,
        setCachedStatus,
        isPendingVerification,
        setPendingVerification,
    } from './licensing-store.svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { addToast } from '$lib/ui/toast/toast-store.svelte'

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

    let existingLicense = $state<LicenseInfo | null>(null)
    let isLoading = $state(true)
    let isConfirmingReset = $state(false)
    let isServerInvalidError = $state(false)
    let serverInvalidRetryCount = $state(0)

    let errorHelpHint = $state('')

    const SUPPORT_EMAIL = 'hello@getcmdr.com'

    const status = getCachedStatus()

    const hasExistingLicense = $derived(existingLicense !== null)
    const cleanedKey = $derived(licenseKey.replace(/\s/g, ''))

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

    function getLicenseTypeLabel(licenseType: string | null | undefined): string | null {
        if (licenseType === 'commercial_perpetual') return 'Commercial perpetual'
        if (licenseType === 'commercial_subscription') return 'Commercial subscription'
        return null
    }

    // Matches backend VALIDATION_INTERVAL_SECS (7 days) in app_status.rs
    const validationIntervalDays = 7

    function getValidityText(s: LicenseStatus | null, pending: boolean): string | null {
        if (pending && s?.type === 'commercial') {
            return 'Not yet verified'
        }
        if (s?.type === 'commercial') {
            if (s.licenseType === 'commercial_perpetual') {
                return s.expiresAt ? `Perpetual — updates until ${formatDate(s.expiresAt)}` : 'Perpetual'
            }
            return s.expiresAt ? `Valid until ${formatDate(s.expiresAt)}` : 'Active'
        }
        if (s?.type === 'expired') return `Expired on ${formatDate(s.expiredAt)}`
        return null
    }

    const licenseTypeLabel = $derived(getLicenseTypeLabel(existingLicense?.licenseType))
    const pendingVerification = $derived(isPendingVerification())
    const validityText = $derived(getValidityText(status, pendingVerification))
    const orgName = $derived(existingLicense?.organizationName ?? null)
    const shortCode = $derived(existingLicense?.shortCode ?? null)
    const isServerInvalid = $derived(existingLicense !== null && status?.type === 'personal')

    function getFriendlyError(e: unknown): { error: string; hint: string } {
        const parsed = parseActivationError(e)
        switch (parsed?.code) {
            case 'badSignature':
                return {
                    error: "This license key failed our signature verification, meaning that it doesn't look like a valid key.",
                    hint: 'Please double-check for typos and try pasting it again from your purchase email.',
                }
            case 'invalidFormat':
            case 'badEncoding':
            case 'badPayload':
                return {
                    error: 'Our license key format is different from this.',
                    hint: 'License keys are either a short code (CMDR-XXXX-XXXX-XXXX) or a longer cryptographic key from your purchase email.',
                }
            case 'shortCodeNotFound':
                return {
                    error: "This license looks good, but we looked hard and couldn't find it in our database.",
                    hint: 'Please verify that you pasted the correct key from your purchase email.',
                }
            case 'networkError':
                return {
                    error: "Ouch, we couldn't reach the license server this time.",
                    hint: 'Please check your internet connection and try again.',
                }
            case 'serverError':
                return {
                    error: "Hmm, the license server responded with something weird. We're sorry about that.",
                    hint: 'Please try again later.',
                }
            default:
                return {
                    error: 'Something went wrong when activating this key.',
                    hint: "Please try again. If the problem persists, email us and we'll help.",
                }
        }
    }

    onMount(async () => {
        try {
            existingLicense = await getLicenseInfo()
        } catch {
            // No existing license
        } finally {
            isLoading = false
        }

        if (!existingLicense) {
            await tick()
            inputElement?.focus()
        }
    })

    function showActivationToast(info: LicenseInfo): void {
        const name = info.organizationName
        const message = name
            ? `Welcome aboard, ${name}! Thanks for your support.`
            : 'License activated — thanks for your support!'
        addToast(message, { level: 'success' })
    }

    function buildFallbackStatus(info: LicenseInfo): LicenseStatus | null {
        if (info.licenseType === 'commercial_subscription' || info.licenseType === 'commercial_perpetual') {
            return {
                type: 'commercial',
                licenseType: info.licenseType,
                organizationName: info.organizationName,
                expiresAt: null,
            }
        }
        return null
    }

    function handleNetworkFallback(licenseInfo: LicenseInfo): void {
        const fallbackStatus = buildFallbackStatus(licenseInfo)
        if (fallbackStatus) {
            setCachedStatus(fallbackStatus)
            setPendingVerification(true)
            error = ''
            errorHelpHint = ''
            hasError = false
            isServerInvalidError = false
            showActivationToast(licenseInfo)
            onSuccess()
        } else {
            error = "Couldn't verify your license with the server. Please try again later."
            hasError = true
        }
    }

    function buildMailtoUrl(key: string): string {
        const subject = encodeURIComponent('License key issue')
        const body = encodeURIComponent(`Hi,\n\nI'm having trouble activating my license key:\n${key}\n\n`)
        return `mailto:${SUPPORT_EMAIL}?subject=${subject}&body=${body}`
    }

    async function handleEmailClick(event: MouseEvent) {
        event.preventDefault()
        const keyForEmail = cleanedKey || shortCode || ''
        const mailtoUrl = buildMailtoUrl(keyForEmail)
        await openExternalUrl(mailtoUrl)
    }

    async function handleActivate() {
        if (!cleanedKey) {
            error = 'Please enter a license key'
            hasError = true
            return
        }

        isActivating = true
        // Don't clear error/hasError here — keep old error visible during retry to avoid flicker.
        // Each outcome path below sets its own error state.

        try {
            // Step 1: Verify the key offline (Ed25519 + short code exchange if needed).
            // Nothing is written to disk — if this fails, we show a clean error.
            const verifyResult = await verifyLicense(cleanedKey)

            // Step 2: Validate with the license server, passing transactionId explicitly
            // since the key isn't stored yet.
            let newStatus: LicenseStatus | null = null
            try {
                newStatus = await validateLicenseWithServer(verifyResult.info.transactionId)
            } catch {
                // Network error — fall through to network fallback
            }

            // Step 3: Decide whether to commit (persist) the key based on server response.
            if (newStatus?.type === 'commercial') {
                await commitLicense(verifyResult.fullKey, verifyResult.shortCode)
                setPendingVerification(false)
                setCachedStatus(newStatus)
                serverInvalidRetryCount = 0
                error = ''
                errorHelpHint = ''
                hasError = false
                isServerInvalidError = false
                showActivationToast(verifyResult.info)
                onSuccess()
            } else if (newStatus?.type === 'expired') {
                // Key IS valid, just expired — commit it so the user can renew
                await commitLicense(verifyResult.fullKey, verifyResult.shortCode)
                setCachedStatus(newStatus)
                isServerInvalidError = false
                error = `This license expired on ${formatDate(newStatus.expiredAt)}.`
                errorHelpHint = 'You can renew your subscription or purchase a new license at getcmdr.com.'
                hasError = true
            } else if (newStatus?.type === 'personal') {
                // Server checked with Paddle and this transaction is unknown.
                // DON'T commit — nothing is stored, no cleanup needed.
                serverInvalidRetryCount++
                isServerInvalidError = true
                error =
                    "We know this key but when we checked it with our payment provider, it didn't recognize it. This can happen if the purchase was refunded or not cleared."
                hasError = true
            } else {
                // Network error (newStatus is null) — key is crypto-valid, commit optimistically
                await commitLicense(verifyResult.fullKey, verifyResult.shortCode)
                handleNetworkFallback(verifyResult.info)
            }
        } catch (e) {
            const friendly = getFriendlyError(e)
            isServerInvalidError = false
            error = friendly.error
            errorHelpHint = friendly.hint
            hasError = true
        } finally {
            isActivating = false
        }
    }

    async function handleResetConfirm() {
        await resetLicense()
        await loadLicenseStatus()
        existingLicense = null
        licenseKey = ''
        isConfirmingReset = false
        await tick()
        inputElement?.focus()
    }

    function handleCancelAfterInvalid() {
        // Invalid keys are never stored, so no cleanup is needed — just close.
        onClose()
    }

    function handleClose() {
        // Invalid keys are never stored (verify/commit split), so no cleanup needed on close.
        onClose()
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
    onclose={handleClose}
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
        {#if !isLoading && hasExistingLicense && !isConfirmingReset}
            {#if isServerInvalid}
                <div class="warning-banner">
                    <span class="warning-text">
                        This key couldn't be verified with the server. Please try a different key or email us at
                        <a href="mailto:{SUPPORT_EMAIL}" class="support-link" onclick={handleEmailClick}
                            >{SUPPORT_EMAIL}</a
                        >.
                    </span>
                </div>
            {/if}

            <div class="info-box">
                {#if licenseTypeLabel}
                    <div class="info-row">
                        <span class="info-label">License type</span>
                        <span class="info-value">{licenseTypeLabel}</span>
                    </div>
                {/if}
                {#if orgName}
                    <div class="info-row">
                        <span class="info-label">Organization</span>
                        <span class="info-value">{orgName}</span>
                    </div>
                {/if}
                {#if validityText}
                    <div class="info-row">
                        <span class="info-label">Validity</span>
                        <span
                            class="info-value validity-value"
                            class:validity-pending={pendingVerification}
                            class:validity-expired={status?.type === 'expired'}>{validityText}</span
                        >
                    </div>
                    {#if pendingVerification}
                        <div class="info-row-sub">
                            <span class="info-hint">
                                We'll verify with the server automatically within {validationIntervalDays} days.
                            </span>
                        </div>
                    {/if}
                {/if}
                {#if shortCode}
                    <div class="info-row">
                        <span class="info-label">License key</span>
                        <span class="info-value mono">{shortCode}</span>
                    </div>
                {/if}
            </div>

            <div class="button-row details-buttons">
                <Button variant="secondary" onclick={() => (isConfirmingReset = true)}>Use a different key</Button>
                <Button variant="primary" onclick={onClose}>Close</Button>
            </div>
        {:else if !isLoading && isConfirmingReset}
            <p class="description">
                This will deactivate your current license on this device. You can reactivate anytime with a valid key.
            </p>

            <div class="button-row">
                <Button variant="secondary" onclick={() => (isConfirmingReset = false)}>Cancel</Button>
                <Button variant="primary" onclick={handleResetConfirm}>Continue</Button>
            </div>
        {:else if !isLoading}
            <p class="description">
                Paste your license key from the email you received after purchase. Don't have one yet? <a
                    href="https://getcmdr.com/pricing"
                    class="buy-link"
                    onclick={(event: MouseEvent) => {
                        event.preventDefault()
                        void openExternalUrl('https://getcmdr.com/pricing')
                    }}>Get a license</a
                >.
            </p>

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
                <p class="error-message">{error}</p>
                {#if errorHelpHint}
                    <p class="help-text">{errorHelpHint}</p>
                {/if}
                <p class="help-text">
                    {#if isServerInvalidError && serverInvalidRetryCount >= 3}
                        We've tried {serverInvalidRetryCount} times and it didn't work. We're sorry for the trouble — please
                        drop us a message at
                        <a href="mailto:{SUPPORT_EMAIL}" class="support-link" onclick={handleEmailClick}
                            >{SUPPORT_EMAIL}</a
                        >

                        and we'll sort it out.
                    {:else if isServerInvalidError}
                        If you believe this is a mistake, email us at
                        <a href="mailto:{SUPPORT_EMAIL}" class="support-link" onclick={handleEmailClick}
                            >{SUPPORT_EMAIL}</a
                        >

                        and we'll sort it out.
                    {:else}
                        If you need help, contact us at
                        <a href="mailto:{SUPPORT_EMAIL}" class="support-link" onclick={handleEmailClick}
                            >{SUPPORT_EMAIL}</a
                        >.
                    {/if}
                </p>
            {/if}

            <div class="button-row">
                {#if isServerInvalidError}
                    <Button variant="secondary" onclick={handleCancelAfterInvalid}>Cancel</Button>
                    <Button variant="primary" onclick={handleActivate} disabled={isActivating}>
                        {#if isActivating}<span class="btn-spinner"></span>{/if}
                        {isActivating ? 'Checking...' : 'Try again'}
                    </Button>
                {:else}
                    <Button variant="secondary" onclick={handleClose}>Cancel</Button>
                    <Button variant="primary" onclick={handleActivate} disabled={isActivating || !cleanedKey}>
                        {#if isActivating}<span class="btn-spinner"></span>{/if}
                        {isActivating ? 'Activating...' : hasError ? 'Try again' : 'Activate'}
                    </Button>
                {/if}
            </div>
        {/if}
    </div>
</ModalDialog>

<style>
    .dialog-body {
        padding: var(--spacing-md) var(--spacing-2xl) var(--spacing-xl);
    }

    .description {
        margin: 0 0 var(--spacing-xl);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: 1.5;
    }

    .info-box {
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-lg);
        padding: var(--spacing-sm) var(--spacing-xl);
        margin-bottom: var(--spacing-xl);
    }

    .info-row {
        display: flex;
        justify-content: space-between;
        align-items: center;
        gap: var(--spacing-xl);
        padding: var(--spacing-md) 0;
    }

    .info-row:not(:last-child) {
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .info-label {
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        flex-shrink: 0;
    }

    .info-value {
        font-size: var(--font-size-md);
        color: var(--color-text-primary);
        font-weight: 500;
        text-align: right;
    }

    .validity-value {
        color: var(--color-toast-success-stripe);
    }

    .validity-value.validity-pending {
        color: var(--color-warning);
    }

    .validity-value.validity-expired {
        color: var(--color-error);
    }

    .info-row-sub {
        text-align: right;
        padding: 0 0 var(--spacing-md);
    }

    .info-hint {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        font-weight: 400;
        line-height: 1.4;
    }

    .info-value.mono {
        font-family: var(--font-mono);
        letter-spacing: 0.02em;
    }

    .warning-banner {
        background: var(--color-warning-bg);
        border: 1px solid var(--color-warning);
        border-radius: var(--radius-lg);
        padding: var(--spacing-md) var(--spacing-lg);
        margin-bottom: var(--spacing-lg);
    }

    .warning-text {
        font-size: var(--font-size-md);
        color: var(--color-warning);
        line-height: 1.5;
    }

    .input-group {
        margin-bottom: var(--spacing-lg);
    }

    .license-input {
        width: 100%;
        padding: var(--spacing-md);
        font-size: var(--font-size-md);
        font-family: var(--font-system), sans-serif;
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
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-md);
        color: var(--color-error);
        line-height: 1.5;
        white-space: pre-wrap;
    }

    .help-text {
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: 1.5;
    }

    .support-link {
        color: var(--color-accent-text);
        text-decoration: underline;
    }

    .support-link:hover {
        color: var(--color-accent-hover);
    }

    .buy-link {
        color: var(--color-accent-text);
        text-decoration: underline;
    }

    .buy-link:hover {
        color: var(--color-accent-hover);
    }

    .button-row {
        display: flex;
        gap: var(--spacing-md);
        justify-content: center;
    }

    .button-row.details-buttons {
        justify-content: center;
    }

    .btn-spinner {
        display: inline-block;
        width: 12px;
        height: 12px;
        border: 2px solid var(--color-accent-fg);
        border-top-color: transparent;
        border-radius: 50%;
        animation: spin 0.8s linear infinite;
        vertical-align: middle;
        margin-right: var(--spacing-xs);
    }
</style>
