<!--
  "What removing it means": the consequences `cmdr-about::removal` derived, most painful first, the top ones shown and
  the rest behind a button. ❌ No line here ever says removing is safe: the enum has no such variant, and each line
  words exactly its variant.
-->
<script lang="ts">
    import type { Snippet } from 'svelte'
    import type { Removal, RemovalConsequence } from '#lib/ipc/bindings.js'
    import { t } from '#lib/intl/messages.svelte.js'
    import Trans from '#lib/intl/Trans.svelte'
    import { formatInteger } from '#lib/intl/number-format.js'
    import Size from '#lib/ui/Size.svelte'
    import LinkButton from '#lib/ui/LinkButton.svelte'
    import { STANDARD_FOLDER_REMOVAL } from './about-words'

    interface Props {
        removal: Removal | null
        /** Keep two lines' height while the facts land, so a longer line arriving moves nothing. */
        settled: boolean
    }

    const { removal, settled }: Props = $props()

    let expanded = $state(false)
    const uid = $props.id()
    const listId = `about-removal-${uid}`

</script>

{#snippet line(consequence: RemovalConsequence)}
    <li>
        {#if consequence.type === 'standardFolder'}{t(STANDARD_FOLDER_REMOVAL[consequence.folder])}
        {:else if consequence.type === 'uncommittedWork'}{t('aboutThis.removal.uncommittedWork')}
        {:else if consequence.type === 'holdsRepoHistory'}{t('aboutThis.removal.holdsRepoHistory')}
        {:else if consequence.type === 'untrackedNotInGit'}{t('aboutThis.removal.untrackedNotInGit')}
        {:else if consequence.type === 'ignoredByGit'}{t('aboutThis.removal.ignoredByGit')}
        {:else if consequence.type === 'userDocument'}{t('aboutThis.removal.userDocument')}
        {:else if consequence.type === 'removesApp'}
            {#if consequence.dataLeft.state === 'known'}
                {@const data = consequence.dataLeft.value}
                {@const count = data.folders.length}
                {#snippet size(_content: Snippet)}<Size bytes={data.totalBytes} />{/snippet}
                <Trans
                    key="aboutThis.removal.removesAppDataLeft"
                    params={{ count, countText: formatInteger(count), atLeast: data.totalIsLowerBound ? 'yes' : 'no' }}
                    snippets={{ size }}
                />
            {:else}
                {t('aboutThis.removal.removesApp')}
            {/if}
        {:else if consequence.type === 'appData'}{t('aboutThis.removal.appData', {
                app: consequence.app,
                certainty: consequence.certainty,
            })}
        {:else if consequence.type === 'installedBy'}
            {#if consequence.installer.type === 'receipt'}{t('aboutThis.removal.installedByReceipt', {
                    pkgId: consequence.installer.pkgId,
                    partOfOs: consequence.installer.partOfOs ? 'yes' : 'no',
                })}{:else}{t('aboutThis.removal.installedByHomebrew', { name: consequence.installer.name })}{/if}
        {:else if consequence.type === 'leftoverOfUninstalledApp'}{t('aboutThis.removal.leftoverOfUninstalledApp', {
                bundleId: consequence.bundleId,
            })}
        {:else if consequence.type === 'recordedDownload'}{t('aboutThis.removal.recordedDownload', { host: consequence.host })}
        {:else if consequence.type === 'gitCanRestore'}{t('aboutThis.removal.gitCanRestore')}
        {:else if consequence.type === 'rebuildableOutput'}{consequence.by === 'packageManager'
                ? t('aboutThis.removal.rebuildablePackageManager')
                : t('aboutThis.removal.rebuildableBuildTool')}
        {:else if consequence.type === 'regeneratedCache'}{t('aboutThis.removal.regeneratedCache', {
                app: consequence.app ?? '',
                hasApp: consequence.app ? 'yes' : 'no',
            })}
        {:else if consequence.type === 'otherNamesRemain'}{t('aboutThis.removal.otherNamesRemain', {
                count: consequence.others,
                countText: formatInteger(consequence.others),
            })}
        {:else if consequence.type === 'linkOnly'}{t('aboutThis.removal.linkOnly')}
        {:else if consequence.type === 'inGitHistory'}{t('aboutThis.removal.inGitHistory')}
        {:else}{t('aboutThis.removal.cantTell')}{/if}
    </li>
{/snippet}

<div class="removal" class:filling={!settled}>
    {#if removal}
        <ul id={listId}>
            {#each removal.shown as consequence, i (i)}{@render line(consequence)}{/each}
            {#if expanded}
                {#each removal.more as consequence, i (i)}{@render line(consequence)}{/each}
            {/if}
        </ul>
        {#if removal.more.length > 0}
            <LinkButton
                aria-expanded={expanded}
                aria-controls={listId}
                onclick={() => {
                    expanded = !expanded
                }}
            >
                {expanded
                    ? t('aboutThis.removal.less')
                    : t('aboutThis.removal.more', { count: removal.more.length, countText: formatInteger(removal.more.length) })}
            </LinkButton>
        {/if}
    {/if}
</div>

<style>
    .removal.filling {
        min-height: calc(2 * var(--font-line-height-normal) * 1em);
    }

    ul {
        margin: 0;
        padding: 0;
        list-style: none;
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
    }
</style>
