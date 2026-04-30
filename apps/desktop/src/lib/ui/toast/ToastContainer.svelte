<script lang="ts">
    import { getToasts, dismissToast } from './toast-store.svelte'
    import ToastItem from './ToastItem.svelte'

    const toasts = getToasts()

    function handleUserDismiss(id: string): void {
        const toast = toasts.find((t) => t.id === id)
        toast?.onDismiss?.()
        dismissToast(id)
    }
</script>

{#if toasts.length > 0}
    <div class="toast-container" aria-live="polite">
        {#each toasts as toast (toast.id)}
            <ToastItem
                id={toast.id}
                content={toast.content}
                level={toast.level}
                dismissal={toast.dismissal}
                timeoutMs={toast.timeoutMs}
                closeTooltip={toast.closeTooltip}
                onTimeout={dismissToast}
                onUserDismiss={handleUserDismiss}
            />
        {/each}
    </div>
{/if}

<style>
    .toast-container {
        position: fixed;
        top: var(--spacing-lg);
        right: var(--spacing-lg);
        z-index: var(--z-notification);
        display: flex;
        flex-direction: column-reverse;
        gap: var(--spacing-sm);
        max-width: 360px;
        pointer-events: none;
    }

    .toast-container > :global(*) {
        pointer-events: auto;
    }
</style>
