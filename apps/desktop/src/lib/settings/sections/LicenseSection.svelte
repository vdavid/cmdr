<script lang="ts">
    import { onMount } from 'svelte'
    import { getCurrentWindow } from '@tauri-apps/api/window'
    import SettingsSection from '../components/SettingsSection.svelte'
    import {
        openExternalUrl,
        getLicenseInfo,
        getLicenseStatus,
        emitExecuteCommand,
        type LicenseInfo,
        type LicenseStatus,
    } from '$lib/tauri-commands'
    import Button from '$lib/ui/Button.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { getLicenseTypeLabel, getStatusText } from './license-section-utils'
    import { deferWindowClose } from '$lib/window-close-defer'

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
        // Cross-window `execute-command` emit: this `commandId` must stay a valid frontend
        // `COMMAND_IDS` entry (it's narrowed by `isCommandId` in `+page.svelte` before
        // dispatch). The `rust-command-id-drift.test.ts` test pins it to the registry.
        await emitExecuteCommand('app.licenseKey')
        // Defer like every other settings-window self-close: destroying the webview
        // straight from a handler risks the macOS WebKit teardown crash (and stalls
        // cross-webview IPC on webkit2gtk). See `$lib/window-close-defer`.
        const win = getCurrentWindow()
        deferWindowClose(() => {
            void win.close()
        })
    }

    async function handleBuyLicense() {
        await openExternalUrl('https://getcmdr.com/pricing')
    }
</script>

<SettingsSection title={tString('licensing.section.title')}>
    {#if isLoading}
        <p class="loading-text">{tString('licensing.section.loading')}</p>
    {:else}
        <SectionCard>
            <div class="license-info">
                <div class="info-row">
                    <span class="info-label">{tString('licensing.section.labelType')}</span>
                    <span class="info-value">{getLicenseTypeLabel(licenseInfo)}</span>
                </div>
                {#if licenseInfo?.organizationName}
                    <div class="info-row">
                        <span class="info-label">{tString('licensing.section.labelOrganization')}</span>
                        <span class="info-value">{licenseInfo.organizationName}</span>
                    </div>
                {/if}
                {#if statusText}
                    <div class="info-row">
                        <span class="info-label">{tString('licensing.section.labelStatus')}</span>
                        <span
                            class="info-value"
                            class:status-expired={licenseStatus?.type === 'expired'}
                            class:status-active={licenseStatus?.type === 'commercial'}>{statusText}</span
                        >
                    </div>
                {/if}
                {#if licenseInfo?.shortCode}
                    <div class="info-row">
                        <span class="info-label">{tString('licensing.section.labelKey')}</span>
                        <span class="info-value mono">{licenseInfo.shortCode}</span>
                    </div>
                {/if}
            </div>

            <div class="actions">
                {#if hasLicense}
                    <Button variant="secondary" onclick={handleManageLicense}
                        >{tString('licensing.section.manageKey')}</Button
                    >
                {:else}
                    <Button variant="secondary" onclick={handleManageLicense}
                        >{tString('licensing.section.enterKey')}</Button
                    >
                    <Button variant="secondary" onclick={handleBuyLicense}
                        >{tString('licensing.section.getLicense')}</Button
                    >
                {/if}
            </div>
        </SectionCard>
    {/if}
</SettingsSection>

<style>
    .loading-text {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        margin: 0;
    }

    /* No background/border/radius of its own: the wrapping SectionCard owns the
       card frame, so this is just the rows-plus-separator block inside it. */
    .license-info {
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
