<script lang="ts">
    import type { WriteOperationError, TransferOperationType, FriendlyError } from '$lib/file-explorer/types'
    import { getUserFriendlyMessage, getTechnicalDetails } from './transfer-error-messages'
    import { renderErrorMarkdown } from '$lib/file-explorer/pane/error-pane-utils'
    import { openExternalUrl, openSystemSettingsUrl } from '$lib/tauri-commands'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import IconCircleAlert from '~icons/lucide/circle-alert'
    import IconTriangleAlert from '~icons/lucide/triangle-alert'
    import IconInfo from '~icons/lucide/info'

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

    /** Variant-derived fallback when the backend didn't attach a `friendly`. */
    const fallback = $derived(getUserFriendlyMessage(error, operationType))

    /** What the dialog actually renders — backend friendly preferred, fallback otherwise. */
    const display = $derived(
        friendlyError
            ? {
                  title: friendlyError.title,
                  // Markdown for backend-supplied copy.
                  bodyHtml: renderErrorMarkdown(friendlyError.explanation),
                  suggestionHtml: renderErrorMarkdown(friendlyError.suggestion),
                  category: friendlyError.category,
                  retryHint: friendlyError.retryHint,
              }
            : {
                  title: fallback.title,
                  bodyHtml: null,
                  bodyText: fallback.message,
                  suggestionHtml: null,
                  suggestionText: fallback.suggestion,
                  category: 'serious' as const,
                  retryHint: false,
              },
    )

    const technicalDetails = $derived(friendlyError?.rawDetail ?? getTechnicalDetails(error))

    /** Container colors per category. NeedsAction is neutral, Transient is warning-yellow, Serious is red. */
    const containerStyle = $derived(
        display.category === 'serious'
            ? 'width: 420px; max-width: 90vw; background: var(--color-error-bg); border-color: var(--color-error-border)'
            : display.category === 'transient'
              ? 'width: 420px; max-width: 90vw; background: var(--color-warning-bg); border-color: var(--color-border-strong)'
              : 'width: 420px; max-width: 90vw; background: var(--color-bg-secondary); border-color: var(--color-border-strong)',
    )

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter') {
            onClose()
        }
    }

    function toggleDetails() {
        showDetails = !showDetails
    }

    /**
     * Backend friendly-error markdown can include `x-apple.systempreferences:` URLs (route through
     * Rust IPC) or plain http(s) URLs (route through the external opener). Mirrors `ErrorPane`.
     * Backend-controlled markdown only, so no allowlist needed.
     */
    function handleMarkdownLinkClick(e: MouseEvent) {
        const link = (e.target instanceof Element ? e.target : null)?.closest('a')
        const href = link?.getAttribute('href')
        if (!link || !href) return
        e.preventDefault()
        if (href.startsWith('x-apple.systempreferences:')) {
            void openSystemSettingsUrl(href)
        } else {
            void openExternalUrl(href)
        }
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
                class:icon-error={display.category === 'serious'}
                class:icon-warning={display.category === 'transient'}
                class:icon-info={display.category === 'needs_action'}
                aria-hidden="true"
            >
                {#if display.category === 'serious'}
                    <IconCircleAlert width="22" height="22" />
                {:else if display.category === 'transient'}
                    <IconTriangleAlert width="22" height="22" />
                {:else}
                    <IconInfo width="22" height="22" />
                {/if}
            </span>
            {display.title}
        </span>
    {/snippet}

    <!-- Click delegate for anchor tags inside rendered markdown -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="error-content" onclick={handleMarkdownLinkClick}>
        {#if display.bodyHtml !== null && display.bodyHtml !== undefined}
            <!-- eslint-disable-next-line svelte/no-at-html-tags -- Backend-controlled markdown, not user input -->
            <div id="error-dialog-message" class="message selectable">{@html display.bodyHtml}</div>
            <!-- eslint-disable-next-line svelte/no-at-html-tags -- Backend-controlled markdown, not user input -->
            <div class="suggestion">{@html display.suggestionHtml}</div>
        {:else}
            <p id="error-dialog-message" class="message selectable">{display.bodyText}</p>
            <p class="suggestion">{display.suggestionText}</p>
        {/if}
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
        {#if onRetry && (display.retryHint || display.category === 'transient')}
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

    .error-content {
        padding: 0 var(--spacing-xl) var(--spacing-lg);
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

    /* Markdown content inside the message/suggestion blocks */
    .message :global(p),
    .suggestion :global(p) {
        margin: 0 0 var(--spacing-sm);
    }

    .message :global(p:last-child),
    .suggestion :global(p:last-child) {
        margin-bottom: 0;
    }

    .message :global(ul),
    .suggestion :global(ul) {
        margin: var(--spacing-xs) 0 0;
        padding-left: var(--spacing-lg);
    }

    .message :global(a),
    .suggestion :global(a) {
        color: var(--color-accent-text);
        text-decoration: underline;
    }

    .message :global(code),
    .suggestion :global(code) {
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
        padding: 0 var(--spacing-xxs);
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-sm);
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
