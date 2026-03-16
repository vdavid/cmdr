<script lang="ts">
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import CommandBox from '$lib/ui/CommandBox.svelte'
    import Button from '$lib/ui/Button.svelte'

    interface Props {
        /** Called when the dialog is closed. */
        onClose: () => void
        /** Called when user wants to retry connecting. */
        onRetry: () => void
    }

    const { onClose, onRetry }: Props = $props()

    const installCommand = `echo 'SUBSYSTEM=="usb", ATTR{bInterfaceClass}=="06", MODE="0664", TAG+="uaccess"\nSUBSYSTEM=="usb", ATTR{bInterfaceClass}=="ff", ATTR{bInterfaceSubClass}=="ff", ATTR{bInterfaceProtocol}=="00", MODE="0664", TAG+="uaccess"' | sudo tee /etc/udev/rules.d/99-cmdr-mtp.rules > /dev/null && sudo udevadm control --reload-rules && sudo udevadm trigger`

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter') {
            onRetry()
        }
    }
</script>

<ModalDialog
    titleId="dialog-title"
    onkeydown={handleKeydown}
    blur
    dialogId="mtp-permission"
    onclose={onClose}
    containerStyle="min-width: 480px; max-width: 560px"
>
    {#snippet title()}Can't access USB device{/snippet}

    <div class="dialog-body">
        <p class="description">
            Cmdr doesn't have permission to access this device. Linux needs udev rules to grant MTP device access.
        </p>

        <p class="explanation">Run this command in your terminal to install the rules and reload them:</p>

        <div class="command-wrapper">
            <CommandBox command={installCommand} />
        </div>

        <p class="help-text">After running the command, unplug and replug the device, then retry.</p>

        <div class="actions">
            <Button variant="secondary" onclick={onClose}>Close</Button>
            <Button variant="primary" onclick={onRetry}>Retry connection</Button>
        </div>
    </div>
</ModalDialog>

<style>
    .dialog-body {
        padding: 0 var(--spacing-2xl) var(--spacing-xl);
    }

    .description {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: 1.5;
    }

    .explanation {
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-md);
        color: var(--color-text-tertiary);
        line-height: 1.6;
    }

    .command-wrapper {
        margin-bottom: var(--spacing-md);
    }

    .help-text {
        margin: 0 0 var(--spacing-xl);
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        line-height: 1.5;
    }

    .actions {
        display: flex;
        gap: var(--spacing-md);
        justify-content: flex-end;
    }
</style>
