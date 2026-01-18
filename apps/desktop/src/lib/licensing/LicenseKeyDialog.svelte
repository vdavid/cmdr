<script lang="ts">
    import { onMount, tick } from 'svelte'
    import { activateLicense, validateLicenseWithServer } from '$lib/tauri-commands'
    import { loadLicenseStatus } from '$lib/licensing-store.svelte'

    interface Props {
        onClose: () => void
        onSuccess: () => void
    }

    const { onClose, onSuccess }: Props = $props()

    let licenseKey = $state('')
    let error = $state('')
    let isActivating = $state(false)
    let inputElement: HTMLInputElement | undefined = $state()
    let overlayElement: HTMLDivElement | undefined = $state()

    onMount(async () => {
        // Focus overlay so keyboard events work immediately
        await tick()
        overlayElement?.focus()
        // Then focus the input
        await tick()
        inputElement?.focus()
    })

    async function handleActivate() {
        if (!licenseKey.trim()) {
            error = 'Please enter a license key'
            return
        }

        isActivating = true
        error = ''

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
            error = String(e)
        } finally {
            isActivating = false
        }
    }

    function handleKeydown(event: KeyboardEvent) {
        // Stop propagation to prevent file explorer from handling keys while modal is open
        event.stopPropagation()
        if (event.key === 'Escape') {
            onClose()
        } else if (event.key === 'Enter' && !isActivating) {
            void handleActivate()
        }
    }

    function handleInputKeydown(event: KeyboardEvent) {
        // Allow Enter to submit
        if (event.key === 'Enter' && !isActivating) {
            event.preventDefault()
            void handleActivate()
        }
        // Stop propagation for all keys to prevent file explorer shortcuts
        event.stopPropagation()
    }
</script>

<div
    bind:this={overlayElement}
    class="modal-overlay"
    role="dialog"
    aria-modal="true"
    aria-labelledby="dialog-title"
    tabindex="-1"
    onkeydown={handleKeydown}
>
    <div class="modal-content">
        <button class="close-button" onclick={onClose} aria-label="Close">×</button>

        <h2 id="dialog-title">Enter license key</h2>

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
                autocorrect="off"
                autocapitalize="off"
                disabled={isActivating}
                onkeydown={handleInputKeydown}
            />
        </div>

        {#if error}
            <p class="error-message">{error}</p>
        {/if}

        <div class="actions">
            <button class="secondary" onclick={onClose} disabled={isActivating}>Cancel</button>
            <button class="primary" onclick={handleActivate} disabled={isActivating || !licenseKey.trim()}>
                {isActivating ? 'Activating...' : 'Activate'}
            </button>
        </div>
    </div>
</div>

<style>
    .modal-overlay {
        position: fixed;
        inset: 0;
        background: rgba(0, 0, 0, 0.6);
        display: flex;
        align-items: center;
        justify-content: center;
        z-index: 9999;
        backdrop-filter: blur(4px);
    }

    .modal-content {
        background: var(--color-bg-secondary, #2a2a2a);
        border: 1px solid var(--color-border-primary, #444);
        border-radius: 12px;
        padding: 24px 32px;
        min-width: 400px;
        max-width: 500px;
        box-shadow: 0 16px 48px rgba(0, 0, 0, 0.4);
        position: relative;
    }

    .close-button {
        position: absolute;
        top: 12px;
        right: 12px;
        background: none;
        border: none;
        color: var(--color-text-secondary, #888);
        font-size: 24px;
        cursor: pointer;
        padding: 4px 8px;
        line-height: 1;
        border-radius: 4px;
    }

    .close-button:hover {
        background: var(--color-button-hover, rgba(255, 255, 255, 0.1));
        color: var(--color-text-primary, #fff);
    }

    h2 {
        margin: 0 0 12px;
        font-size: 18px;
        font-weight: 600;
        color: var(--color-text-primary, #fff);
    }

    .description {
        margin: 0 0 16px;
        font-size: 14px;
        color: var(--color-text-secondary, #aaa);
        line-height: 1.5;
    }

    .input-group {
        margin-bottom: 16px;
    }

    .license-input {
        width: 100%;
        padding: 12px 14px;
        font-size: 14px;
        font-family: var(--font-system);
        background: var(--color-bg-primary, #1e1e1e);
        border: 1px solid var(--color-border-primary, #444);
        border-radius: 8px;
        color: var(--color-text-primary, #fff);
        box-sizing: border-box;
    }

    .license-input::placeholder {
        color: var(--color-text-muted, #666);
    }

    .license-input:focus {
        outline: none;
        border-color: var(--color-accent, #4da3ff);
        box-shadow: 0 0 0 2px rgba(77, 163, 255, 0.2);
    }

    .license-input.has-error {
        border-color: var(--color-error, #f44336);
    }

    .license-input:disabled {
        opacity: 0.6;
        cursor: not-allowed;
    }

    .error-message {
        margin: 0 0 16px;
        font-size: 13px;
        color: var(--color-error, #f44336);
        line-height: 1.4;
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
        background: var(--color-accent, #4da3ff);
        color: white;
        border: none;
    }

    .primary:hover:not(:disabled) {
        filter: brightness(1.1);
    }

    .secondary {
        background: transparent;
        color: var(--color-text-secondary, #aaa);
        border: 1px solid var(--color-border-primary, #444);
    }

    .secondary:hover:not(:disabled) {
        background: var(--color-bg-tertiary, #333);
        color: var(--color-text-primary, #fff);
    }
</style>
