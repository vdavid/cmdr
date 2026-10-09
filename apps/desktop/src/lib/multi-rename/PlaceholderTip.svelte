<script lang="ts">
    /**
     * The body of one placeholder button's tooltip: what the placeholder means, an example from the
     * batch's first file, a few of its other forms, each with its example, the part a form takes set
     * apart from what the field keeps around it. Rendered into a hidden host and handed to the
     * tooltip as `contentEl` (`$lib/ui/DETAILS.md` § Live rich content), so it reads as text to
     * `aria-describedby` too.
     */
    import { tString } from '$lib/intl/messages.svelte'
    import { FULL_NAME_MASK, examplePieces, type PlaceholderHelp } from './placeholder-help'

    interface Props {
        help: PlaceholderHelp
        /** Each example mask's text for the first file; `null` while there's none to show. */
        rendered: ReadonlyMap<string, string> | null
        /** The first file has no modified time, so dates show a sample one. */
        sampleDate: boolean
        /** The element the tooltip adopts. */
        contentEl?: HTMLDivElement
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let { help, rendered, sampleDate, contentEl = $bindable() }: Props = $props()
    /* eslint-enable prefer-const */

    /** What `SAMPLE_MODIFIED` in `plan.rs` is, for a file with no modified time. */
    const SAMPLE_DATE_TEXT = '2026-06-15 23:10:09'

    const known = $derived(rendered ?? new Map<string, string>())
    const example = $derived(examplePieces({ mask: help.example === 'counter' ? '[C]' : help.placeholder }, known))
    const lead = $derived.by(() => {
        if (help.example === 'counter') return tString('multiRename.placeholderHelp.forFirstFiles')
        if (help.example === 'date' && sampleDate) {
            return tString('multiRename.placeholderHelp.forSampleDate', { date: SAMPLE_DATE_TEXT })
        }
        const file = known.get(FULL_NAME_MASK)
        return file === undefined ? null : tString('multiRename.placeholderHelp.forFile', { file })
    })
</script>

<div hidden>
    <div class="placeholder-tip" bind:this={contentEl}>
        <p class="head"><span class="mask">{help.placeholder}</span> {tString(help.meaning)}</p>
        {#if lead !== null && example !== null}
            <p class="lead">
                {lead}
                <span class="example">
                    {#each example as part, i (i)}<span class:taken={part.taken}>{part.text}</span>{/each}
                </span>
            </p>
        {/if}
        <div class="forms">
            {#each help.syntax as line (line.mask)}
                {@const parts = examplePieces(line, known)}
                <span class="mask">{line.mask}</span>
                <span class="meaning">{tString(line.meaning, line.meaningParams)}</span>
                {#if parts !== null && (lead !== null || help.example === 'counter')}
                    <span class="example">
                        {#each parts as part, i (i)}<span class:taken={part.taken}>{part.text}</span>{/each}
                    </span>
                {:else}
                    <span></span>
                {/if}
            {/each}
        </div>
        {#if help.footnote}
            <p class="footnote">{tString(help.footnote)}</p>
        {/if}
    </div>
</div>

<style>
    .placeholder-tip {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        padding: var(--spacing-xxs) 0;
        /* The tooltip keeps newlines (`pre-line`); this markup's own line breaks aren't text. */
        white-space: normal;
    }

    p {
        margin: 0;
    }

    .mask,
    .example {
        font-family: var(--font-mono);
    }

    .head .mask {
        font-weight: 600;
    }

    .lead {
        color: var(--color-text-secondary);
    }

    /* Form, meaning, example: three columns, so the forms and their examples line up. */
    .forms {
        display: grid;
        grid-template-columns: auto auto auto;
        gap: var(--spacing-xxs) var(--spacing-md);
        align-items: baseline;
        padding-top: var(--spacing-xs);
        border-top: 1px solid var(--color-border-glass);
    }

    .meaning {
        color: var(--color-text-secondary);
    }

    /* What the form leaves around its part is quiet, the part it takes stands out. */
    .example {
        color: var(--color-text-tertiary);
        white-space: pre-wrap;
        overflow-wrap: anywhere;
    }

    .example .taken {
        color: var(--color-text-primary);
        font-weight: 600;
    }

    .footnote {
        color: var(--color-text-secondary);
    }
</style>
