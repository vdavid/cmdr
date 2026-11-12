<script lang="ts">
    import { dismissToast } from '#lib/ui/toast/index.js'
    import Button from '#lib/ui/Button.svelte'
    import ToastActions from '#lib/ui/toast/ToastActions.svelte'
    import Checkbox from '#lib/ui/Checkbox.svelte'
    import { setSetting } from '#lib/settings/index.js'
    import { tString } from '#lib/intl/messages.svelte.js'

    interface Props {
        toastId: string
        /** The device's name as the backend reported it; empty when it had none. */
        deviceName: string
    }

    const { toastId, deviceName }: Props = $props()
    let dontShowAgain = $state(false)

    // Resolved here so the fallback follows a live language switch.
    const shownName = $derived(deviceName || tString('mtp.deviceFallbackName'))

    // What Cmdr had to do to reach the device, and how to hand it back. Empty where Cmdr shares
    // the device with other apps (Windows), and then the toast has no body line.
    const body = $derived(tString('mtp.connectedToast.body'))

    function handleOk() {
        if (dontShowAgain) {
            setSetting('fileOperations.mtpConnectionWarning', false)
        }
        dismissToast(toastId)
    }

    function handleDisableMtp() {
        setSetting('fileOperations.mtpEnabled', false)
        if (dontShowAgain) {
            setSetting('fileOperations.mtpConnectionWarning', false)
        }
        dismissToast(toastId)
    }
</script>

<div>
    <p class="title">{tString('mtp.connectedToast.title', { deviceName: shownName })}</p>
    {#if body}
        <p class="body">{body}</p>
    {/if}
    <div class="dont-show-again">
        <Checkbox bind:checked={dontShowAgain}>{tString('mtp.connectedToast.dontShowAgain')}</Checkbox>
    </div>
    <ToastActions>
        <Button size="mini" variant="secondary" onclick={handleDisableMtp}>{tString('mtp.connectedToast.disableMtp')}</Button>
        <Button size="mini" variant="primary" onclick={handleOk}>{tString('mtp.connectedToast.ok')}</Button>
    </ToastActions>
</div>

<style>
    .title {
        font-weight: 600;
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
    }

    .body {
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
    }

    .dont-show-again {
        margin-top: var(--spacing-sm);
    }

    /* Keep the toast's compact scale: the house checkbox label defaults to md. */
    .dont-show-again :global(.checkbox-label) {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }
</style>
