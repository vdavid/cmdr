<script lang="ts">
    /**
     * Copyable monospace text: a terminal command by default, or any other string
     * a user needs verbatim (a path). One-click Copy with "Copied!" feedback.
     */
    import { copyToClipboard } from '$lib/tauri-commands'
    import Button from './Button.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        /** The command to display and copy. */
        command: string
        /**
         * Rendered instead of `command` when the full string is too long to show
         * (a shortened form with an ellipsis). Copy still carries the WHOLE
         * `command`: the cap protects the layout, and the clipboard has none.
         */
        displayText?: string
        /** aria-label for the Copy button. Defaults to the terminal-command wording. */
        copyAriaLabel?: string
    }

    const { command, displayText, copyAriaLabel }: Props = $props()

    let copied = $state(false)

    async function handleCopy() {
        try {
            await copyToClipboard(command)
        } catch {
            await navigator.clipboard.writeText(command)
        }
        copied = true
        setTimeout(() => {
            copied = false
        }, 2000)
    }
</script>

<div class="command-box">
    <code class="command">{displayText ?? command}</code>
    <Button
        variant="secondary"
        size="mini"
        onclick={handleCopy}
        aria-label={copyAriaLabel ?? tString('ui.commandBox.copyAria')}
    >
        {copied ? tString('ui.commandBox.copied') : tString('ui.commandBox.copy')}
    </Button>
</div>

<style>
    .command-box {
        display: flex;
        align-items: stretch;
        gap: var(--spacing-sm);
        background: var(--color-bg-primary);
        border: 1px solid var(--color-border-strong);
        border-radius: var(--radius-lg);
        padding: var(--spacing-md);
    }

    .command {
        flex: 1;
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        word-break: break-all;
        line-height: 1.5;
        background: none;
        padding: 0;
    }
</style>
