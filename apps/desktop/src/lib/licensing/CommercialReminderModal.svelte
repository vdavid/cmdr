<script lang="ts">
    import { markCommercialReminderDismissed, openExternalUrl } from '$lib/tauri-commands'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'

    interface Props {
        onClose: () => void
    }

    const { onClose }: Props = $props()

    async function handleDismiss() {
        await markCommercialReminderDismissed()
        onClose()
    }

    async function handleGetCommercial() {
        await openExternalUrl('https://getcmdr.com/commercial')
    }
</script>

<ModalDialog
    titleId="modal-title"
    blur
    dialogId="commercial-reminder"
    onclose={() => {
        void handleDismiss()
    }}
    containerStyle="max-width: 420px; background: var(--color-bg-primary); border-color: var(--color-border)"
>
    {#snippet title()}Thanks for using Cmdr!{/snippet}

    <div class="modal-body">
        <p class="message">
            You're using a Personal license. If you're using Cmdr at work, please get a Commercial license to stay
            compliant.
        </p>

        <p class="info">Commercial licenses are $59/year/user and support continued development.</p>

        <div class="actions">
            <Button variant="primary" onclick={handleGetCommercial}>Get commercial license</Button>
            <Button variant="secondary" onclick={handleDismiss}>Remind me in 30 days</Button>
        </div>
    </div>
</ModalDialog>

<style>
    .modal-body {
        padding: 0 var(--spacing-2xl) var(--spacing-xl);
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
