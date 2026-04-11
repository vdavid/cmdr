<script lang="ts">
    import { onDestroy } from 'svelte'
    import type { FriendlyError } from '../types'
    import { openPrivacySettings } from '$lib/tauri-commands'
    import { isMacOS } from '$lib/shortcuts/key-capture'
    import Button from '$lib/ui/Button.svelte'
    import { renderErrorMarkdown } from './error-pane-utils'

    interface Props {
        friendly: FriendlyError
        folderPath: string
        onRetry?: () => void
    }

    const { friendly, folderPath, onRetry }: Props = $props()

    // Retry tracking (resets when component is destroyed/recreated on navigation)
    let retryCount = $state(0)
    let retryTimestamps = $state<number[]>([])
    let now = $state(Date.now())

    // Update `now` every 5 seconds for relative time display
    const intervalId = setInterval(() => {
        now = Date.now()
    }, 5000)

    onDestroy(() => clearInterval(intervalId))

    function handleRetry() {
        retryCount += 1
        retryTimestamps = [...retryTimestamps, Date.now()]
        now = Date.now()
        onRetry?.()
    }

    function formatRelativeTime(timestampMs: number, currentMs: number): string {
        const seconds = Math.round((currentMs - timestampMs) / 1000)
        if (seconds < 5) return 'a moment ago'
        if (seconds < 60) return `${String(seconds)}s ago`
        const minutes = Math.round(seconds / 60)
        if (minutes < 60) return `${String(minutes)}m ago`
        const hours = Math.round(minutes / 60)
        return `${String(hours)}h ago`
    }

    const titleColorClass = $derived(
        friendly.category === 'transient'
            ? 'title-warning'
            : friendly.category === 'serious'
              ? 'title-error'
              : 'title-default',
    )

    const isPermissionDenied = $derived(
        friendly.category === 'needs_action' && friendly.title.toLowerCase().includes('no permission'),
    )

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
        <h2 class={titleColorClass}>{friendly.title}</h2>
        <p class="folder-path">{folderPath}</p>

        <div class="explanation">
            <!-- eslint-disable-next-line svelte/no-at-html-tags -- Input is our own hardcoded strings from Rust, not user content -->
            {@html renderErrorMarkdown(friendly.explanation)}
        </div>

        <div class="suggestion">
            <!-- eslint-disable-next-line svelte/no-at-html-tags -- Input is our own hardcoded strings from Rust, not user content -->
            {@html renderErrorMarkdown(friendly.suggestion)}
        </div>

        {#if showRetryButton}
            <div class="cta">
                <Button variant="primary" onclick={handleRetry}>Try again</Button>
            </div>
        {/if}

        {#if isPermissionDenied && isMacOS()}
            <div class="cta">
                <Button variant="primary" onclick={() => openPrivacySettings()}>Open System Settings</Button>
            </div>
        {/if}

        <details class="technical-details">
            <summary>Technical details</summary>
            <pre class="raw-detail">{friendly.rawDetail}</pre>
            {#if retryInfo}
                <p class="retry-info">
                    Retry #{retryInfo.count} · first try {retryInfo.firstAgo}{retryInfo.lastAgo
                        ? ` · last try ${retryInfo.lastAgo}`
                        : ''}
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
        line-height: 1.5;
    }

    .content {
        max-width: 450px;
    }

    h2 {
        font-size: var(--font-size-xl);
        font-weight: 600;
        margin: 0 0 var(--spacing-sm) 0;
    }

    .title-warning {
        color: var(--color-warning);
    }

    .title-error {
        color: var(--color-error);
    }

    .title-default {
        color: var(--color-text-primary);
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

    .cta {
        display: flex;
        justify-content: center;
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
