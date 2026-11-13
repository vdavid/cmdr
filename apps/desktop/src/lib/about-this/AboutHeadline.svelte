<!--
  The card's one-line summary, worded from the typed parts `cmdr-about::headline` derives: what it is, its size, and
  the single strongest origin or use fact. Two lines are reserved, since it may grow as facts land.
-->
<script lang="ts">
    import type { Snippet } from 'svelte'
    import type { Headline } from '#lib/ipc/bindings.js'
    import { t, tString } from '#lib/intl/messages.svelte.js'
    import Trans from '#lib/intl/Trans.svelte'
    import Size from '#lib/ui/Size.svelte'
    import DateLabel from '#lib/ui/DateLabel.svelte'
    import { KIND_LABEL, shortCommitId } from './about-words'

    interface Props {
        headline: Headline | null
        /** The facts have settled: the second line it held for a longer headline goes, with the other reservations. */
        settled: boolean
    }

    const { headline, settled }: Props = $props()

    function whatText(what: Headline['what']): string {
        switch (what.type) {
            case 'file':
                if (what.kind) return tString(KIND_LABEL[what.kind])
                return what.extension
                    ? tString('aboutThis.what.fileWithExtension', { extension: what.extension })
                    : tString('aboutThis.what.file')
            case 'folder':
                return tString('aboutThis.what.folder')
            case 'app':
                return tString('aboutThis.what.app', { version: what.version ?? '', hasVersion: what.version ? 'yes' : 'no' })
            case 'package':
                return tString('aboutThis.what.package')
            case 'symlink':
                return tString('aboutThis.what.symlink')
            case 'archiveEntry':
                return tString('aboutThis.what.archiveEntry')
        }
    }

    const base = $derived(
        headline
            ? {
                  what: whatText(headline.what),
                  sizeShown: headline.size === null ? 'none' : headline.size.atLeast ? 'atLeast' : 'exact',
              }
            : null,
    )
</script>

<p class="headline" class:reserving={!settled} data-headline>
    {#if headline && base}
        {@const lead = headline.lead}
        {@const bytes = headline.size?.bytes ?? null}
        {#snippet size(_content: Snippet)}<Size {bytes} />{/snippet}
        {#if lead.template === 'insideArchive'}
            <Trans key="aboutThis.headline.insideArchive" params={{ ...base, archive: lead.archive }} snippets={{ size }} />
        {:else if lead.template === 'fromGitHistory'}
            <Trans
                key="aboutThis.headline.fromGitHistory"
                params={{ ...base, kind: lead.kind, name: lead.kind === 'commit' ? shortCommitId(lead.name) : lead.name }}
                snippets={{ size }}
            />
        {:else if lead.template === 'downloadedFrom'}
            {@const at = lead.at}
            {#snippet date(_content: Snippet)}<DateLabel modifiedAt={at} />{/snippet}
            <Trans
                key="aboutThis.headline.downloadedFrom"
                params={{ ...base, host: lead.host, app: lead.by ?? '', hasApp: lead.by ? 'yes' : 'no', hasDate: at !== null ? 'yes' : 'no' }}
                snippets={{ size, date }}
            />
        {:else if lead.template === 'putHereByCmdr'}
            {@const at = lead.at}
            {#snippet date(_content: Snippet)}<DateLabel modifiedAt={at} />{/snippet}
            <Trans
                key="aboutThis.headline.putHereByCmdr"
                params={{ ...base, kind: lead.kind, volume: lead.sourceVolume ?? '', hasVolume: lead.sourceVolume ? 'yes' : 'no' }}
                snippets={{ size, date }}
            />
        {:else if lead.template === 'receivedByMail' || lead.template === 'receivedByAirDrop'}
            {@const at = lead.at}
            {#snippet date(_content: Snippet)}<DateLabel modifiedAt={at} />{/snippet}
            <Trans
                key={lead.template === 'receivedByMail' ? 'aboutThis.headline.receivedByMail' : 'aboutThis.headline.receivedByAirDrop'}
                params={{ ...base, hasDate: at !== null ? 'yes' : 'no' }}
                snippets={{ size, date }}
            />
        {:else if lead.template === 'dataOfApp'}
            <Trans
                key="aboutThis.headline.dataOfApp"
                params={{ ...base, app: lead.app, certainty: lead.certainty }}
                snippets={{ size }}
            />
        {:else if lead.template === 'dataOfMissingApp'}
            <Trans key="aboutThis.headline.dataOfMissingApp" params={{ ...base, bundleId: lead.bundleId }} snippets={{ size }} />
        {:else if lead.template === 'downloadedBy'}
            {@const at = lead.at}
            {#snippet date(_content: Snippet)}<DateLabel modifiedAt={at} />{/snippet}
            <Trans
                key="aboutThis.headline.downloadedBy"
                params={{ ...base, app: lead.app, hasDate: at !== null ? 'yes' : 'no' }}
                snippets={{ size, date }}
            />
        {:else if lead.template === 'lastOpened'}
            {@const at = lead.at}
            {#snippet date(_content: Snippet)}<DateLabel modifiedAt={at} />{/snippet}
            <Trans key="aboutThis.headline.lastOpened" params={base} snippets={{ size, date }} />
        {:else}
            <Trans key="aboutThis.headline.plain" params={base} snippets={{ size }} />
        {/if}
    {:else}
        <span class="skeleton" aria-hidden="true"></span><span class="sr-only">{t('aboutThis.dialog.lookingUp')}</span>
    {/if}
</p>

<style>
    .headline {
        margin: 0;
        font-size: var(--font-size-md);
        color: var(--color-text-primary);
    }

    .headline.reserving {
        min-height: calc(2 * var(--font-line-height-normal) * 1em);
    }

    .skeleton {
        display: inline-block;
        width: 60%;
        height: 0.8em;
        border-radius: var(--radius-xs);
        background: var(--color-bg-tertiary);
        vertical-align: middle;
    }
</style>
