<script lang="ts">
    import { onMount } from 'svelte'
    import { copyToClipboard, getPtpcameradWorkaroundCommand } from '$lib/tauri-commands'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
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
    let copied = $state(false)

    onMount(async () => {
        workaroundCommand = await getPtpcameradWorkaroundCommand()
    })

    async function handleCopyCommand() {
        if (!workaroundCommand) return

        try {
            await copyToClipboard(workaroundCommand)
            copied = true
            setTimeout(() => {
                copied = false
            }, 2000)
        } catch {
            // Fallback to browser clipboard API
            await navigator.clipboard.writeText(workaroundCommand)
            copied = true
            setTimeout(() => {
                copied = false
            }, 2000)
        }
    }

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

        <div class="command-box">
            <code class="command">{workaroundCommand}</code>
            <button
                class="copy-button"
                onclick={handleCopyCommand}
                disabled={!workaroundCommand}
                aria-label="Copy command to clipboard"
            >
                {copied ? 'Copied!' : 'Copy'}
            </button>
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
        padding: 2px var(--spacing-xs);
        border-radius: var(--radius-sm);
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
    }

    .command-box {
        display: flex;
        align-items: stretch;
        gap: 8px;
        margin-bottom: 12px;
        background: var(--color-bg-primary);
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-lg);
        padding: 12px;
    }

    .command {
        flex: 1;
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        word-break: break-all;
        line-height: 1.5;
        background: none;
        padding: 0;
    }

    .copy-button {
        flex-shrink: 0;
        padding: var(--spacing-sm) var(--spacing-md);
        font-size: var(--font-size-sm);
        font-weight: 500;
        border-radius: var(--radius-sm);
        cursor: pointer;
        transition: all var(--transition-base);
        background: var(--color-bg-tertiary);
        color: var(--color-text-secondary);
        border: 1px solid var(--color-border-strong);
    }

    .copy-button:hover:not(:disabled) {
        background: var(--color-accent-subtle);
        color: var(--color-text-primary);
    }

    .copy-button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .help-text {
        margin: 0 0 20px;
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        line-height: 1.5;
    }

    .help-text kbd {
        background: var(--color-bg-tertiary);
        padding: 2px var(--spacing-xs);
        border-radius: var(--radius-sm);
        font-family: var(--font-system);
        font-size: var(--font-size-sm);
        border: 1px solid var(--color-border-strong);
    }

    .actions {
        display: flex;
        gap: 12px;
        justify-content: flex-end;
    }
</style>
