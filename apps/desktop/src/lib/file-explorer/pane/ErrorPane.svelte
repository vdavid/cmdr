<script lang="ts">
    import { onDestroy } from 'svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import type { FriendlyError } from '../types'
    import { openExternalUrl, openPrivacySettings, openSystemSettingsUrl } from '$lib/tauri-commands'
    import { isMacOS } from '$lib/shortcuts/key-capture'
    import { eventMatchesCommand } from '$lib/shortcuts/shortcut-dispatch'
    import Button from '$lib/ui/Button.svelte'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import { renderErrorMarkdown } from './error-pane-utils'
    import { systemStrings } from '$lib/system-strings.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        friendly: FriendlyError
        folderPath: string
        onRetry?: () => void
        /**
         * Whether this pane's tab has somewhere to go back to. False on a first-paint
         * error (history isn't persisted across sessions, and a fresh tab seeds a
         * single entry), where `nav.back` would be a silent no-op — so the button is
         * hidden rather than shown dead. "Go to home folder" is the always-available
         * way out and renders regardless.
         */
        canGoBack?: boolean
        onGoBack?: () => void
        onGoHome?: () => void
        /** Whether this pane is the focused one, gating the ⌘D key handler. */
        isFocused?: boolean
    }

    const {
        friendly,
        folderPath,
        onRetry,
        canGoBack = false,
        onGoBack,
        onGoHome,
        isFocused = false,
    }: Props = $props()

    let detailsOpen = $state(false)

    // Retry tracking (resets when component is destroyed/recreated on navigation)
    let retryCount = $state(0)
    let retryTimestamps = $state<number[]>([])
    let now = $state(Date.now())

    // Update `now` every 5 seconds for relative time display
    const intervalId = setInterval(() => {
        now = Date.now()
    }, 5000)

    onDestroy(() => { clearInterval(intervalId); })

    /**
     * ⌘D belongs to the error screen for as long as one is showing. The listener is
     * CAPTURE-phase on `document`, so it runs ahead of both the explorer container's
     * keydown handler and the document-level command dispatcher: the screen's
     * "Technical details ⌘D" hint stays true even when the user has bound ⌘D to
     * another command. That's also why `errorPane.toggleTechnicalDetails` is a
     * fixed-key command — rebinding it here would be a no-op illusion.
     *
     * Gated on `isFocused` so two simultaneous error panes don't both toggle.
     */
    $effect(() => {
        if (!isFocused) return

        function handleDetailsKey(e: KeyboardEvent) {
            if (!eventMatchesCommand(e, 'errorPane.toggleTechnicalDetails')) return
            e.preventDefault()
            e.stopPropagation()
            detailsOpen = !detailsOpen
        }

        document.addEventListener('keydown', handleDetailsKey, true)
        return () => { document.removeEventListener('keydown', handleDetailsKey, true) }
    })

    function handleRetry() {
        retryCount += 1
        retryTimestamps = [...retryTimestamps, Date.now()]
        now = Date.now()
        onRetry?.()
    }

    /**
     * Route anchor clicks inside the markdown blocks. `x-apple.systempreferences:` URLs
     * go through a dedicated Rust IPC because Tauri's opener plugin only allows
     * http/https/mailto/tel by default and would silently swallow them. Everything else
     * goes through the standard external opener. The friendly-error markdown is
     * backend-controlled (no user input), so no URL allowlisting is needed here.
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

    function formatRelativeTime(timestampMs: number, currentMs: number): string {
        const seconds = Math.round((currentMs - timestampMs) / 1000)
        if (seconds < 5) return tString('fileExplorer.errorPane.aMomentAgo')
        if (seconds < 60) return tString('fileExplorer.errorPane.secondsAgo', { seconds })
        const minutes = Math.round(seconds / 60)
        if (minutes < 60) return tString('fileExplorer.errorPane.minutesAgo', { minutes })
        const hours = Math.round(minutes / 60)
        return tString('fileExplorer.errorPane.hoursAgo', { hours })
    }

    const isPermissionDenied = $derived(friendly.actionKind === 'open_privacy_settings')

    const showRetryButton = $derived(friendly.category === 'transient' && friendly.retryHint)

    const retryInfo = $derived.by(() => {
        if (retryTimestamps.length === 0) return null
        const first = retryTimestamps[0]
        const last = retryTimestamps[retryTimestamps.length - 1]
        return {
            count: retryCount,
            firstAgo: formatRelativeTime(first, now),
            lastAgo: retryCount > 1 ? formatRelativeTime(last, now) : null,
        }
    })
</script>

<div class="error-pane" role="alert" aria-live="assertive">
    <div class="content">
        <h2 class="title">
            {#if friendly.category === 'serious'}
                <span class="title-icon icon-error"><Icon name="circle-alert" size={20} aria-hidden="true" /></span>
            {:else if friendly.category === 'transient'}
                <span class="title-icon icon-warning"><Icon name="triangle-alert" size={20} aria-hidden="true" /></span>
            {/if}
            {friendly.title}
        </h2>
        <p class="folder-path">{folderPath}</p>

        <!-- Click delegate for anchor tags inside rendered markdown -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="explanation" onclick={handleMarkdownLinkClick}>
            <!-- eslint-disable-next-line svelte/no-at-html-tags -- Input is our own hardcoded strings from Rust, not user content -->
            {@html renderErrorMarkdown(friendly.explanation)}
        </div>

        <!-- Click delegate for anchor tags inside rendered markdown -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="suggestion" onclick={handleMarkdownLinkClick}>
            <!-- eslint-disable-next-line svelte/no-at-html-tags -- Input is our own hardcoded strings from Rust, not user content -->
            {@html renderErrorMarkdown(friendly.suggestion)}
        </div>

        <!--
          One row for every action, so the error-specific CTA and the two always-there
          ways out read as one set. Every error screen gets at least "Go to home folder":
          44 of the ~60 error reasons carry no CTA of their own and were dead ends.
        -->
        <div class="cta">
            {#if showRetryButton}
                <Button variant="primary" onclick={handleRetry}>{tString('fileExplorer.errorPane.tryAgain')}</Button>
            {/if}

            {#if isPermissionDenied && isMacOS()}
                <Button variant="primary" onclick={() => openPrivacySettings()}
                    >{tString('fileExplorer.errorPane.openSystemSettings', {
                        systemSettings: systemStrings.systemSettings,
                    })}</Button
                >
            {/if}

            {#if canGoBack}
                <Button onclick={() => onGoBack?.()}>
                    {tString('fileExplorer.errorPane.goBack')}
                    <ShortcutChip commandId="nav.back" clickable={false} size="sm" />
                </Button>
            {/if}

            <Button onclick={() => onGoHome?.()}>
                {tString('fileExplorer.errorPane.goHome')}
                <ShortcutChip commandId="nav.goHome" clickable={false} size="sm" />
            </Button>
        </div>

        <details class="technical-details" bind:open={detailsOpen}>
            <summary>
                {tString('fileExplorer.errorPane.technicalDetails')}
                <ShortcutChip commandId="errorPane.toggleTechnicalDetails" clickable={false} size="sm" />
            </summary>
            <pre class="raw-detail">{friendly.rawDetail}</pre>
            {#if retryInfo}
                <p class="retry-info">
                    {retryInfo.lastAgo
                        ? tString('fileExplorer.errorPane.retryInfoWithLast', {
                              count: retryInfo.count,
                              firstAgo: retryInfo.firstAgo,
                              lastAgo: retryInfo.lastAgo,
                          })
                        : tString('fileExplorer.errorPane.retryInfo', {
                              count: retryInfo.count,
                              firstAgo: retryInfo.firstAgo,
                          })}
                </p>
            {/if}
        </details>
    </div>
</div>

<style>
    .error-pane {
        display: flex;
        align-items: center;
        justify-content: center;
        height: 100%;
        padding: var(--spacing-xl);
        line-height: var(--font-line-height-prose);
        user-select: text;
        -webkit-user-select: text;
    }

    .error-pane ::selection {
        background: color-mix(in srgb, var(--color-accent) 20%, transparent);
        color: inherit;
    }

    .content {
        max-width: 450px;
    }

    h2 {
        font-size: var(--font-size-xl);
        font-weight: 600;
        margin: 0 0 var(--spacing-sm) 0;
        color: var(--color-accent-text);
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }

    .title-icon {
        display: flex;
        flex-shrink: 0;
    }

    .icon-warning {
        color: var(--color-warning);
    }

    .icon-error {
        color: var(--color-error);
    }

    .folder-path {
        color: var(--color-text-secondary);
        margin: 0 0 var(--spacing-lg) 0;
        word-break: break-all;
    }

    .explanation {
        margin-bottom: var(--spacing-lg);
    }

    .suggestion {
        margin-bottom: var(--spacing-lg);
    }

    /* Style markdown output within explanation/suggestion */
    .explanation :global(strong),
    .suggestion :global(strong) {
        font-weight: 600;
    }

    .explanation :global(a),
    .suggestion :global(a) {
        color: var(--color-accent-text);
        text-decoration: underline;
        /* The global `cursor: default` on html overrides the anchor's default pointer. */
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- re-enable pointer on links over the global cursor: default (see above) */
        cursor: pointer;
    }

    .explanation :global(a:hover),
    .suggestion :global(a:hover) {
        color: var(--color-accent-hover);
    }

    .explanation :global(ul),
    .suggestion :global(ul) {
        padding-left: var(--spacing-xl);
        margin: var(--spacing-sm) 0;
    }

    .explanation :global(li),
    .suggestion :global(li) {
        margin-bottom: var(--spacing-xs);
    }

    .explanation :global(code),
    .suggestion :global(code) {
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
        background: var(--color-bg-tertiary);
        padding: var(--spacing-xxs) var(--spacing-xs);
        border-radius: var(--radius-xs);
    }

    /* Wraps rather than overflows: the pane is 450px wide and a localized
       "Go to home folder" plus its chip can outgrow one line next to a CTA. */
    .cta {
        display: flex;
        flex-wrap: wrap;
        justify-content: center;
        gap: var(--spacing-sm);
        margin: var(--spacing-lg) 0;
    }

    .technical-details {
        margin-top: var(--spacing-lg);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    .technical-details summary {
        user-select: none;
    }

    /* Chips ride along inside the button labels and the summary. Don't give `summary`
       a flex/grid display to place its chip: WebKit draws the disclosure triangle off
       `display: list-item`, and any other value drops it. The chip is inline-flex on
       its own, so it flows after the label on the markup's own whitespace. */
    .cta :global(.shortcut-chip),
    .technical-details summary :global(.shortcut-chip) {
        vertical-align: middle;
    }

    .technical-details summary:hover {
        color: var(--color-text-primary);
    }

    .raw-detail {
        margin: var(--spacing-sm) 0;
        padding: var(--spacing-sm);
        background: var(--color-bg-secondary);
        border-radius: var(--radius-sm);
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
        white-space: pre-wrap;
        word-break: break-all;
    }

    .retry-info {
        margin: var(--spacing-xs) 0 0;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }
</style>
