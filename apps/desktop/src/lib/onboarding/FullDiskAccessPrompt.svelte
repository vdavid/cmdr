<script lang="ts">
    import { openPrivacySettings } from '$lib/tauri-commands'
    import { saveSettings } from '$lib/settings-store'

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

<div class="fda-prompt">
    <div class="content">
        <h1>Full disk access</h1>

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
            <li class="pro">
                <strong>Pro:</strong> The app will access your entire disk without nagging you for permissions to each folder
                like Downloads, Documents, and Desktop.
            </li>
            <li class="con">
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
            <button class="allow" onclick={handleOpenSettings}>Open System Settings</button>
            <button class="deny" onclick={handleDeny}>Deny</button>
        </div>
        {#if hasClickedOpenSettings}
            <p class="post-allow-instructions">Great! Make sure to restart the app after you've enabled the access.</p>
            <p>If you change your mind, you can still click "Deny" above.</p>
        {/if}
    </div>
</div>

<style>
    .fda-prompt {
        position: fixed;
        inset: 0;
        display: flex;
        align-items: center;
        justify-content: center;
        line-height: 24px;
        /*background: var(--color-bg-primary);*/
        /*font-family: -apple-system, BlinkMacSystemFont, 'SF Pro Text', 'Helvetica Neue', Arial, sans-serif;*/
        color: var(--color-text-primary);
    }

    .content {
        width: 480px;
        font-size: 14px;
    }

    h1 {
        font-size: 24px;
        font-weight: 600;
        margin: 0 0 24px 0;
        color: var(--color-text-primary);
    }

    p {
        line-height: 1.6;
        margin: 0 0 12px 0;
    }

    .post-allow-instructions {
        font-weight: 500;
    }

    .pros-cons {
        margin: 16px 0;
    }

    .pros-cons li {
        margin-bottom: 12px;
    }

    .buttons {
        display: flex;
        gap: 24px;
        justify-content: center;
        margin: 24px 0;
    }

    button {
        padding: 10px 24px;
        border-radius: 8px;
        font-size: 14px;
        font-weight: 500;
        cursor: pointer;
        border: none;
    }

    button:hover {
        filter: brightness(1.1);
    }

    button.allow {
        background: var(--color-allow);
        color: white;
    }

    button.deny {
        background: var(--color-error);
        color: white;
    }
</style>
