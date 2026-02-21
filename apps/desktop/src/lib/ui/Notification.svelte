<script lang="ts">
    type NotificationStyle = 'info' | 'error'

    interface Props {
        message: string
        style?: NotificationStyle
        onclose: () => void
    }

    const { message, style = 'info', onclose }: Props = $props()
</script>

<div class="notification" class:error={style === 'error'} role="alert">
    <span class="notification-message">{message}</span>
    <button class="notification-close" onclick={onclose} aria-label="Close notification">x</button>
</div>

<style>
    .notification {
        position: fixed;
        top: var(--spacing-lg);
        right: var(--spacing-lg);
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-lg);
        padding: var(--spacing-sm) var(--spacing-md);
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        box-shadow: var(--shadow-md);
        z-index: var(--z-notification);
        max-width: 360px;
        animation: slide-in 0.2s ease-out;
    }

    .notification.error {
        border-color: var(--color-error-border);
        background: var(--color-error-bg);
    }

    @keyframes slide-in {
        from {
            opacity: 0;
            transform: translateX(20px);
        }
        to {
            opacity: 1;
            transform: translateX(0);
        }
    }

    .notification-message {
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        line-height: 1.4;
    }

    .error .notification-message {
        color: var(--color-error);
    }

    .notification-close {
        flex-shrink: 0;
        background: none;
        border: none;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-md);
        cursor: pointer;
        padding: 2px var(--spacing-xs);
        border-radius: var(--radius-sm);
        line-height: 1;
    }

    .notification-close:hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }
</style>
