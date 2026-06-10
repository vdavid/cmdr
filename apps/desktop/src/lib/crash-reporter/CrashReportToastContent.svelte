<script lang="ts">
    import { dismissToast } from '$lib/ui/toast'
    import Button from '$lib/ui/Button.svelte'
    import { openSettingsWindow } from '$lib/settings/settings-window'

    function handleOpenSettings() {
        dismissToast('crash-report-sent')
        void openSettingsWindow()
    }
</script>

<div class="content">
    <span class="message">Crash report sent. Thanks for helping improve Cmdr.</span>
    <div class="actions">
        <Button size="mini" variant="secondary" onclick={handleOpenSettings}>Change in Settings &gt; Updates</Button>
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
        justify-content: flex-end;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-md);
    }
</style>
