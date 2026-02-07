<script lang="ts">
    import ModalDialog from '$lib/ui/ModalDialog.svelte'

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
            <button class="primary" onclick={onClose}>{buttonText}</button>
        </div>
    </div>
</ModalDialog>

<style>
    .alert-body {
        padding: 0 24px 20px;
    }

    .message {
        margin: 0 0 16px;
        font-size: 13px;
        color: var(--color-text-secondary);
        text-align: center;
        line-height: 1.5;
    }

    .button-row {
        display: flex;
        justify-content: center;
    }

    button {
        padding: 8px 20px;
        border-radius: 6px;
        font-size: 13px;
        font-weight: 500;
        cursor: pointer;
        transition: all 0.15s ease;
        min-width: 80px;
    }

    .primary {
        background: var(--color-accent);
        color: white;
        border: none;
    }

    .primary:hover {
        filter: brightness(1.1);
    }
</style>
