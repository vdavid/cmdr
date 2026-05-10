<script lang="ts">
    import { onMount } from 'svelte'
    import {
        checkFullDiskAccess,
        getMacosMajorVersion,
        openPrivacySettings,
        startIndexingAfterFdaDecision,
    } from '$lib/tauri-commands'
    import { saveSettings } from '$lib/settings-store'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { getAppLogger } from '$lib/logging/logger'

    const log = getAppLogger('onboarding')

    interface Props {
        onComplete: () => void
        wasRevoked?: boolean
    }

    const { onComplete, wasRevoked = false }: Props = $props()

    let hasClickedOpenSettings = $state(false)
    // Default to Ventura+ copy. Updated on mount once the backend reports the
    // actual macOS version. macOS 13+ shows the FDA list alphabetically;
    // macOS 12 and older append new entries at the end.
    let isVenturaOrNewer = $state(true)

    onMount(async () => {
        const major = await getMacosMajorVersion()
        if (major > 0) {
            isVenturaOrNewer = major >= 13
        }
    })

    async function handleOpenSettings() {
        hasClickedOpenSettings = true
        // Re-probe right before opening Settings so the bundle is freshly
        // registered with TCC. Without this, the Cmdr row may not appear in
        // the Full Disk Access list — TCC only adds apps that have recently
        // attempted to read a protected path.
        try {
            await checkFullDiskAccess()
        } catch (error) {
            log.warn('FDA re-probe before opening Settings failed: {error}', { error })
        }
        await openPrivacySettings()
    }

    async function handleDeny() {
        await saveSettings({ fullDiskAccessChoice: 'deny' })
        // Indexing was deferred at app launch (FDA gate). Now that the user has
        // decided, start it within this session so they don't need to restart
        // for the index to start populating.
        try {
            await startIndexingAfterFdaDecision()
        } catch (error) {
            log.warn('Failed to start indexing after FDA deny: {error}', { error })
        }
        onComplete()
    }
</script>

<ModalDialog titleId="fda-prompt-title" dialogId="full-disk-access" containerStyle="max-width: 480px">
    {#snippet title()}Full disk access{/snippet}

    <div class="fda-body">
        {#if wasRevoked}
            <p>It looks like you accepted full disk access before but then revoked it.</p>
            <p><strong>The app currently has no full disk access.</strong></p>
            <p>If that was intentional, click "Deny" and the app won't bother you again.</p>
            <p>If it <em>wasn't</em> intentional, consider allowing full disk access again.</p>
            <p>Here are the pros and cons:</p>
        {:else}
            <p>Would you like to give this app full disk access?</p>
            <p>Here's what that means:</p>
        {/if}

        <ul class="pros-cons">
            <li>
                <strong>Pro:</strong> The app will access your entire disk without nagging you for permissions to each folder
                like Downloads, Documents, and Desktop.
            </li>
            <li>
                <strong>Con:</strong> Full disk access is pretty powerful. It lets the app read any file on your Mac. Only
                grant this to apps you trust.
            </li>
        </ul>

        <p>If you decide to allow:</p>

        <ol>
            <li>Click <strong>Open System Settings</strong> below</li>
            {#if isVenturaOrNewer}
                <li>
                    Find <strong>Cmdr</strong> in the list and toggle it on
                    <p class="step-tip">
                        Tip: Is Cmdr not in the list? Click the "+" button at the bottom, and choose
                        <strong>Cmdr</strong> from your <strong>Applications</strong> folder.
                    </p>
                </li>
            {:else}
                <li>
                    Find <strong>Cmdr</strong> at the end of the list and toggle it on
                    <p class="step-tip">
                        Tip: Is Cmdr not in the list? Click the "+" button at the bottom, and choose
                        <strong>Cmdr</strong> from your <strong>Applications</strong> folder.
                    </p>
                </li>
            {/if}
            <li>Confirm and click <strong>Quit & Reopen</strong></li>
        </ol>

        <div class="buttons">
            <Button variant="primary" onclick={handleOpenSettings}>Open System Settings</Button>
            <Button variant="danger" onclick={handleDeny}>Deny</Button>
        </div>
        {#if hasClickedOpenSettings}
            <p class="post-allow-instructions">Great! Make sure to restart the app after you've enabled the access.</p>
            <p>If you change your mind, you can still click "Deny" above.</p>
        {/if}
    </div>
</ModalDialog>

<style>
    .fda-body {
        padding: 0 var(--spacing-xl) var(--spacing-xl);
        font-size: var(--font-size-md);
        color: var(--color-text-primary);
        line-height: 1.6;
    }

    .fda-body p {
        margin: 0 0 var(--spacing-md) 0;
    }

    .post-allow-instructions {
        font-weight: 500;
    }

    .step-tip {
        margin: var(--spacing-xs) 0 0 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    .pros-cons {
        margin: var(--spacing-lg) 0;
    }

    .pros-cons li {
        margin-bottom: var(--spacing-md);
    }

    .buttons {
        display: flex;
        gap: var(--spacing-md);
        justify-content: center;
        margin: var(--spacing-xl) 0;
    }
</style>
