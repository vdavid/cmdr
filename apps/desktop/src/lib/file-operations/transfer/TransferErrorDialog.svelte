<script lang="ts">
    import type { WriteOperationError, TransferOperationType } from '$lib/file-explorer/types'
    import { getUserFriendlyMessage, getTechnicalDetails } from './transfer-error-messages'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'

    interface Props {
        operationType: TransferOperationType
        error: WriteOperationError
        onClose: () => void
        onRetry?: () => void
    }

    const { operationType, error, onClose, onRetry }: Props = $props()

    let showDetails = $state(false)

    const friendly = $derived(getUserFriendlyMessage(error, operationType))
    const technicalDetails = $derived(getTechnicalDetails(error))

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter') {
            onClose()
        }
    }

    function toggleDetails() {
        showDetails = !showDetails
    }
</script>

<ModalDialog
    titleId="error-dialog-title"
    onkeydown={handleKeydown}
    role="alertdialog"
    dialogId="transfer-error"
    onclose={onClose}
    ariaDescribedby="error-dialog-message"
    containerStyle="width: 420px; max-width: 90vw; background: var(--color-error-bg); border-color: var(--color-error-border)"
>
    {#snippet title()}
        <span class="error-title-content">
            <span class="error-icon" aria-hidden="true">
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
            </span>
            {friendly.title}
        </span>
    {/snippet}

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
            <Button variant="secondary" onclick={onRetry}>Retry</Button>
        {/if}
        <Button variant="primary" onclick={onClose}>Close</Button>
    </div>
</ModalDialog>

<style>
    .error-title-content {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: 12px;
    }

    .error-icon {
        flex-shrink: 0;
        width: 24px;
        height: 24px;
        display: flex;
        align-items: center;
        justify-content: center;
        color: var(--color-error);
    }

    .error-content {
        padding: 0 24px 16px;
    }

    .message {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: 1.5;
    }

    .suggestion {
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        line-height: 1.5;
    }

    /* Make text selectable (override global user-select: none) */
    .selectable {
        user-select: text;
        -webkit-user-select: text;
        cursor: text;
    }

    .details-section {
        padding: 0 var(--spacing-xl) var(--spacing-lg);
        border-top: 1px solid var(--color-border-strong);
        margin-top: var(--spacing-xs);
        padding-top: var(--spacing-md);
    }

    .details-toggle {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: 4px 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        background: none;
        border: none;
        cursor: pointer;
        transition: color var(--transition-base);
    }

    .details-toggle:hover {
        color: var(--color-text-secondary);
    }

    .details-toggle:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
    }

    .toggle-icon {
        display: flex;
        align-items: center;
        justify-content: center;
        transition: transform var(--transition-base);
    }

    .toggle-icon.expanded {
        transform: rotate(90deg);
    }

    .details-content {
        margin-top: var(--spacing-sm);
    }

    .details-text {
        width: 100%;
        padding: var(--spacing-sm) 10px;
        font-size: var(--font-size-sm);
        font-family: var(--font-mono);
        color: var(--color-text-secondary);
        background: var(--color-bg-tertiary);
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-md);
        resize: none;
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
</style>
