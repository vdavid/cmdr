<script lang="ts">
    import { onMount } from 'svelte'
    import type { WriteOperationError, TransferOperationType, FriendlyError } from '$lib/file-explorer/types'
    import type { ProgressAtStop } from '$lib/tauri-commands'
    import { getUserFriendlyMessage, getTechnicalDetails, getErrorDisplayMeta } from './transfer-error-messages'
    import FallbackErrorContent from './FallbackErrorContent.svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import TextArea from '$lib/ui/TextArea.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import DecisionKeyHint from '../DecisionKeyHint.svelte'
    import { answerDecisionKey, DECISION_KEYS_ARM_MS, ERROR_KEYS, type DecisionChoice } from '../decision-keys'

    interface Props {
        operationType: TransferOperationType
        error: WriteOperationError
        /** How far the operation got when it stopped, from the `write-error`
         *  event. The copy for a drive that left says so; every other variant
         *  ignores it. */
        progressAtStop?: ProgressAtStop | null
        onClose: () => void
        onRetry?: () => void
        /** Starts the same copy again with the free-space check skipped. Offered
         *  only for a copy refused for space, whose figure is an upper bound. */
        onCopyAnyway?: () => void
    }

    const { operationType, error, progressAtStop = null, onClose, onRetry, onCopyAnyway }: Props = $props()

    let showDetails = $state(false)

    /** Title, explanation, and suggestion all come from the typed error. */
    const titleText = $derived(getUserFriendlyMessage(error, operationType, progressAtStop).title)

    /** Category (tint + icon) and Retry visibility derive from the typed error. */
    const displayMeta = $derived(getErrorDisplayMeta(error))
    const category = $derived<FriendlyError['category']>(displayMeta.category)

    /** Retry button visibility: transient kinds always offer retry, others gated on explicit retryHint. */
    const showRetry = $derived(onRetry !== undefined && (category === 'transient' || displayMeta.retryHint))

    /** "Copy anyway": a space shortfall is the person's call (`SpaceShortfall` in the backend). */
    const showCopyAnyway = $derived(
        onCopyAnyway !== undefined && operationType === 'copy' && error.type === 'insufficient_space',
    )

    /** Container styling per category. */
    const containerStyle = $derived(
        category === 'serious'
            ? 'width: 420px; max-width: 90vw; background: var(--color-error-bg); border-color: var(--color-error-border)'
            : category === 'transient'
              ? 'width: 420px; max-width: 90vw; background: var(--color-warning-bg-solid); border-color: var(--color-border-strong)'
              : 'width: 420px; max-width: 90vw; background: var(--color-bg-secondary); border-color: var(--color-border-strong)',
    )

    const technicalDetails = $derived(getTechnicalDetails(error))

    /** Retry and Copy anyway are offered only with their handler, so `run` never meets a missing one. */
    const keyChoices = $derived<DecisionChoice[]>([
        { key: ERROR_KEYS.retry, enabled: showRetry, run: () => onRetry?.() },
        { key: ERROR_KEYS.copyAnyway, enabled: showCopyAnyway, run: () => onCopyAnyway?.() },
        { key: ERROR_KEYS.close, enabled: true, run: onClose },
    ])

    /** Keys answer only once the dialog has been up `DECISION_KEYS_ARM_MS`: it
     *  opens when an operation stops, which can be mid-keystroke. */
    let keysArmed = $state(false)
    onMount(() => {
        const timer = setTimeout(() => {
            keysArmed = true
        }, DECISION_KEYS_ARM_MS)
        return () => {
            clearTimeout(timer)
        }
    })

    /** A bare letter answers (`decision-keys.ts`); Enter means Close, the safe way out. */
    function handleKeydown(event: KeyboardEvent) {
        if (!keysArmed) return
        answerDecisionKey(event, keyChoices, ERROR_KEYS.close)
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
    resizable="horizontal"
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

    <FallbackErrorContent {error} {operationType} {progressAtStop} />

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
                <TextArea
                    value={technicalDetails}
                    readonly
                    mono
                    resizable={false}
                    radius="md"
                    rows={technicalDetails.split('\n').length}
                    ariaLabel={tString('fileOperations.errorDialog.technicalDetailsAria')}
                />
            </div>
        {/if}
    </div>

    {#snippet footer()}
        {#if onRetry && showRetry}
            <Button variant="secondary" onclick={onRetry} aria-keyshortcuts={ERROR_KEYS.retry}
                >{tString('fileOperations.errorDialog.retry')}<DecisionKeyHint key={ERROR_KEYS.retry} /></Button
            >
        {/if}
        {#if onCopyAnyway && showCopyAnyway}
            <Button variant="secondary" onclick={onCopyAnyway} aria-keyshortcuts={ERROR_KEYS.copyAnyway}
                >{tString('fileOperations.errorDialog.copyAnyway')}<DecisionKeyHint key={ERROR_KEYS.copyAnyway} /></Button
            >
        {/if}
        <Button variant="primary" onclick={onClose} aria-keyshortcuts="{ERROR_KEYS.close} Enter"
            >{tString('fileOperations.errorDialog.close')}<DecisionKeyHint key={ERROR_KEYS.close} /></Button
        >
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
        padding: var(--spacing-md) 0 var(--spacing-lg);
        border-top: 1px solid var(--color-border-strong);
        margin-top: var(--spacing-xs);
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

</style>
