<script lang="ts">
    /**
     * One list of the coverage note, as one line: an icon holding the folders, the
     * sentence, and a quiet copy button.
     *
     * The folders used to print in full under their sentence, and after a search over
     * `/` eighteen refused folders covered every result at the default dialog height.
     * Now they sit in the icon's tooltip, numbered so the count checks out at a glance,
     * capped at `TOOLTIP_FOLDER_CAP` with "+N more" because a tooltip never scrolls. The
     * copy button is the way to the whole list, one path per line.
     *
     * The icon is an `InfoTip`, so the list is one Tab away for keyboard and screen
     * reader users too, and its tone follows `line.actionable`: a warning where the note
     * offers a way out for this line, an info glyph where nothing is asked.
     */
    import InfoTip from '#lib/ui/InfoTip.svelte'
    import CopyButton from '#lib/ui/CopyButton.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { formatNumber } from '#lib/file-explorer/selection/selection-info-utils.js'
    import { foldersAsText, tooltipFolders, type CoverageLine } from './coverage-note'

    interface Props {
        line: CoverageLine
        /** The line's sentence, already in the user's language. */
        message: string
    }

    const { line, message }: Props = $props()

    const folders = $derived(tooltipFolders(line.paths))
</script>

<div class="coverage-line" data-kind={line.kind}>
    <InfoTip label={tString('search.coverage.list.show')} tone={line.actionable ? 'warning' : 'info'} size={12}>
        <ol class="folders">
            {#each folders.shown as path (path)}
                <li><code>{path}</code></li>
            {/each}
        </ol>
        {#if folders.more > 0}
            <p class="more">
                {tString('search.coverage.list.more', { count: folders.more, countText: formatNumber(folders.more) })}
            </p>
        {/if}
    </InfoTip>
    <span class="message">{message}</span>
    <CopyButton text={foldersAsText(line.paths)} label={tString('search.coverage.list.copy')} />
</div>

<style>
    .coverage-line {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    /* The glyph leads here, so its "follows a word" margin would only push it off the edge. */
    .coverage-line :global(.info-tip) {
        margin-left: 0;
    }

    .message {
        min-width: 0;
    }

    /* These two render inside the tooltip (the InfoTip hands them over as its body). */
    .folders {
        margin: 0;
        padding-left: var(--spacing-lg);
    }

    .folders code {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        background: none;
        padding: 0;
        overflow-wrap: break-word;
    }

    .more {
        margin: var(--spacing-xxs) 0 0;
        font-size: var(--font-size-xs);
    }
</style>
