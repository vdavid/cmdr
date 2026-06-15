<script lang="ts">
    import { dismissToast } from '$lib/ui/toast'
    import Button from '$lib/ui/Button.svelte'
    import { setSetting } from '$lib/settings'
    import { isMacOS } from '$lib/shortcuts/key-capture'
    import { getLastConnectedDeviceName } from './mtp-connected-toast-state.svelte'

    const toastId = 'mtp-connected'
    let dontShowAgain = $state(false)

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

<div class="mtp-toast">
    <p class="title">Connected to {getLastConnectedDeviceName()}</p>
    <p class="body">
        {#if isMacOS()}
            Cmdr paused the macOS camera daemon (ptpcamerad) to access this device. To use it in another app, disable
            MTP support in settings.
        {:else}
            To use this device in another app, disable MTP support in settings.
        {/if}
    </p>
    <label class="dont-show-again">
        <input type="checkbox" bind:checked={dontShowAgain} />
        Don't show again
    </label>
    <div class="actions">
        <Button size="mini" variant="secondary" onclick={handleDisableMtp}>Disable MTP...</Button>
        <Button size="mini" variant="primary" onclick={handleOk}>OK</Button>
    </div>
</div>

<style>
    .mtp-toast {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
    }

    .title {
        margin: 0;
        font-weight: 600;
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
    }

    .body {
        margin: 0;
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        line-height: 1.4;
    }

    .dont-show-again {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        cursor: default;
        margin-top: var(--spacing-xs);
    }

    .dont-show-again input[type='checkbox'] {
        margin: 0;
        cursor: default;
    }

    .actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-md);
    }
</style>
