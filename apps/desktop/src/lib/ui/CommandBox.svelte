<script lang="ts">
    /**
     * Copyable terminal command display.
     * Monospace command string with a one-click Copy button and "Copied!" feedback.
     */
    import { copyToClipboard } from '$lib/tauri-commands'
    import Button from './Button.svelte'

    interface Props {
        /** The command to display and copy. */
        command: string
    }

    const { command }: Props = $props()

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
    <code class="command">{command}</code>
    <Button variant="secondary" size="mini" onclick={handleCopy} aria-label="Copy command to clipboard">
        {copied ? 'Copied!' : 'Copy'}
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
