<script lang="ts">
    import { markExpirationModalShown, openExternalUrl } from '$lib/tauri-commands'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'

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
        await openExternalUrl('https://getcmdr.com/renew')
    }
</script>

<ModalDialog
    titleId="modal-title"
    blur
    dialogId="expiration"
    onclose={() => {
        void handleDismiss()
    }}
    containerStyle="max-width: 420px; background: var(--color-bg-primary); border-color: var(--color-border)"
>
    {#snippet title()}Your commercial license has expired{/snippet}

    <div class="modal-body">
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
</ModalDialog>

<style>
    .modal-body {
        padding: 0 32px 24px;
    }

    .org-name {
        margin: 0 0 12px;
        font-size: 14px;
        color: var(--color-text-secondary);
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
