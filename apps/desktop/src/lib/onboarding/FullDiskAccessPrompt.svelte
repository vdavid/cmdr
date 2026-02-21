<script lang="ts">
    import { openPrivacySettings } from '$lib/tauri-commands'
    import { saveSettings } from '$lib/settings-store'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'

    interface Props {
        onComplete: () => void
        wasRevoked?: boolean
    }

    const { onComplete, wasRevoked = false }: Props = $props()

    let hasClickedOpenSettings = $state(false)

    async function handleOpenSettings() {
        hasClickedOpenSettings = true
        await openPrivacySettings()
    }

    async function handleDeny() {
        await saveSettings({ fullDiskAccessChoice: 'deny' })
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
            <li>Click <strong>Full Disk Access</strong> in the list</li>
            <li>Find <strong>Cmdr</strong> in the list and toggle it on</li>
            <li>Confirm it and click <strong>Quit & Reopen</strong></li>
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
