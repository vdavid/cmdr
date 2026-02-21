<script lang="ts">
    import { getUpdateState, restartToUpdate } from './updater.svelte'

    const updateState = getUpdateState()

    function handleRestart() {
        void restartToUpdate()
    }

    function handleDismiss() {
        dismissed = true
    }

    let dismissed = $state(false)
</script>

{#if updateState.status === 'ready' && !dismissed}
    <div class="update-notification">
        <span class="update-text">New version available. Restart to update.</span>
        <div class="update-actions">
            <button class="update-button restart" onclick={handleRestart}>Restart</button>
            <button class="update-button later" onclick={handleDismiss}>Later</button>
        </div>
    </div>
{/if}

<style>
    .update-notification {
        position: fixed;
        top: var(--spacing-lg);
        right: var(--spacing-lg);
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-lg);
        padding: var(--spacing-sm) var(--spacing-md);
        display: flex;
        align-items: center;
        gap: var(--spacing-lg);
        box-shadow: var(--shadow-md);
        z-index: var(--z-notification);
    }

    .update-text {
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
    }

    .update-actions {
        display: flex;
        gap: var(--spacing-xs);
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
