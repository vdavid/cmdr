<script lang="ts">
    import { onMount } from 'svelte'
    import { getPtpcameradWorkaroundCommand } from '$lib/tauri-commands'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import CommandBox from '$lib/ui/CommandBox.svelte'
    import Button from '$lib/ui/Button.svelte'

    interface Props {
        /** The process name that's blocking (like "pid 45145, ptpcamerad"). */
        blockingProcess?: string
        /** Called when the dialog is closed. */
        onClose: () => void
        /** Called when user wants to retry connecting. */
        onRetry: () => void
    }

    const { blockingProcess, onClose, onRetry }: Props = $props()

    let workaroundCommand = $state('')

    onMount(async () => {
        workaroundCommand = await getPtpcameradWorkaroundCommand()
    })

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
    dialogId="ptpcamerad"
    onclose={onClose}
    containerStyle="min-width: 480px; max-width: 560px"
>
    {#snippet title()}Can't connect to MTP device{/snippet}

    <div class="dialog-body">
        <p class="description">
            {#if blockingProcess}
                The device is in use by <strong>{blockingProcess}</strong>.
            {:else}
                Another process has exclusive access to the device.
            {/if}
        </p>

        <p class="explanation">
            On macOS, the system daemon <code>ptpcamerad</code> automatically claims Android devices. To work around this,
            run the following command in Terminal (keep it running while using Cmdr):
        </p>

        <div class="command-wrapper">
            {#if workaroundCommand}
                <CommandBox command={workaroundCommand} />
            {/if}
        </div>

        <p class="help-text">
            This command continuously stops ptpcamerad while running. Press <kbd>Ctrl+C</kbd> in Terminal to stop it when
            done.
        </p>

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

    .description strong {
        color: var(--color-text-primary);
        font-weight: 500;
    }

    .explanation {
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-md);
        color: var(--color-text-tertiary);
        line-height: 1.6;
    }

    .explanation code {
        background: var(--color-bg-tertiary);
        padding: var(--spacing-xxs) var(--spacing-xs);
        border-radius: var(--radius-sm);
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
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

    .help-text kbd {
        background: var(--color-bg-tertiary);
        padding: var(--spacing-xxs) var(--spacing-xs);
        border-radius: var(--radius-sm);
        font-family: var(--font-system);
        font-size: var(--font-size-sm);
        border: 1px solid var(--color-border-strong);
    }

    .actions {
        display: flex;
        gap: var(--spacing-md);
        justify-content: flex-end;
    }
</style>
