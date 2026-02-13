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
        top: var(--spacing-md);
        right: var(--spacing-md);
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border);
        border-radius: 8px;
        padding: var(--spacing-sm) var(--spacing-md);
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
        z-index: 9998;
        max-width: 360px;
        animation: slide-in 0.2s ease-out;
    }

    @media (prefers-color-scheme: dark) {
        .notification {
            box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
        }
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
        color: var(--color-text-muted);
        font-size: 14px;
        cursor: pointer;
        padding: 2px 6px;
        border-radius: 4px;
        line-height: 1;
    }

    .notification-close:hover {
        background: var(--color-button-hover);
        color: var(--color-text-primary);
    }
</style>
