<script module lang="ts">
    // Module-level state: set by +layout.svelte before addToast is called
    let lastConnectedDeviceName = $state('MTP device')

    export function setLastConnectedDeviceName(name: string) {
        lastConnectedDeviceName = name
    }
</script>

<script lang="ts">
    import { dismissToast } from '$lib/ui/toast'
    import { setSetting } from '$lib/settings'
    import { isMacOS } from '$lib/shortcuts/key-capture'

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
    <p class="title">Connected to {lastConnectedDeviceName}</p>
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
        <button class="ok-button" onclick={handleOk}>OK</button>
        <button class="disable-link" onclick={handleDisableMtp}>Disable MTP...</button>
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
        align-items: center;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-xs);
    }

    .ok-button {
        background: var(--color-accent);
        color: var(--color-accent-fg);
        border: none;
        border-radius: var(--radius-sm);
        padding: var(--spacing-xs) var(--spacing-md);
        font-size: var(--font-size-xs);
        font-weight: 500;
        cursor: default;
        transition: background var(--transition-fast);
    }

    .ok-button:hover {
        background: var(--color-accent-hover);
    }

    .disable-link {
        background: none;
        border: none;
        padding: 0;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        cursor: default;
        transition: color var(--transition-fast);
    }

    .disable-link:hover {
        color: var(--color-text-secondary);
    }
</style>
