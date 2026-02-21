<script lang="ts">
    import { markExpirationModalShown, openExternalUrl } from '$lib/tauri-commands'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'

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
            <Button variant="primary" onclick={handleRenew}>Renew license</Button>
            <Button variant="secondary" onclick={handleDismiss}>Continue in personal mode</Button>
        </div>
    </div>
</ModalDialog>

<style>
    .modal-body {
        padding: 0 var(--spacing-2xl) var(--spacing-xl);
    }

    .org-name {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
    }

    .message {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: 1.5;
    }

    .info {
        margin: 0 0 20px;
        font-size: var(--font-size-md);
        color: var(--color-text-tertiary);
        line-height: 1.5;
    }

    .actions {
        display: flex;
        gap: 12px;
        justify-content: flex-end;
    }
</style>
