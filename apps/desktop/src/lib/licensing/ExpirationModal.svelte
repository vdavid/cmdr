<script lang="ts">
    import { markExpirationModalShown, openExternalUrl } from '$lib/tauri-commands'

    interface Props {
        organizationName: string | null
        expiredAt: string
        onClose: () => void
    }

    const { organizationName, expiredAt, onClose }: Props = $props()

    function formatDate(isoString: string): string {
        try {
            const date = new Date(isoString)
            return date.toLocaleDateString(undefined, { year: 'numeric', month: 'long', day: 'numeric' })
        } catch {
            return expiredAt
        }
    }

    async function handleDismiss() {
        await markExpirationModalShown()
        onClose()
    }

    async function handleRenew() {
        // Open renew page in the system browser
        await openExternalUrl('https://getcmdr.com/renew')
    }
</script>

<div class="modal-overlay" role="dialog" aria-modal="true" aria-labelledby="modal-title">
    <div class="modal-content">
        <h2 id="modal-title">Your commercial license has expired</h2>

        {#if organizationName}
            <p class="org-name">License for: <strong>{organizationName}</strong></p>
        {/if}

        <p class="message">
            Your commercial subscription expired on <strong>{formatDate(expiredAt)}</strong>.
        </p>

        <p class="info">
            Cmdr is now running in personal use mode. If you're still using it for work, please renew your license.
        </p>

        <div class="actions">
            <button class="primary" onclick={handleRenew}>Renew license</button>
            <button class="secondary" onclick={handleDismiss}>Continue in personal mode</button>
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
        background: var(--color-surface, #1e1e2e);
        border: 1px solid var(--color-border, #363646);
        border-radius: 12px;
        padding: 24px 32px;
        max-width: 420px;
        box-shadow: 0 16px 48px rgba(0, 0, 0, 0.4);
    }

    h2 {
        margin: 0 0 16px;
        font-size: 18px;
        font-weight: 600;
        color: var(--color-text-primary, #fff);
    }

    .org-name {
        margin: 0 0 12px;
        font-size: 14px;
        color: var(--color-text-secondary, #a0a0b0);
    }

    .message {
        margin: 0 0 8px;
        font-size: 14px;
        color: var(--color-text-secondary, #a0a0b0);
        line-height: 1.5;
    }

    .info {
        margin: 0 0 20px;
        font-size: 13px;
        color: var(--color-text-tertiary, #707080);
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
        background: var(--color-accent, #6366f1);
        color: white;
        border: none;
    }

    .primary:hover {
        background: var(--color-accent-hover, #5558e8);
    }

    .secondary {
        background: transparent;
        color: var(--color-text-secondary, #a0a0b0);
        border: 1px solid var(--color-border, #363646);
    }

    .secondary:hover {
        background: var(--color-surface-hover, #262636);
        color: var(--color-text-primary, #fff);
    }
</style>
