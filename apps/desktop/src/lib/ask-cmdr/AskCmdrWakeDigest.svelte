<!--
  What a wake noticed, opening a thread the agent started for itself.

  ⚠️ The backend hands this over as COUNTS AND PATHS, never as the sentence the model read
  (`agent/llm/types.rs`, `WakeDigest`). Every word here is ours and is translated; the English
  digest is a prompt and stays one. So: never render a backend string in this block.

  Collapsed by default. The thread's own title already names the busiest folder, and the
  interesting part of a wake is what the agent SAID about the activity, not the tally that
  prompted it. Expanding shows the per-folder breakdown, paths as escaped plain text (a folder
  name is attacker-controlled), never {@html}.
-->
<script lang="ts">
    import Icon from '$lib/ui/Icon.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { tString } from '$lib/intl/messages.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import type { WakeDigestFolderView, WakeDigestRollupView } from '$lib/tauri-commands'

    interface Props {
        folders: WakeDigestFolderView[]
        rollups: WakeDigestRollupView[]
    }
    const { folders, rollups }: Props = $props()

    let expanded = $state(false)

    /** A count as both the preformatted string the sentence drops in and the raw integer
     *  that picks the plural form (the catalogs' `*Text` convention). */
    function counts(count: number): { countText: string; count: number } {
        return { countText: formatInteger(count), count }
    }

    /** Every folder the digest covered, the rolled-up ones included: the summary line has to
     *  agree with what expanding reveals, and a rollup stands for real folders. */
    const totalFolders = $derived(folders.length + rollups.reduce((sum, rollup) => sum + rollup.folders, 0))

    const summary = $derived(
        tString('askCmdr.wakeDigest.summary', {
            foldersText: formatInteger(totalFolders),
            folders: totalFolders,
        }),
    )

    /** The four kinds that happened in one folder, in the order the counters are declared.
     *  A kind with a zero count says nothing: a line of zeroes is noise. */
    function changeLines(folder: WakeDigestFolderView): string[] {
        const kinds = [
            ['askCmdr.wakeDigest.created', folder.created],
            ['askCmdr.wakeDigest.modified', folder.modified],
            ['askCmdr.wakeDigest.removed', folder.removed],
            ['askCmdr.wakeDigest.renamed', folder.renamed],
        ] as const
        const lines = kinds.filter(([, count]) => count > 0).map(([key, count]) => tString(key, counts(count)))
        // A folder whose counters are all zero is possible (a batch that coalesced itself
        // away), and saying nothing at all about it would read as a rendering bug.
        return lines.length > 0 ? lines : [tString('askCmdr.wakeDigest.noChanges')]
    }

    function rollupLine(rollup: WakeDigestRollupView): string {
        return tString('askCmdr.wakeDigest.rollup', {
            foldersText: formatInteger(rollup.folders),
            folders: rollup.folders,
            ancestor: rollup.ancestor,
            changesText: formatInteger(rollup.changes),
            changes: rollup.changes,
        })
    }
</script>

<div class="wake-digest">
    <button type="button" class="digest-toggle" aria-expanded={expanded} onclick={() => (expanded = !expanded)}>
        <span class="glyph"><Icon name="bot" size={13} aria-hidden="true" /></span>
        <span class="label">{summary}</span>
        <span class="chevron">
            <Icon name={expanded ? 'chevron-down' : 'chevron-right'} size={13} aria-hidden="true" />
        </span>
    </button>
    {#if expanded}
        <ul class="detail">
            {#each folders as folder (folder.folder)}
                <li>
                    <span class="path" use:tooltip={{ text: folder.folder, overflowOnly: true }}>{folder.folder}</span>
                    <span class="counts">{changeLines(folder).join(', ')}</span>
                </li>
            {/each}
            {#each rollups as rollup (rollup.ancestor)}
                <li class="rollup">{rollupLine(rollup)}</li>
            {/each}
        </ul>
    {/if}
</div>

<style>
    .wake-digest {
        display: flex;
        flex-direction: column;
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-md);
    }

    .digest-toggle {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        width: 100%;
        min-height: 28px;
        padding: var(--spacing-xxs) var(--spacing-xs);
        border: none;
        background: none;
        color: inherit;
        font: inherit;
        text-align: left;
        border-radius: var(--radius-md);
    }

    .digest-toggle:hover {
        background: var(--color-bg-secondary);
    }

    .glyph {
        display: flex;
        width: 16px;
        justify-content: center;
        flex: none;
        color: var(--color-text-tertiary);
    }

    .label {
        flex: 1;
        min-width: 0;
    }

    .chevron {
        display: flex;
        flex: none;
        color: var(--color-text-tertiary);
    }

    .detail {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        margin: 0;
        padding: 0 var(--spacing-xs) var(--spacing-xs) calc(var(--spacing-xs) + 20px);
        list-style: none;
    }

    .detail li {
        display: flex;
        flex-direction: column;
        min-width: 0;
    }

    .path {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        font-family: var(--font-mono);
        color: var(--color-text-secondary);
    }

    .counts,
    .rollup {
        color: var(--color-text-tertiary);
    }
</style>
