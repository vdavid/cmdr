<script lang="ts">
    import { markCommercialReminderDismissed, openExternalUrl } from '$lib/tauri-commands'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        onClose: () => void
    }

    const { onClose }: Props = $props()

    async function handleDismiss() {
        await markCommercialReminderDismissed()
        onClose()
    }

    async function handleGetCommercial() {
        await openExternalUrl('https://getcmdr.com/pricing/')
    }
</script>

{#snippet lineBreak(children: import('svelte').Snippet)}<br />{@render children()}{/snippet}

<ModalDialog
    titleId="modal-title"
    blur
    dialogId="commercial-reminder"
    onclose={() => {
        void handleDismiss()
    }}
    containerStyle="max-width: 600px; background: var(--color-bg-primary); border-color: var(--color-border)"
>
    {#snippet title()}{tString('licensing.commercialReminder.title')}{/snippet}

    <div class="modal-body">
        <p class="message">{tString('licensing.commercialReminder.usingPersonal')}</p>
        <p class="message">{tString('licensing.commercialReminder.askCommercial')}</p>

        <p class="info">{tString('licensing.commercialReminder.priceInfo')}</p>

        <div class="actions">
            <Button variant="secondary" onclick={handleDismiss}>
                <Trans key="licensing.commercialReminder.declinePersonal" snippets={{ break: lineBreak }} />
            </Button>
            <Button variant="primary" onclick={handleGetCommercial}
                >{tString('licensing.commercialReminder.getCommercial')}</Button
            >
        </div>
    </div>
</ModalDialog>

<style>
    .message {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: 1.5;
    }

    .info {
        margin: 0 0 var(--spacing-xl);
        font-size: var(--font-size-md);
        color: var(--color-text-tertiary);
        line-height: 1.5;
    }

    .actions {
        display: flex;
        gap: var(--spacing-md);
    }

    .actions :global(button) {
        flex: 1;
    }
</style>
