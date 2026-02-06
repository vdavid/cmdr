<script lang="ts">
    /**
     * Simple alert dialog for showing informational messages.
     */
    import { onMount, tick } from 'svelte'

    interface Props {
        /** Dialog title */
        title: string
        /** Message to display */
        message: string
        /** Button text (defaults to "OK") */
        buttonText?: string
        /** Callback when dialog is dismissed */
        onClose: () => void
    }

    const { title, message, buttonText = 'OK', onClose }: Props = $props()

    let overlayElement: HTMLDivElement | undefined = $state()

    function handleKeydown(event: KeyboardEvent) {
        event.stopPropagation()
        if (event.key === 'Escape' || event.key === 'Enter') {
            onClose()
        }
    }

    onMount(async () => {
        await tick()
        overlayElement?.focus()
    })
</script>

<div
    bind:this={overlayElement}
    class="modal-overlay"
    role="alertdialog"
    aria-modal="true"
    aria-labelledby="alert-dialog-title"
    aria-describedby="alert-dialog-message"
    tabindex="-1"
    onkeydown={handleKeydown}
>
    <div class="alert-dialog">
        <h2 id="alert-dialog-title">{title}</h2>
        <p id="alert-dialog-message" class="message">{message}</p>
        <div class="button-row">
            <button class="primary" onclick={onClose}>{buttonText}</button>
        </div>
    </div>
</div>

<style>
    .modal-overlay {
        position: fixed;
        inset: 0;
        background: rgba(0, 0, 0, 0.4);
        display: flex;
        align-items: center;
        justify-content: center;
        z-index: 9999;
    }

    .alert-dialog {
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border-primary);
        border-radius: 12px;
        width: 360px;
        padding: 20px 24px;
        box-shadow: 0 16px 48px rgba(0, 0, 0, 0.4);
    }

    h2 {
        margin: 0 0 12px;
        font-size: 16px;
        font-weight: 600;
        color: var(--color-text-primary);
        text-align: center;
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
