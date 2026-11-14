<!--
  "What's inside" for a folder, from the digest `cmdr-about::digest` shaped: how many items and of what kinds, what
  the folder is when its name says (installed packages, build output), its biggest children, the largest files at any
  depth, and how much sat untouched for a year. Rows of the section's `<dl>`, beside `AboutRow`'s.

  An empty folder says so in one line and nothing more. ❌ No rule here: the order, the cut, the class, and every
  "at least" arrive typed.
-->
<script lang="ts">
    import type { Snippet } from 'svelte'
    import type { FolderDigest, SizedChild } from '#lib/ipc/bindings.js'
    import { t, tString } from '#lib/intl/messages.svelte.js'
    import Trans from '#lib/intl/Trans.svelte'
    import { formatInteger, getNumberFormatter } from '#lib/intl/number-format.js'
    import { formatConjunctionList } from '#lib/intl/list-format.js'
    import Size from '#lib/ui/Size.svelte'
    import { parentOrRoot, pathRelativeTo } from '#lib/path/grammar.js'
    import AboutAbsence from './AboutAbsence.svelte'
    import AboutPlaceList, { type PlaceItem } from './AboutPlaceList.svelte'
    import type { AboutGoTarget } from './about-card'
    import { CHILD_KIND, FOLDER_CLASS } from './about-words'

    interface Props {
        digest: FolderDigest
        /** The folder the card is about, so a deep file says where under it it sits. */
        folderPath: string
        onGoTo: (target: AboutGoTarget) => void
    }

    const { digest, folderPath, onGoTo }: Props = $props()

    /** What the index listed directly inside, or `null` when it hasn't listed the folder. */
    const children = $derived(digest.children.state === 'known' ? digest.children.value : null)
    const kinds = $derived(
        formatConjunctionList(
            (children?.kindMix ?? []).map(({ kind, count }) =>
                tString(CHILD_KIND[kind], { count, countText: formatInteger(count) }),
            ),
        ),
    )

    /** Where a deep file sits, relative to the card's folder; `null` for one directly in it. */
    function whereInside(path: string): string | null {
        const inside = pathRelativeTo(parentOrRoot(path), folderPath)
        return inside ? tString('aboutThis.contents.inFolder', { path: inside }) : null
    }

    function items(children: SizedChild[], deep: boolean): PlaceItem[] {
        return children.map((child) => ({
            name: child.name,
            path: child.path,
            isFolder: child.isFolder,
            size: child.size,
            detail: deep ? whereInside(child.path) : null,
        }))
    }

    const percent = getNumberFormatter({ style: 'percent', maximumFractionDigits: 0 })
</script>

<div class="row" data-row-kind="folderDigest">
    <dt>{t('aboutThis.label.contents')}</dt>
    <dd>
        {#if children}
            <span class="line"
                >{t('aboutThis.contents.summary', {
                    count: children.count,
                    countText: formatInteger(children.count),
                    kinds,
                })}</span
            >
        {:else if digest.children.state === 'unknown'}
            <span class="line"><AboutAbsence kind="folderDigest" unknown={digest.children.value} /></span>
        {:else if digest.children.state === 'notApplicable'}
            <span class="line"><AboutAbsence kind="folderDigest" notApplicable={digest.children.value} /></span>
        {/if}
        {#if digest.class !== 'plain'}<span class="line quiet">{t(FOLDER_CLASS[digest.class])}</span>{/if}
    </dd>
</div>

<!-- An empty folder says so in its one line; one the index hasn't listed has nothing more to show either. -->
{#if children && children.count > 0}
    <div class="row">
        <dt>{t('aboutThis.label.biggest')}</dt>
        <dd><AboutPlaceList items={items(children.biggest, false)} {onGoTo} /></dd>
    </div>

    <div class="row" data-row-kind="largestFiles">
        <dt>{t('aboutThis.label.largestFiles')}</dt>
        <dd>
            {#if digest.largestFiles.state === 'known'}
                <AboutPlaceList items={items(digest.largestFiles.value, true)} {onGoTo} />
            {:else if digest.largestFiles.state === 'unknown' && digest.largestFiles.value.why === 'notIndexed'}
                <!-- The search arena isn't in memory: said as when the list shows up, ❌ never as "no files". -->
                <span class="quiet">{t('aboutThis.contents.notLoaded')}</span>
            {:else if digest.largestFiles.state === 'unknown'}
                <AboutAbsence kind="folderDigest" unknown={digest.largestFiles.value} />
            {:else if digest.largestFiles.state === 'notApplicable'}
                <AboutAbsence kind="folderDigest" notApplicable={digest.largestFiles.value} />
            {/if}
        </dd>
    </div>

    <!-- Read from the same search as the largest files, so it shows when they do; the line above already says why
         neither is there yet. -->
    {#if digest.unchangedForAYear.state === 'known' && digest.unchangedForAYear.value.of > 0}
        {@const share = digest.unchangedForAYear.value}
        {#snippet untouchedBytes(_content: Snippet)}<Size bytes={share.bytes} />{/snippet}
        <div class="row">
            <dt>{t('aboutThis.label.unchanged')}</dt>
            <dd>
                <Trans
                    key="aboutThis.contents.unchanged"
                    params={{ atLeast: share.atLeast ? 'yes' : 'no', percent: percent.format(share.bytes / share.of) }}
                    snippets={{ size: untouchedBytes }}
                />
            </dd>
        </div>
    {:else if digest.unchangedForAYear.state === 'unknown' && digest.largestFiles.state === 'known'}
        <!-- The search answered and the share still can't be told (the folder's total is a lower bound). -->
        <div class="row">
            <dt>{t('aboutThis.label.unchanged')}</dt>
            <dd><AboutAbsence kind="folderDigest" unknown={digest.unchangedForAYear.value} /></dd>
        </div>
    {/if}
{/if}

<style>
    .row {
        display: grid;
        grid-template-columns: 8.5rem minmax(0, 1fr);
        column-gap: var(--spacing-md);
    }

    dt {
        color: var(--color-text-secondary);
    }

    dd {
        margin: 0;
        min-width: 0;
        overflow-wrap: break-word;
    }

    .line {
        display: block;
    }

    .quiet {
        color: var(--color-text-tertiary);
    }
</style>
