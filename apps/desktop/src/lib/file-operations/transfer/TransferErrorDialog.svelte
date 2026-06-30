<script lang="ts">
    import type { WriteOperationError, TransferOperationType, FriendlyError } from '$lib/file-explorer/types'
    import { getUserFriendlyMessage, getTechnicalDetails, getErrorDisplayMeta } from './transfer-error-messages'
    import FallbackErrorContent from './FallbackErrorContent.svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        operationType: TransferOperationType
        error: WriteOperationError
        onClose: () => void
        onRetry?: () => void
    }

    const { operationType, error, onClose, onRetry }: Props = $props()

    let showDetails = $state(false)

    /** Title, explanation, and suggestion all come from the typed error. */
    const titleText = $derived(getUserFriendlyMessage(error, operationType).title)

    /** Category (tint + icon) and Retry visibility derive from the typed error. */
    const displayMeta = $derived(getErrorDisplayMeta(error))
    const category = $derived<FriendlyError['category']>(displayMeta.category)

    /** Retry button visibility: transient kinds always offer retry, others gated on explicit retryHint. */
    const showRetry = $derived(onRetry !== undefined && (category === 'transient' || displayMeta.retryHint))

    /** Container styling per category. */
    const containerStyle = $derived(
        category === 'serious'
            ? 'width: 420px; max-width: 90vw; background: var(--color-error-bg); border-color: var(--color-error-border)'
            : category === 'transient'
              ? 'width: 420px; max-width: 90vw; background: var(--color-warning-bg-solid); border-color: var(--color-border-strong)'
              : 'width: 420px; max-width: 90vw; background: var(--color-bg-secondary); border-color: var(--color-border-strong)',
    )

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
    {containerStyle}
>
    {#snippet title()}
        <span class="error-title-content">
            <span
                class="error-icon"
                class:icon-error={category === 'serious'}
                class:icon-warning={category === 'transient'}
                class:icon-info={category === 'needs_action'}
                aria-hidden="true"
            >
                {#if category === 'serious'}
                    <Icon name="circle-alert" size={22} />
                {:else if category === 'transient'}
                    <Icon name="triangle-alert" size={22} />
                {:else}
                    <Icon name="info" size={22} />
                {/if}
            </span>
            {titleText}
        </span>
    {/snippet}

    <FallbackErrorContent {error} {operationType} />

    <!-- Technical details (collapsible) -->
    <div class="details-section">
        <button class="details-toggle" onclick={toggleDetails} aria-expanded={showDetails}>
            <span class="toggle-icon" class:expanded={showDetails}>
                <Icon name="chevron-right" size={12} />
            </span>
            {tString('fileOperations.errorDialog.technicalDetails')}
        </button>
        {#if showDetails}
            <div class="details-content">
                <textarea
                    class="details-text"
                    readonly
                    rows={technicalDetails.split('\n').length}
                    aria-label={tString('fileOperations.errorDialog.technicalDetailsAria')}>{technicalDetails}</textarea
                >
            </div>
        {/if}
    </div>

    {#snippet footer()}
        {#if onRetry && showRetry}
            <Button variant="secondary" onclick={onRetry}>{tString('fileOperations.errorDialog.retry')}</Button>
        {/if}
        <Button variant="primary" onclick={onClose}>{tString('fileOperations.errorDialog.close')}</Button>
    {/snippet}
</ModalDialog>

<style>
    .error-title-content {
        display: flex;
        align-items: center;
        justify-content: flex-start;
        gap: var(--spacing-md);
    }

    .error-icon {
        flex-shrink: 0;
        width: 24px;
        height: 24px;
        display: flex;
        align-items: center;
        justify-content: center;
    }

    .error-icon.icon-error {
        color: var(--color-error);
    }

    .error-icon.icon-warning {
        color: var(--color-warning);
    }

    .error-icon.icon-info {
        color: var(--color-text-secondary);
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
        padding: var(--spacing-xs) 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        background: none;
        border: none;
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
        padding: var(--spacing-sm) var(--spacing-md);
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
</style>
