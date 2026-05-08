<script lang="ts">
    import type { FriendlyError } from '$lib/file-explorer/types'
    import { renderErrorMarkdown } from '$lib/file-explorer/pane/error-pane-utils'
    import { openExternalUrl, openSystemSettingsUrl } from '$lib/tauri-commands'

    interface Props {
        friendly: FriendlyError
    }

    const { friendly }: Props = $props()

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

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="error-content" onclick={handleMarkdownLinkClick}>
    <!-- eslint-disable-next-line svelte/no-at-html-tags -- Backend-controlled markdown, not user input -->
    <div id="error-dialog-message" class="message selectable">{@html renderErrorMarkdown(friendly.explanation)}</div>
    <!-- eslint-disable-next-line svelte/no-at-html-tags -- Backend-controlled markdown, not user input -->
    <div class="suggestion">{@html renderErrorMarkdown(friendly.suggestion)}</div>
</div>

<style>
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

    .selectable {
        user-select: text;
        -webkit-user-select: text;
        cursor: text;
    }
</style>
