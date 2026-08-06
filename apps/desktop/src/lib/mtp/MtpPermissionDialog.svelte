<script lang="ts">
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import CopyBox from '$lib/ui/CopyBox.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { tString } from '$lib/intl/messages.svelte'

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
    {#snippet title()}{tString('mtp.permissionDialog.title')}{/snippet}

    <div class="dialog-body">
        <p class="description">
            {tString('mtp.permissionDialog.description')}
        </p>

        <p class="explanation">{tString('mtp.permissionDialog.explanation')}</p>

        <div class="command-wrapper">
            <CopyBox text={installCommand} />
        </div>

        <p class="help-text">{tString('mtp.permissionDialog.helpText')}</p>
    </div>

    {#snippet footer()}
        <Button variant="secondary" onclick={onClose}>{tString('mtp.permissionDialog.close')}</Button>
        <Button variant="primary" onclick={onRetry}>{tString('mtp.permissionDialog.retry')}</Button>
    {/snippet}
</ModalDialog>

<style>
    .description {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: var(--font-line-height-prose);
    }

    .explanation {
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-md);
        color: var(--color-text-tertiary);
        line-height: var(--font-line-height-prose);
    }

    .command-wrapper {
        margin-bottom: var(--spacing-md);
    }

    .help-text {
        margin: 0 0 var(--spacing-xl);
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        line-height: var(--font-line-height-prose);
    }
</style>
