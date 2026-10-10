<script lang="ts">
    /**
     * The body of one placeholder button's tooltip: what the placeholder means, an example on a
     * made-up file, a few of its other forms, each with its example, the part a form takes set
     * apart from what the field keeps around it. Rendered into a hidden host and handed to the
     * tooltip as `contentEl` (`$lib/ui/DETAILS.md` § Live rich content), so it reads as text to
     * `aria-describedby` too.
     */
    import { tString } from '$lib/intl/messages.svelte'
    import { DATE_LEAD_MASK, FOLDER_LEAD_MASK, hintPieces, type PlaceholderHelp } from './placeholder-help'
    import { SAMPLE_FILE } from './rename-examples'

    interface Props {
        help: PlaceholderHelp
        /** Each example's text, keyed as asked; `null` until the engine rendered them. */
        rendered: ReadonlyMap<string, string> | null
        /** The element the tooltip adopts. */
        contentEl?: HTMLDivElement
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let { help, rendered, contentEl = $bindable() }: Props = $props()
    /* eslint-enable prefer-const */

    const known = $derived(rendered ?? new Map<string, string>())
    const example = $derived(hintPieces({ mask: help.example === 'counter' ? '[C]' : help.placeholder }, known))
    const lead = $derived.by(() => {
        switch (help.example) {
            case 'counter':
                return tString('multiRename.placeholderHelp.forFirstFiles')
            case 'file':
                return tString('multiRename.placeholderHelp.forFile', { file: SAMPLE_FILE })
            case 'folder': {
                const path = known.get(FOLDER_LEAD_MASK)
                return path === undefined ? null : tString('multiRename.placeholderHelp.forFile', { file: path })
            }
            case 'date': {
                const date = known.get(DATE_LEAD_MASK)
                return date === undefined ? null : tString('multiRename.placeholderHelp.forSampleDate', { date })
            }
        }
    })
</script>

<div hidden>
    <div class="placeholder-tip" bind:this={contentEl}>
        <p class="head"><span class="mask">{help.placeholder}</span> {tString(help.meaning)}</p>
        {#if lead !== null && example !== null}
            <p class="lead">
                {lead}
                <span class="example">
                    {#each example as part, i (i)}<span class:marked={part.marked}>{part.text}</span>{/each}
                </span>
            </p>
        {/if}
        <div class="forms">
            {#each help.syntax as line (line.mask)}
                {@const parts = hintPieces(line, known)}
                <span class="mask">{line.mask}</span>
                <span class="meaning">{tString(line.meaning, line.meaningParams)}</span>
                {#if parts !== null}
                    <span class="example">
                        {#each parts as part, i (i)}<span class:marked={part.marked}>{part.text}</span>{/each}
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

    .example .marked {
        color: var(--color-text-primary);
        font-weight: 600;
    }

    .footnote {
        color: var(--color-text-secondary);
    }
</style>
