<script lang="ts">
    import { dismissToast } from '$lib/ui/toast'
    import { openPrivacySettings } from '$lib/tauri-commands'
    import { REVEAL_FDA_TOAST_ID } from './reveal-ids'

    async function handleOpenSystemSettings() {
        await openPrivacySettings()
        dismissToast(REVEAL_FDA_TOAST_ID)
    }

    function handleDismiss() {
        dismissToast(REVEAL_FDA_TOAST_ID)
    }
</script>

<div class="content">
    <span class="message">Cmdr needs Full Disk Access to watch your Downloads folder.</span>
    <div class="actions">
        <button class="link-button" onclick={handleOpenSystemSettings}>Open System Settings</button>
        <button class="link-button" onclick={handleDismiss}>Dismiss</button>
    </div>
</div>

<style>
    .content {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
    }

    .message {
        color: var(--color-text-primary);
        line-height: 1.4;
    }

    .actions {
        display: flex;
        gap: var(--spacing-md);
    }

    .link-button {
        background: none;
        border: none;
        padding: 0;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .link-button:hover {
        color: var(--color-text-secondary);
    }
</style>
