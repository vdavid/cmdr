<script lang="ts">
    /**
     * Error dialog for copy operation failures.
     * Shows user-friendly, volume-agnostic error messages with selectable text
     * and collapsible technical details.
     */
    import { onMount, tick } from 'svelte'
    import type { WriteOperationError } from '$lib/file-explorer/types'
    import { getUserFriendlyMessage, getTechnicalDetails } from './copy-error-messages'

    interface Props {
        /** The error that occurred */
        error: WriteOperationError
        /** Callback when dialog is closed */
        onClose: () => void
        /** Optional callback to retry the operation */
        onRetry?: () => void
    }

    const { error, onClose, onRetry }: Props = $props()

    let overlayElement: HTMLDivElement | undefined = $state()
    let showDetails = $state(false)

    const friendly = $derived(getUserFriendlyMessage(error))
    const technicalDetails = $derived(getTechnicalDetails(error))

    function handleKeydown(event: KeyboardEvent) {
        event.stopPropagation()
        if (event.key === 'Escape' || event.key === 'Enter') {
            onClose()
        }
    }

    function toggleDetails() {
        showDetails = !showDetails
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
    aria-labelledby="error-dialog-title"
    aria-describedby="error-dialog-message"
    tabindex="-1"
    onkeydown={handleKeydown}
>
    <div class="error-dialog">
        <!-- Error icon and title -->
        <div class="error-header">
            <div class="error-icon" aria-hidden="true">
                <svg width="24" height="24" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                    <circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="2" />
                    <line
                        x1="12"
                        y1="8"
                        x2="12"
                        y2="13"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                    />
                    <circle cx="12" cy="16.5" r="1" fill="currentColor" />
                </svg>
            </div>
            <h2 id="error-dialog-title">{friendly.title}</h2>
        </div>

        <!-- Main message (selectable) -->
        <div class="error-content">
            <p id="error-dialog-message" class="message selectable">{friendly.message}</p>
            <p class="suggestion">{friendly.suggestion}</p>
        </div>

        <!-- Technical details (collapsible) -->
        <div class="details-section">
            <button class="details-toggle" onclick={toggleDetails} aria-expanded={showDetails}>
                <span class="toggle-icon" class:expanded={showDetails}>
                    <svg width="12" height="12" viewBox="0 0 12 12" fill="none" xmlns="http://www.w3.org/2000/svg">
                        <path d="M4 2L8 6L4 10" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
                    </svg>
                </span>
                Technical details
            </button>
            {#if showDetails}
                <div class="details-content">
                    <textarea
                        class="details-text"
                        readonly
                        rows={technicalDetails.split('\n').length}
                        aria-label="Technical error details">{technicalDetails}</textarea
                    >
                </div>
            {/if}
        </div>

        <!-- Action buttons -->
        <div class="button-row">
            {#if onRetry}
                <button class="secondary" onclick={onRetry}>Retry</button>
            {/if}
            <button class="primary" onclick={onClose}>Close</button>
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

    .error-dialog {
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border-primary);
        border-radius: 12px;
        width: 420px;
        max-width: 90vw;
        box-shadow: 0 16px 48px rgba(0, 0, 0, 0.4);
    }

    .error-header {
        display: flex;
        align-items: center;
        gap: 12px;
        padding: 20px 24px 12px;
    }

    .error-icon {
        flex-shrink: 0;
        width: 32px;
        height: 32px;
        display: flex;
        align-items: center;
        justify-content: center;
        color: var(--color-error);
    }

    h2 {
        margin: 0;
        font-size: 16px;
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .error-content {
        padding: 0 24px 16px;
    }

    .message {
        margin: 0 0 8px;
        font-size: 13px;
        color: var(--color-text-secondary);
        line-height: 1.5;
    }

    .suggestion {
        margin: 0;
        font-size: 12px;
        color: var(--color-text-muted);
        line-height: 1.5;
    }

    /* Make text selectable (override global user-select: none) */
    .selectable {
        user-select: text;
        -webkit-user-select: text;
        cursor: text;
    }

    .details-section {
        padding: 0 24px 16px;
        border-top: 1px solid var(--color-border-primary);
        margin-top: 4px;
        padding-top: 12px;
    }

    .details-toggle {
        display: flex;
        align-items: center;
        gap: 6px;
        padding: 4px 0;
        font-size: 12px;
        color: var(--color-text-muted);
        background: none;
        border: none;
        cursor: pointer;
        transition: color 0.15s ease;
    }

    .details-toggle:hover {
        color: var(--color-text-secondary);
    }

    .toggle-icon {
        display: flex;
        align-items: center;
        justify-content: center;
        transition: transform 0.15s ease;
    }

    .toggle-icon.expanded {
        transform: rotate(90deg);
    }

    .details-content {
        margin-top: 8px;
    }

    .details-text {
        width: 100%;
        padding: 8px 10px;
        font-size: 11px;
        font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
        color: var(--color-text-secondary);
        background: var(--color-bg-tertiary);
        border: 1px solid var(--color-border-primary);
        border-radius: 6px;
        resize: none;
        /* Make text selectable */
        user-select: text;
        -webkit-user-select: text;
        cursor: text;
        line-height: 1.4;
    }

    .details-text:focus {
        outline: none;
        border-color: var(--color-accent);
    }

    .button-row {
        display: flex;
        gap: 12px;
        justify-content: center;
        padding: 0 24px 20px;
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

    .secondary {
        background: transparent;
        color: var(--color-text-secondary);
        border: 1px solid var(--color-border-primary);
    }

    .secondary:hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }
</style>
