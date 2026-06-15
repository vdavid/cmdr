<script lang="ts">
    import type { WriteOperationError, TransferOperationType, FriendlyError } from '$lib/file-explorer/types'
    import { getUserFriendlyMessage, getTechnicalDetails } from './transfer-error-messages'
    import FriendlyErrorContent from './FriendlyErrorContent.svelte'
    import FallbackErrorContent from './FallbackErrorContent.svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Icon from '$lib/ui/Icon.svelte'

    interface Props {
        operationType: TransferOperationType
        error: WriteOperationError
        /** Backend-supplied friendly error info; preferred over the FE-derived copy when present. */
        friendlyError?: FriendlyError
        onClose: () => void
        onRetry?: () => void
    }

    const { operationType, error, friendlyError, onClose, onRetry }: Props = $props()

    let showDetails = $state(false)

    /** Title: backend-supplied friendly title preferred, FE-derived fallback otherwise. */
    const titleText = $derived(friendlyError?.title ?? getUserFriendlyMessage(error, operationType).title)

    /** Category drives icon and container colors. Fallback path is always treated as `serious`. */
    const category = $derived<FriendlyError['category']>(friendlyError?.category ?? 'serious')

    /** Retry button visibility: transient kinds always offer retry, others gated on explicit retryHint. */
    const showRetry = $derived(onRetry !== undefined && (category === 'transient' || friendlyError?.retryHint === true))

    /** Container styling per category. */
    const containerStyle = $derived(
        category === 'serious'
            ? 'width: 420px; max-width: 90vw; background: var(--color-error-bg); border-color: var(--color-error-border)'
            : category === 'transient'
              ? 'width: 420px; max-width: 90vw; background: var(--color-warning-bg-solid); border-color: var(--color-border-strong)'
              : 'width: 420px; max-width: 90vw; background: var(--color-bg-secondary); border-color: var(--color-border-strong)',
    )

    const technicalDetails = $derived(friendlyError?.rawDetail ?? getTechnicalDetails(error))

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

    {#if friendlyError}
        <FriendlyErrorContent friendly={friendlyError} />
    {:else}
        <FallbackErrorContent {error} {operationType} />
    {/if}

    <!-- Technical details (collapsible) -->
    <div class="details-section">
        <button class="details-toggle" onclick={toggleDetails} aria-expanded={showDetails}>
            <span class="toggle-icon" class:expanded={showDetails}>
                <Icon name="chevron-right" size={12} />
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

    <div class="button-row">
        {#if onRetry && showRetry}
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

    .button-row {
        display: flex;
        gap: var(--spacing-md);
        justify-content: center;
        padding: 0 var(--spacing-xl) var(--spacing-xl);
    }
</style>
