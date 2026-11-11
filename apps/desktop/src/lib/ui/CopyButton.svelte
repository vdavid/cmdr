<!--
  A quiet icon-only Copy: a copy glyph in a bare button that puts `text` on the clipboard and
  turns into a check for a moment. For text the surface already shows, or summarizes, beside
  it (a folder list behind an `InfoTip`), where `CopyBox`'s framed box and worded button would
  be louder than the line it sits on.
-->
<script lang="ts">
    import Icon from './Icon.svelte'
    import { tooltip } from '../tooltip/tooltip.js'
    import { copyToClipboard } from '#lib/tauri-commands/index.js'
    import { tString } from '#lib/intl/messages.svelte.js'

    interface Props {
        /** What lands on the clipboard: the WHOLE thing, however little of it is on screen. */
        text: string
        /**
         * What pressing it copies ("Copy the full list of folders"). The accessible name and
         * the hover hint: the glyph alone says copy, not copy what.
         */
        label: string
    }

    const { text, label }: Props = $props()

    /** How long the check stays before the glyph goes back to copy. */
    const COPIED_MS = 2000

    let copied = $state(false)
    let resetTimer: ReturnType<typeof setTimeout> | undefined

    async function copy(): Promise<void> {
        try {
            await copyToClipboard(text)
        } catch {
            // The browser demo and tests have no backend; the web clipboard is the honest fallback.
            await navigator.clipboard.writeText(text)
        }
        copied = true
        clearTimeout(resetTimer)
        resetTimer = setTimeout(() => {
            copied = false
        }, COPIED_MS)
    }

    $effect(() => () => {
        clearTimeout(resetTimer)
    })

    const name = $derived(copied ? tString('ui.copyButton.copied') : label)
</script>

<button type="button" class="copy-button hit-target" onclick={copy} aria-label={name} use:tooltip={name}>
    <Icon name={copied ? 'check' : 'copy'} size={12} aria-hidden="true" />
</button>

<style>
    .copy-button {
        flex: none;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 18px;
        height: 18px;
        padding: 0;
        background: none;
        border: none;
        border-radius: var(--radius-xs);
        color: var(--color-text-tertiary);
        transition:
            background var(--transition-fast),
            color var(--transition-fast);
    }

    .copy-button:hover {
        background: var(--color-tint-hover);
        color: var(--color-text-primary);
    }

    .copy-button:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
    }
</style>
