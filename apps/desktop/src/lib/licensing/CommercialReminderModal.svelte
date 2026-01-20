<script lang="ts">
    import { markCommercialReminderDismissed, openExternalUrl } from '$lib/tauri-commands'

    interface Props {
        onClose: () => void
    }

    const { onClose }: Props = $props()

    async function handleDismiss() {
        await markCommercialReminderDismissed()
        onClose()
    }

    async function handleGetCommercial() {
        // Open commercial license page in the system browser
        await openExternalUrl('https://getcmdr.com/commercial')
    }

    function handleKeydown(event: KeyboardEvent) {
        // Stop propagation to prevent file explorer from handling keys while modal is open
        event.stopPropagation()
        if (event.key === 'Escape') {
            void handleDismiss()
        }
    }
</script>

<div class="modal-overlay" role="dialog" aria-modal="true" aria-labelledby="modal-title" onkeydown={handleKeydown}>
    <div class="modal-content">
        <h2 id="modal-title">Thanks for using Cmdr!</h2>

        <p class="message">
            You're using a Personal license. If you're using Cmdr at work, please get a Commercial license to stay
            compliant.
        </p>

        <p class="info">Commercial licenses are $59/year/user and support continued development.</p>

        <div class="actions">
            <button class="primary" onclick={handleGetCommercial}>Get commercial license</button>
            <button class="secondary" onclick={handleDismiss}>Remind me in 30 days</button>
        </div>
    </div>
</div>

<style>
    .modal-overlay {
        position: fixed;
        inset: 0;
        background: rgba(0, 0, 0, 0.6);
        display: flex;
        align-items: center;
        justify-content: center;
        z-index: 9999;
        backdrop-filter: blur(4px);
    }

    .modal-content {
        background: var(--color-bg-primary);
        border: 1px solid var(--color-border);
        border-radius: 12px;
        padding: 24px 32px;
        max-width: 420px;
        box-shadow: 0 16px 48px rgba(0, 0, 0, 0.4);
    }

    h2 {
        margin: 0 0 16px;
        font-size: 18px;
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .message {
        margin: 0 0 8px;
        font-size: 14px;
        color: var(--color-text-secondary);
        line-height: 1.5;
    }

    .info {
        margin: 0 0 20px;
        font-size: 13px;
        color: var(--color-text-tertiary);
        line-height: 1.5;
    }

    .actions {
        display: flex;
        gap: 12px;
        justify-content: flex-end;
    }

    button {
        padding: 8px 16px;
        border-radius: 6px;
        font-size: 13px;
        font-weight: 500;
        cursor: pointer;
        transition: all 0.15s ease;
    }

    .primary {
        background: var(--color-accent);
        color: white;
        border: none;
    }

    .primary:hover {
        background: var(--color-accent-hover);
    }

    .secondary {
        background: transparent;
        color: var(--color-text-secondary);
        border: 1px solid var(--color-border);
    }

    .secondary:hover {
        background: var(--color-bg-secondary);
        color: var(--color-text-primary);
    }
</style>
