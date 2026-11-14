<!--
  "Its data in your Library" for an app: the Library folders named after its bundle id, which removing the app leaves
  behind, with their total and the places Cmdr's index hasn't listed yet. From the index only: ❌ the card never opens
  a container. The answer of the `appData` row, inside `AboutRow`'s `<dd>`.
-->
<script lang="ts">
    import type { Snippet } from 'svelte'
    import type { AppData } from '#lib/ipc/bindings.js'
    import { t, tString } from '#lib/intl/messages.svelte.js'
    import Trans from '#lib/intl/Trans.svelte'
    import { formatInteger } from '#lib/intl/number-format.js'
    import { formatConjunctionList } from '#lib/intl/list-format.js'
    import Size from '#lib/ui/Size.svelte'
    import AboutPlaceList, { type PlaceItem } from './AboutPlaceList.svelte'
    import type { AboutGoTarget } from './about-card'
    import { OWNER_PLACE } from './about-words'

    interface Props {
        data: AppData
        onGoTo: (target: AboutGoTarget) => void
    }

    const { data, onGoTo }: Props = $props()

    const count = $derived(data.folders.length)
    const items = $derived<PlaceItem[]>(
        // Every folder is named after the same id, so the Library place leads and the name follows, muted.
        data.folders.map((folder) => ({
            name: tString(OWNER_PLACE[folder.place]),
            path: folder.path,
            isFolder: true,
            size: folder.size,
            detail: folder.name,
        })),
    )
    const notLookedIn = $derived(formatConjunctionList(data.notLookedIn.map((place) => tString(OWNER_PLACE[place]))))
</script>

{#snippet total(_content: Snippet)}<Size bytes={data.totalBytes} />{/snippet}
<span class="line">
    <Trans
        key="aboutThis.appData.summary"
        params={{ count, countText: formatInteger(count), atLeast: data.totalIsLowerBound ? 'yes' : 'no' }}
        snippets={{ size: total }}
    />
</span>
{#if count > 0}<AboutPlaceList {items} {onGoTo} />{/if}
{#if data.notLookedIn.length > 0}
    <span class="line quiet">{t('aboutThis.appData.notLookedIn', { places: notLookedIn })}</span>
{/if}

<style>
    .line {
        display: block;
    }

    .quiet {
        color: var(--color-text-tertiary);
    }
</style>
