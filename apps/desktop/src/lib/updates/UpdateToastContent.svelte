<script lang="ts">
    import { restartToUpdate } from './updater.svelte'
    import { dismissToast } from '$lib/ui/toast'

    function handleRestart() {
        void restartToUpdate()
    }

    function handleDismiss() {
        dismissToast('update')
    }
</script>

<span class="update-text">New version available. Restart to update.</span>
<div class="update-actions">
    <button class="update-button restart" onclick={handleRestart}>Restart</button>
    <button class="update-button later" onclick={handleDismiss}>Later</button>
</div>

<style>
    .update-text {
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
    }

    .update-actions {
        display: flex;
        gap: var(--spacing-xs);
        margin-top: var(--spacing-xs);
    }

    .update-button {
        padding: var(--spacing-xs) var(--spacing-sm);
        border-radius: var(--radius-md);
        font-size: var(--font-size-sm);
        cursor: pointer;
        border: none;
    }

    .update-button.restart {
        background: var(--color-accent);
        color: #fff;
    }

    .update-button.restart:hover {
        filter: brightness(1.1);
    }

    .update-button.later {
        background: transparent;
        color: var(--color-text-secondary);
    }

    .update-button.later:hover {
        background: var(--color-bg-tertiary);
    }
</style>
