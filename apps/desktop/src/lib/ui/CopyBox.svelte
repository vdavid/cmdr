<script lang="ts">
    /**
     * Copyable monospace text: a terminal command, a filesystem path, anything a
     * user needs verbatim. One-click Copy with "Copied!" feedback.
     */
    import { copyToClipboard } from '$lib/tauri-commands'
    import Button from './Button.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        /** The text to display and copy. */
        text: string
        /**
         * Rendered instead of `text` when the full string is too long to show (a
         * shortened form with an ellipsis). Copy still carries the WHOLE `text`:
         * the cap protects the layout, and the clipboard has none.
         */
        displayText?: string
        /** aria-label for the Copy button. Defaults to the terminal-command wording. */
        copyAriaLabel?: string
    }

    const { text, displayText, copyAriaLabel }: Props = $props()

    let copied = $state(false)

    async function handleCopy() {
        try {
            await copyToClipboard(text)
        } catch {
            await navigator.clipboard.writeText(text)
        }
        copied = true
        setTimeout(() => {
            copied = false
        }, 2000)
    }
</script>

<div class="copy-box">
    <code class="text">{displayText ?? text}</code>
    <Button
        variant="secondary"
        size="mini"
        onclick={handleCopy}
        aria-label={copyAriaLabel ?? tString('ui.copyBox.copyAria')}
    >
        {copied ? tString('ui.copyBox.copied') : tString('ui.copyBox.copy')}
    </Button>
</div>

<style>
    .copy-box {
        display: flex;
        align-items: stretch;
        gap: var(--spacing-sm);
        background: var(--color-bg-primary);
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-lg);
        padding: var(--spacing-md);
    }

    .text {
        flex: 1;
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        word-break: break-all;
        line-height: var(--font-line-height-normal);
        background: none;
        padding: 0;
    }
</style>
