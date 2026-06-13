<!--
  The "By country" downloads table with a hover/focus tooltip that breaks the hovered country down by
  architecture and version as mini timelines. Used inside the Download section. Owns its own hover
  state; shares the timeline zoom window (`zoomXMin`/`zoomXMax`) with its parent so all the Download
  mini-charts zoom together.
-->
<script lang="ts">
    import type { DownloadRow } from '$lib/server/sources/cloudflare.js'
    import { aggregateBy, buildTimeline, maxDailyAcrossGroups } from '$lib/chart-helpers.js'
    import { formatNumber, formatCountry } from '$lib/format.js'
    import MiniTimeline from '$lib/components/MiniTimeline.svelte'

    let {
        downloads,
        allDays,
        allTimestamps,
        zoomXMin = null,
        zoomXMax = null,
    }: {
        downloads: DownloadRow[]
        allDays: string[]
        allTimestamps: number[]
        zoomXMin?: number | null
        zoomXMax?: number | null
    } = $props()

    let hoveredCountry: string | null = $state(null)
    let tooltipX = $state(0)
    let tooltipY = $state(0)

    const countries = $derived(aggregateBy(downloads, 'country', 'downloads').slice(0, 8))
</script>

<div class="overflow-x-auto">
    <table class="w-full text-left text-sm">
        <thead>
            <tr class="border-b border-border-subtle text-text-tertiary">
                <th class="pb-2 pr-4 font-medium">Country</th>
                <th class="pb-2 text-right font-medium">Downloads</th>
            </tr>
        </thead>
        <tbody>
            {#each countries as item}
                <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
                <tr
                    class="border-b border-border-subtle/50"
                    onmouseenter={(e: MouseEvent) => {
                        hoveredCountry = item.x
                        tooltipX = e.clientX
                        tooltipY = e.clientY
                    }}
                    onmouseleave={() => {
                        hoveredCountry = null
                    }}
                >
                    <td class="py-1.5 pr-4 text-text-primary">{formatCountry(item.x)}</td>
                    <td class="py-1.5 text-right tabular-nums text-text-secondary">{formatNumber(item.y)}</td>
                </tr>
            {/each}
        </tbody>
    </table>
</div>

{#if hoveredCountry}
    {@const countryRows = downloads.filter((r) => r.country === hoveredCountry)}
    {@const arches = aggregateBy(countryRows, 'arch', 'downloads')}
    {@const countryVersions = aggregateBy(countryRows, 'version', 'downloads').slice(0, 5)}
    {@const archMaxY = maxDailyAcrossGroups(countryRows, 'arch', arches.map((a) => a.x), allDays)}
    {@const verMaxY = maxDailyAcrossGroups(countryRows, 'version', countryVersions.map((v) => v.x), allDays)}
    <div
        class="pointer-events-none fixed z-50 w-80 rounded-lg border border-border bg-surface-elevated p-3 shadow-lg"
        style="left: {Math.max(16, tooltipX - 336)}px; top: {Math.max(16, tooltipY - 120)}px;"
    >
        <p class="mb-2 text-sm font-medium text-text-primary">{formatCountry(hoveredCountry)}</p>

        {#if arches.length > 0}
            <p class="mb-1 text-xs text-text-tertiary">By architecture</p>
            {#each arches as arch}
                {@const td = buildTimeline(countryRows.filter((r) => r.arch === arch.x), allDays, allTimestamps)}
                <div class="mb-1">
                    <div class="flex items-baseline justify-between text-xs">
                        <span class="text-text-secondary">{arch.x}</span>
                        <span class="tabular-nums text-text-tertiary">{formatNumber(arch.y)}</span>
                    </div>
                    <MiniTimeline data={td} height={48} maxY={archMaxY} xMin={zoomXMin} xMax={zoomXMax} />
                </div>
            {/each}
        {/if}

        {#if countryVersions.length > 0}
            <p class="mt-2 mb-1 text-xs text-text-tertiary">By version</p>
            {#each countryVersions as ver}
                {@const td = buildTimeline(countryRows.filter((r) => r.version === ver.x), allDays, allTimestamps)}
                <div class="mb-1">
                    <div class="flex items-baseline justify-between text-xs">
                        <span class="text-text-secondary">{ver.x}</span>
                        <span class="tabular-nums text-text-tertiary">{formatNumber(ver.y)}</span>
                    </div>
                    <MiniTimeline data={td} height={48} maxY={verMaxY} xMin={zoomXMin} xMax={zoomXMax} />
                </div>
            {/each}
        {/if}
    </div>
{/if}
