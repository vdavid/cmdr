<!--
  DateLabel: the canonical way to render a file's modified date in the UI.

  Wraps `formattedDate(modifiedAt)` from `reactive-settings.svelte.ts` so the
  date picks up the user's current `appearance.dateTimeFormat` and
  `appearance.dateColors` automatically. Each segment carries its own age-tier
  class (year/month/day/time) so the active date palette colors components
  independently; literals (separators) and tier-less segments render plain.

  Use this component anywhere you'd otherwise reach for `formatDateTime` or
  hand-roll a date string. The FullList opts out because its column-alignment
  needs the two halves rendered into specific elements, but it uses the same
  `formattedDate(...)` data — keep it that way.
-->
<script lang="ts">
    import { formattedDate } from '$lib/settings/reactive-settings.svelte'

    interface Props {
        /** Unix timestamp in seconds (matching the FileEntry convention). */
        modifiedAt: number | null | undefined
        /** Optional class for the outer wrapper, in case the parent needs to scope it. */
        class?: string
    }

    const { modifiedAt, class: className = '' }: Props = $props()

    const d = $derived(formattedDate(modifiedAt))
</script>

<span class="date-label {className}">
    {#if d.text === ''}
        <!-- Empty state: render nothing (matches the previous formatDateTime behavior). -->
    {:else}
        {#each d.parts.left as seg, i (i)}{#if seg.ageClass}<span class={seg.ageClass}>{seg.text}</span
                >{:else}{seg.text}{/if}{/each}{#if d.parts.right !== null}
            {#each d.parts.right as seg, i (i)}{#if seg.ageClass}<span class={seg.ageClass}>{seg.text}</span
                >{:else}{seg.text}{/if}{/each}
        {/if}
    {/if}
</span>

<style>
    .date-label {
        white-space: nowrap;
        font-variant-numeric: tabular-nums;
    }
</style>
