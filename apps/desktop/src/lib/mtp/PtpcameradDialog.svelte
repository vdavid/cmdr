<script lang="ts">
    import { onMount, tick } from 'svelte'
    import { copyToClipboard, getPtpcameradWorkaroundCommand } from '$lib/tauri-commands'

    interface Props {
        /** The process name that's blocking (e.g., "pid 45145, ptpcamerad"). */
        blockingProcess?: string
        /** Called when the dialog is closed. */
        onClose: () => void
        /** Called when user wants to retry connecting. */
        onRetry: () => void
    }

    const { blockingProcess, onClose, onRetry }: Props = $props()

    let workaroundCommand = $state('')
    let copied = $state(false)
    let overlayElement: HTMLDivElement | undefined = $state()

    onMount(async () => {
        // Get the workaround command from backend
        workaroundCommand = await getPtpcameradWorkaroundCommand()

        // Focus overlay so keyboard events work immediately
        await tick()
        overlayElement?.focus()
    })

    async function handleCopyCommand() {
        if (!workaroundCommand) return

        try {
            await copyToClipboard(workaroundCommand)
            copied = true
            // Reset after 2 seconds
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
        // Stop propagation to prevent file explorer from handling keys
        event.stopPropagation()
        if (event.key === 'Escape') {
            onClose()
        } else if (event.key === 'Enter') {
            onRetry()
        }
    }
</script>

<div
    bind:this={overlayElement}
    class="modal-overlay"
    role="dialog"
    aria-modal="true"
    aria-labelledby="dialog-title"
    tabindex="-1"
    onkeydown={handleKeydown}
>
    <div class="modal-content">
        <button class="close-button" onclick={onClose} aria-label="Close">×</button>

        <h2 id="dialog-title">Can't connect to MTP device</h2>

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
            <button class="secondary" onclick={onClose}>Close</button>
            <button class="primary" onclick={onRetry}>Retry connection</button>
        </div>
    </div>
</div>

<style>
    .modal-overlay {
        position: fixed;
        inset: 0;
        background: rgba(0, 0, 0, 0.6);
        display: flex;
        align-items: center;
        justify-content: center;
        z-index: 9999;
        backdrop-filter: blur(4px);
    }

    .modal-content {
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border-primary);
        border-radius: 12px;
        padding: 24px 32px;
        min-width: 480px;
        max-width: 560px;
        box-shadow: 0 16px 48px rgba(0, 0, 0, 0.4);
        position: relative;
    }

    .close-button {
        position: absolute;
        top: 12px;
        right: 12px;
        background: none;
        border: none;
        color: var(--color-text-secondary);
        font-size: 24px;
        cursor: pointer;
        padding: 4px 8px;
        line-height: 1;
        border-radius: 4px;
    }

    .close-button:hover {
        background: var(--color-button-hover);
        color: var(--color-text-primary);
    }

    h2 {
        margin: 0 0 12px;
        font-size: 18px;
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .description {
        margin: 0 0 12px;
        font-size: 14px;
        color: var(--color-text-secondary);
        line-height: 1.5;
    }

    .description strong {
        color: var(--color-text-primary);
        font-weight: 500;
    }

    .explanation {
        margin: 0 0 16px;
        font-size: 13px;
        color: var(--color-text-muted);
        line-height: 1.6;
    }

    .explanation code {
        background: var(--color-bg-tertiary);
        padding: 2px 6px;
        border-radius: 4px;
        font-family: 'SF Mono', Menlo, Monaco, Consolas, monospace;
        font-size: 12px;
    }

    .command-box {
        display: flex;
        align-items: stretch;
        gap: 8px;
        margin-bottom: 12px;
        background: var(--color-bg-primary);
        border: 1px solid var(--color-border-primary);
        border-radius: 8px;
        padding: 12px;
    }

    .command {
        flex: 1;
        font-family: 'SF Mono', Menlo, Monaco, Consolas, monospace;
        font-size: 12px;
        color: var(--color-text-primary);
        word-break: break-all;
        line-height: 1.5;
        background: none;
        padding: 0;
    }

    .copy-button {
        flex-shrink: 0;
        padding: 6px 12px;
        font-size: 12px;
        font-weight: 500;
        border-radius: 4px;
        cursor: pointer;
        transition: all 0.15s ease;
        background: var(--color-bg-tertiary);
        color: var(--color-text-secondary);
        border: 1px solid var(--color-border-primary);
    }

    .copy-button:hover:not(:disabled) {
        background: var(--color-button-hover);
        color: var(--color-text-primary);
    }

    .copy-button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .help-text {
        margin: 0 0 20px;
        font-size: 12px;
        color: var(--color-text-muted);
        line-height: 1.5;
    }

    .help-text kbd {
        background: var(--color-bg-tertiary);
        padding: 2px 6px;
        border-radius: 4px;
        font-family: var(--font-system);
        font-size: 11px;
        border: 1px solid var(--color-border-primary);
    }

    .actions {
        display: flex;
        gap: 12px;
        justify-content: flex-end;
    }

    button {
        padding: 10px 18px;
        border-radius: 6px;
        font-size: 14px;
        font-weight: 500;
        cursor: pointer;
        transition: all 0.15s ease;
    }

    button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .primary {
        background: var(--color-accent);
        color: white;
        border: none;
    }

    .primary:hover:not(:disabled) {
        filter: brightness(1.1);
    }

    .secondary {
        background: transparent;
        color: var(--color-text-secondary);
        border: 1px solid var(--color-border-primary);
    }

    .secondary:hover:not(:disabled) {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }
</style>
