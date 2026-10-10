<script lang="ts">
    /**
     * The five search options as toggle chips beside the search fields, the way code editors
     * draw their find options: a few characters each (`Aa`, `.*`), on in the accent fill. Each is
     * a real toggle button (`aria-pressed`, its full name as `aria-label`), and its tooltip gives
     * the name, its ⌘⌥ key, and a tiny replace on a made-up file with the option on and off, the
     * text the replace put in set apart (`search-option-help.ts`).
     */
    import type { Snippet } from 'svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import type { MultiRenameSpec } from '$lib/tauri-commands'
    import { TOGGLE_COMMANDS } from './option-keys'
    import { examplePieces } from './rename-examples'
    import { SEARCH_OPTIONS, searchExampleKey, type SearchOptionField } from './search-option-help'

    interface Props {
        spec: MultiRenameSpec
        onToggle: (field: SearchOptionField) => void
        /** Each example's text, keyed as asked; `null` until the engine rendered them. */
        rendered: ReadonlyMap<string, string> | null
    }

    const { spec, onToggle, rendered }: Props = $props()

    /** Each chip's tooltip body, which the tooltip adopts on show. */
    const tips = $state<Partial<Record<SearchOptionField, HTMLDivElement>>>({})

    function results(field: SearchOptionField) {
        const on = rendered?.get(searchExampleKey(field, true))
        const off = rendered?.get(searchExampleKey(field, false))
        return on === undefined || off === undefined ? null : { on: examplePieces(on), off: examplePieces(off) }
    }
</script>

<div class="search-options" role="group" aria-label={tString('multiRename.searchOptions')}>
    {#each SEARCH_OPTIONS as option (option.field)}
        <button
            type="button"
            class="option-chip"
            aria-pressed={spec[option.field]}
            aria-label={tString(option.label)}
            onclick={() => { onToggle(option.field) }}
            use:tooltip={{ contentEl: tips[option.field] }}
        >
            <span aria-hidden="true">{option.glyph}</span>
        </button>
    {/each}
</div>

{#each SEARCH_OPTIONS as option (option.field)}
    {@const shown = results(option.field)}
    <div hidden>
        <div class="search-option-tip" bind:this={tips[option.field]}>
            <p class="head">
                <span class="name">{tString(option.label)}</span>
                <ShortcutChip commandId={TOGGLE_COMMANDS[option.field]} clickable={false} size="sm" />
            </p>
            {#if shown}
                <p class="setup">
                    <Trans
                        key="multiRename.searchOptionHelp.setup"
                        params={{ search: option.search, replace: option.replace, name: option.fileName }}
                        snippets={{ code }}
                    />
                </p>
                <div class="results">
                    <span class="state">{tString('multiRename.searchOptionHelp.on')}</span>
                    <span class="example">
                        {#each shown.on as part, i (i)}<span class:marked={part.marked}>{part.text}</span>{/each}
                    </span>
                    <span class="state">{tString('multiRename.searchOptionHelp.off')}</span>
                    <span class="example">
                        {#each shown.off as part, i (i)}<span class:marked={part.marked}>{part.text}</span>{/each}
                    </span>
                </div>
            {/if}
        </div>
    </div>
{/each}

{#snippet code(children: Snippet)}
    <span class="code">{@render children()}</span>
{/snippet}

<style>
    .search-options {
        display: flex;
        gap: var(--spacing-xxs);
    }

    /* As tall as the text fields beside it: their frame's own recipe (`app-field.css`). */
    .option-chip {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        min-width: calc(var(--font-size-input) * var(--font-line-height-tight) + 2 * var(--spacing-input) + 2px);
        height: calc(var(--font-size-input) * var(--font-line-height-tight) + 2 * var(--spacing-input) + 2px);
        padding: 0 var(--spacing-xs);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        background: var(--color-bg-primary);
        color: var(--color-text-secondary);
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
        font-weight: 600;
        white-space: nowrap;
        transition:
            background var(--transition-base),
            border-color var(--transition-base),
            color var(--transition-base);
    }

    .option-chip:hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    /* On: the accent fill, as a chosen `ToggleGroup` cell. */
    .option-chip[aria-pressed='true'] {
        border-color: var(--color-accent);
        background: var(--color-accent);
        color: var(--color-accent-fg);
    }

    .option-chip[aria-pressed='true']:hover {
        background: var(--color-accent-hover);
    }

    .option-chip:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
        box-shadow: var(--shadow-focus);
    }

    .search-option-tip {
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

    .head {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-md);
    }

    .name {
        font-weight: 600;
    }

    .setup {
        color: var(--color-text-secondary);
    }

    .code,
    .example {
        font-family: var(--font-mono);
    }

    .code {
        color: var(--color-text-primary);
    }

    /* On / Off, then the name the replace makes: two columns, so the names line up. */
    .results {
        display: grid;
        grid-template-columns: auto auto;
        justify-content: start;
        gap: var(--spacing-xxs) var(--spacing-sm);
        align-items: baseline;
    }

    .state {
        color: var(--color-text-secondary);
    }

    /* What the replace left alone is quiet; what it put in stands out. */
    .example {
        color: var(--color-text-tertiary);
        white-space: pre-wrap;
        overflow-wrap: anywhere;
    }

    .example .marked {
        color: var(--color-text-primary);
        font-weight: 600;
    }
</style>
