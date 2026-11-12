<!--
  An expanded tool line: a curated look at what the call handed the model (its first rows, how
  much it got, the sentence it was told when refused), plus a low-key Copy for the raw JSON.

  Names, paths, snippets, and the `problem` sentence are untrusted (a crafted filename, the
  model's own input echoed back), so they render as plain text, never {@html}. The raw JSON
  renders nowhere: it's private (file text, photo locations) and can be large, so only Copy
  carries it.
-->
<script lang="ts">
    import Button from '#lib/ui/Button.svelte'
    import LinkButton from '#lib/ui/LinkButton.svelte'
    import Size from '#lib/ui/Size.svelte'
    import Spinner from '#lib/ui/Spinner.svelte'
    import { pathParam, tString } from '#lib/intl/messages.svelte.js'
    import { formatInteger } from '#lib/intl/number-format.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { copyToClipboard } from '#lib/tauri-commands/index.js'
    import type { ToolPreviewRow } from '#lib/ipc/bindings.js'
    import { previewNotes, previewStatusText } from './ask-cmdr-tool-preview'
    import type { ToolDetailState } from './ask-cmdr-tool-groups.svelte'
    import type { RevealPath } from './ask-cmdr-tool-line'
    import { returnFocusToPane } from './ask-cmdr-trigger.svelte'

    interface Props {
        /** What the line fetched; `undefined` until its fetch starts. */
        detail: ToolDetailState | undefined
        id: string
        revealPath: RevealPath
    }
    const { detail, id, revealPath }: Props = $props()

    const ready = $derived(detail?.status === 'ready' ? detail.detail : null)
    const notes = $derived(ready ? previewNotes(ready.preview) : null)

    let copy = $state<'idle' | 'copied' | 'refused'>('idle')
    let copyTimer: ReturnType<typeof setTimeout> | undefined

    async function copyRaw(rawJson: string): Promise<void> {
        clearTimeout(copyTimer)
        try {
            await copyToClipboard(rawJson)
            copy = 'copied'
            copyTimer = setTimeout(() => (copy = 'idle'), 2000)
        } catch {
            // ❌ Nothing logged: the text is the private result.
            copy = 'refused'
        }
    }

    /** Show the row in a pane, then hand focus back to the panes, where the user is now looking. */
    async function show(row: ToolPreviewRow & { path: string }): Promise<void> {
        await revealPath({ path: row.path, kind: row.kind === 'folder' ? 'folder' : 'file' })
        returnFocusToPane()
    }
</script>

<div class="preview" {id}>
    {#if detail === undefined || detail.status === 'loading'}
        <Spinner size="sm" label={tString('askCmdr.tool.preview.loading')} />
    {:else if detail.status === 'notFound'}
        <p class="note">{tString('askCmdr.tool.preview.notFound')}</p>
    {:else if detail.status === 'unavailable'}
        <p class="note">{tString('askCmdr.tool.preview.unavailable')}</p>
    {:else if ready && notes}
        {#if ready.preview.setAside}
            <p class="note">{tString('askCmdr.tool.preview.setAside')}</p>
        {/if}
        {#if ready.preview.problem !== null}
            <p class="problem">
                <span class="problem-label">{tString('askCmdr.tool.preview.problemLabel')}</span>
                <span>{ready.preview.problem}</span>
            </p>
        {/if}
        {#if ready.preview.rows.length > 0}
            <ul class="rows">
                {#each ready.preview.rows as row, index (index)}
                    <li class="row">
                        <span class="head">
                        {#if row.path !== null && row.kind !== 'other'}
                            {@const target = { ...row, path: row.path }}
                            <!-- Always on, like a tool line's path: the link truncates itself, so
                                 its box never reads as overflowing, and the full path says more
                                 than the name anyway. -->
                            <span class="name" use:tooltip={row.path}>
                                <LinkButton
                                    aria-label={tString('askCmdr.tool.showInPane', { path: pathParam(row.path) })}
                                    onclick={() => void show(target)}>{row.name}</LinkButton
                                >
                            </span>
                        {:else}
                            <span class="name" use:tooltip={{ text: row.name, overflowOnly: true }}>{row.name}</span>
                        {/if}
                        {#if row.sizeBytes !== null}
                            <span class="meta"><Size bytes={row.sizeBytes} /></span>
                        {/if}
                        {#if row.line !== null}
                            <span class="meta"
                                >{tString('askCmdr.tool.preview.line', { lineText: formatInteger(row.line) })}</span
                            >
                        {/if}
                        {#if row.status !== null}
                            <span class="meta">{previewStatusText(row.status)}</span>
                        {/if}
                        </span>
                        {#if row.snippet !== null}
                            <span class="snippet">{row.snippet}</span>
                        {/if}
                    </li>
                {/each}
            </ul>
        {/if}
        {#each [notes.given, notes.firstRows, notes.noRows] as note, index (index)}
            {#if note !== null}<p class="note">{note}</p>{/if}
        {/each}
        <div class="copy">
            <Button
                variant="secondary"
                size="mini"
                aria-label={tString('askCmdr.tool.preview.copyLabel')}
                onclick={() => void copyRaw(ready.rawJson)}
            >
                {copy === 'copied' ? tString('askCmdr.tool.preview.copied') : tString('askCmdr.tool.preview.copy')}
            </Button>
            {#if copy === 'refused'}
                <span class="note" role="status">{tString('askCmdr.tool.preview.copyRefused')}</span>
            {/if}
        </div>
    {/if}
</div>

<style>
    .preview {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        padding: var(--spacing-xxs) var(--spacing-xs) var(--spacing-xs) calc(var(--spacing-xs) + 20px);
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
    }

    .rows {
        display: flex;
        flex-direction: column;
        margin: 0;
        padding: 0;
        list-style: none;
    }

    .row {
        display: flex;
        flex-direction: column;
        min-width: 0;
    }

    /* The name gives way first, so its size stays on its line however long the name is. */
    .head {
        display: flex;
        align-items: baseline;
        gap: var(--spacing-xs);
        min-width: 0;
    }

    .name {
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        font-family: var(--font-mono);
    }

    /* A linked name is a `<button>`, an atomic inline box the ellipsis above can't cut into, so
       it truncates itself. */
    .name :global(.link-button) {
        max-width: 100%;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .meta {
        flex: none;
        color: var(--color-text-tertiary);
    }

    /* A search hit's line of text: its own row under the name, so a long line wraps there. */
    .snippet {
        min-width: 0;
        overflow-wrap: anywhere;
        color: var(--color-text-tertiary);
        font-family: var(--font-mono);
    }

    .note,
    .problem {
        margin: 0;
        color: var(--color-text-tertiary);
    }

    .problem {
        overflow-wrap: anywhere;
    }

    .problem-label {
        margin-right: var(--spacing-xxs);
        color: var(--color-text-secondary);
    }

    .copy {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }
</style>
