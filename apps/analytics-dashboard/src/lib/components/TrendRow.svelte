<!--
  One metric on the Acquisition page: the chart on the left, the stats panel on the right (stacked on
  narrow screens), titled with an (i) that explains the metric. `unavailable` swaps both for a short
  "Not available yet" line, so a missing field never renders as an empty chart or fake zeros.
-->
<script lang="ts">
    import type { Snippet } from 'svelte'
    import InfoPopover from './InfoPopover.svelte'

    interface Props {
        title: string
        info: Snippet
        chart: Snippet
        stats: Snippet
        /** When set, the reason this metric can't be drawn yet, shown in place of chart and stats. */
        unavailable?: string | null
    }

    const { title, info, chart, stats, unavailable = null }: Props = $props()
</script>

<div class="grid grid-cols-1 gap-x-6 gap-y-3 border-t border-border-subtle py-5 first:border-t-0 lg:grid-cols-[minmax(0,1fr)_19rem]">
    <h3 class="flex items-center gap-2 text-sm font-medium text-text-primary lg:col-span-2">
        {title}
        <InfoPopover topic={title}>{@render info()}</InfoPopover>
    </h3>
    {#if unavailable}
        <p class="text-sm text-text-tertiary lg:col-span-2">{unavailable}</p>
    {:else}
        <div class="min-w-0">{@render chart()}</div>
        <div>{@render stats()}</div>
    {/if}
</div>
