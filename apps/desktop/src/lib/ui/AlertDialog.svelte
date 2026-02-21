<script lang="ts">
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'

    interface Props {
        title: string
        message: string
        buttonText?: string
        onClose: () => void
    }

    const { title: dialogTitle, message, buttonText = 'OK', onClose }: Props = $props()

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

    <div class="alert-body">
        <p id="alert-dialog-message" class="message">{message}</p>
        <div class="button-row">
            <Button variant="primary" onclick={onClose}>{buttonText}</Button>
        </div>
    </div>
</ModalDialog>

<style>
    .alert-body {
        padding: 0 24px 20px;
    }

    .message {
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        text-align: center;
        line-height: 1.5;
    }

    .button-row {
        display: flex;
        justify-content: center;
    }
</style>
