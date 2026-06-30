<script lang="ts">
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        title: string
        message: string
        buttonText?: string
        onClose: () => void
    }

    const { title: dialogTitle, message, buttonText, onClose }: Props = $props()
    const resolvedButtonText = $derived(buttonText ?? tString('ui.alertDialog.defaultButton'))

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter') {
            onClose()
        }
    }
</script>

<ModalDialog
    titleId="alert-dialog-title"
    onkeydown={handleKeydown}
    role="alertdialog"
    dialogId="alert"
    onclose={onClose}
    ariaDescribedby="alert-dialog-message"
    containerStyle="width: 360px"
>
    {#snippet title()}{dialogTitle}{/snippet}

    <p id="alert-dialog-message" class="message">{message}</p>

    {#snippet footer()}
        <Button variant="primary" onclick={onClose}>{resolvedButtonText}</Button>
    {/snippet}
</ModalDialog>

<style>
    .message {
        margin: 0;
        padding: 0 var(--spacing-xl);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: 1.5;
    }
</style>
