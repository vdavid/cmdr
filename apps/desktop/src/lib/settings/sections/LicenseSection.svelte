<script lang="ts">
    import { onMount } from 'svelte'
    import { emitTo } from '@tauri-apps/api/event'
    import { getCurrentWindow } from '@tauri-apps/api/window'
    import SettingsSection from '../components/SettingsSection.svelte'
    import {
        openExternalUrl,
        getLicenseInfo,
        getLicenseStatus,
        type LicenseInfo,
        type LicenseStatus,
    } from '$lib/tauri-commands'
    import Button from '$lib/ui/Button.svelte'
    import { getLicenseTypeLabel, getStatusText } from './license-section-utils'

    let licenseInfo = $state<LicenseInfo | null>(null)
    let licenseStatus = $state<LicenseStatus | null>(null)
    let isLoading = $state(true)

    onMount(async () => {
        try {
            const [info, status] = await Promise.all([
                getLicenseInfo().catch(() => null),
                getLicenseStatus().catch(() => null),
            ])
            licenseInfo = info
            licenseStatus = status
        } finally {
            isLoading = false
        }
    })

    const hasLicense = $derived(licenseInfo !== null)
    const statusText = $derived(getStatusText(licenseStatus))

    async function handleManageLicense() {
        await emitTo('main', 'execute-command', { commandId: 'app.licenseKey' })
        await getCurrentWindow().close()
    }

    async function handleBuyLicense() {
        await openExternalUrl('https://getcmdr.com/pricing')
    }
</script>

<SettingsSection title="License">
    {#if isLoading}
        <p class="loading-text">Loading...</p>
    {:else}
        <div class="license-info">
            <div class="info-row">
                <span class="info-label">License type</span>
                <span class="info-value">{getLicenseTypeLabel(licenseInfo)}</span>
            </div>
            {#if licenseInfo?.organizationName}
                <div class="info-row">
                    <span class="info-label">Organization</span>
                    <span class="info-value">{licenseInfo.organizationName}</span>
                </div>
            {/if}
            {#if statusText}
                <div class="info-row">
                    <span class="info-label">Status</span>
                    <span
                        class="info-value"
                        class:status-expired={licenseStatus?.type === 'expired'}
                        class:status-active={licenseStatus?.type === 'commercial' ||
                            licenseStatus?.type === 'supporter'}>{statusText}</span
                    >
                </div>
            {/if}
            {#if licenseInfo?.shortCode}
                <div class="info-row">
                    <span class="info-label">License key</span>
                    <span class="info-value mono">{licenseInfo.shortCode}</span>
                </div>
            {/if}
        </div>

        <div class="actions">
            {#if hasLicense}
                <Button variant="secondary" onclick={handleManageLicense}>Manage license key</Button>
            {:else}
                <Button variant="secondary" onclick={handleManageLicense}>Enter license key</Button>
                <Button variant="secondary" onclick={handleBuyLicense}>Get a license</Button>
            {/if}
        </div>
    {/if}
</SettingsSection>

<style>
    .loading-text {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        margin: 0;
    }

    .license-info {
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-lg);
        padding: var(--spacing-xs) var(--spacing-lg);
        margin-bottom: var(--spacing-lg);
    }

    .info-row {
        display: flex;
        justify-content: space-between;
        align-items: center;
        gap: var(--spacing-xl);
        padding: var(--spacing-sm) 0;
    }

    .info-row:not(:last-child) {
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .info-label {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        flex-shrink: 0;
    }

    .info-value {
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        font-weight: 500;
        text-align: right;
    }

    .info-value.mono {
        font-family: var(--font-mono);
        letter-spacing: 0.02em;
    }

    .info-value.status-expired {
        color: var(--color-error);
    }

    .info-value.status-active {
        color: var(--color-toast-success-stripe);
    }

    .actions {
        display: flex;
        gap: var(--spacing-md);
    }
</style>
