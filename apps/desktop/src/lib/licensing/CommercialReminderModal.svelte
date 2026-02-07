<script lang="ts">
    import { markCommercialReminderDismissed, openExternalUrl } from '$lib/tauri-commands'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'

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
            <button class="primary" onclick={handleGetCommercial}>Get commercial license</button>
            <button class="secondary" onclick={handleDismiss}>Remind me in 30 days</button>
        </div>
    </div>
</ModalDialog>

<style>
    .modal-body {
        padding: 0 32px 24px;
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
