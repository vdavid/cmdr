<script lang="ts">
    /**
     * The once-ever toast for a key that ran its command instead of typing into the
     * quick filter (`quick-filter-typing-hint.ts`). It hides by itself, and the
     * frame's own × closes it sooner.
     */
    import type { Snippet } from 'svelte'
    import Trans from '#lib/intl/Trans.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import ShortcutChip from '#lib/ui/ShortcutChip.svelte'

    interface Props {
        /** The key by its name (`*`, `Space`); the chip spells it the way shortcut lists do. */
        keyLabel: string
        /** The typing window in seconds, for the plural. */
        seconds: number
        /** The same, formatted for the UI language (`1.5`, `1,5`). */
        secondsText: string
    }

    const { keyLabel, seconds, secondsText }: Props = $props()
</script>

<!-- The chip draws the key from `keyLabel`; the tag's `{key}` text is there for translators. -->
{#snippet keyName(_children: Snippet)}<ShortcutChip key={keyLabel} />{/snippet}

<div class="content">
    <strong class="title">{tString('fileExplorer.quickFilter.typingHint.title')}</strong>
    <span class="body">
        <Trans
            key="fileExplorer.quickFilter.typingHint.body"
            params={{ key: keyLabel, seconds, secondsText }}
            snippets={{ keyName }}
        />
    </span>
</div>

<style>
    /* Block flow, so the text wraps around the toast's corner float (`ui/DETAILS.md` § Toast system). */
    .content {
        font-size: var(--font-size-sm);
    }

    .title,
    .body {
        display: block;
        color: var(--color-text-primary);
    }

    .body {
        margin-top: var(--spacing-xxs);
    }

    .title {
        font-weight: 600;
    }
</style>
