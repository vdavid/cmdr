<script lang="ts">
    /**
     * Discrete stacked bar chart: one bar per day, segmented by series, with an exact-numbers tooltip
     * on hover/focus. Built from plain elements (not uPlot) because the data is discrete per-day and
     * the priority is reading precise per-segment counts, not a continuous zoomable timeline.
     */

    interface Series {
        key: string
        label: string
        /** CSS color for the segment fill. */
        color: string
        /** One value per day, aligned to the `days` array. */
        values: number[]
    }

    interface Props {
        /** ISO day strings (YYYY-MM-DD), oldest first. */
        days: string[]
        series: Series[]
        /** Noun for the tooltip total, for example "downloads" or "updates". */
        unitLabel: string
        height?: number
    }

    let { days, series, unitLabel, height = 160 }: Props = $props()

    let hoveredDay = $state<number | null>(null)

    const dayTotals = $derived(days.map((_, dayIdx) => series.reduce((sum, s) => sum + (s.values[dayIdx] ?? 0), 0)))
    const maxTotal = $derived(Math.max(1, ...dayTotals))

    /** Show at most ~7 x-axis labels so they don't overlap on a 30-day range. */
    const labelEvery = $derived(Math.max(1, Math.ceil(days.length / 7)))

    function shortDay(day: string): string {
        return day.slice(5) // MM-DD
    }
</script>

<div class="w-full">
    <div class="flex items-end gap-px" style="height: {height}px;" role="group" aria-label="Daily {unitLabel} by category">
        {#each days as day, dayIdx}
            {@const total = dayTotals[dayIdx]}
            <button
                type="button"
                class="group relative flex h-full flex-1 cursor-default flex-col-reverse rounded-sm focus:outline-none focus-visible:ring-2 focus-visible:ring-accent"
                onmouseenter={() => (hoveredDay = dayIdx)}
                onmouseleave={() => (hoveredDay = null)}
                onfocus={() => (hoveredDay = dayIdx)}
                onblur={() => (hoveredDay = null)}
                aria-label="{day}: {total} {unitLabel}"
            >
                <!-- Hover/focus highlight behind the bar -->
                <span
                    class="pointer-events-none absolute inset-0 rounded-sm bg-text-primary/5 opacity-0 transition-opacity {hoveredDay ===
                    dayIdx
                        ? 'opacity-100'
                        : ''}"
                ></span>
                {#each series as s}
                    {@const value = s.values[dayIdx] ?? 0}
                    {#if value > 0}
                        <span
                            class="block w-full"
                            style="height: {(value / maxTotal) * 100}%; background-color: {s.color};"
                        ></span>
                    {/if}
                {/each}
            </button>
        {/each}
    </div>

    <!-- X-axis labels -->
    <div class="mt-1 flex gap-px">
        {#each days as day, dayIdx}
            <div class="flex-1 text-center text-[9px] tabular-nums text-text-tertiary">
                {dayIdx % labelEvery === 0 ? shortDay(day) : ''}
            </div>
        {/each}
    </div>

    <!-- Legend -->
    <div class="mt-2 flex flex-wrap gap-x-3 gap-y-1">
        {#each series as s}
            <div class="flex items-center gap-1.5 text-xs text-text-secondary">
                <span class="inline-block h-2.5 w-2.5 rounded-sm" style="background-color: {s.color};"></span>
                {s.label}
            </div>
        {/each}
    </div>

    <!-- Tooltip: exact per-segment numbers for the hovered/focused day -->
    {#if hoveredDay != null}
        {@const dayIdx = hoveredDay}
        <div class="mt-3 inline-block rounded-lg border border-border bg-surface-elevated p-3 text-sm">
            <p class="mb-1 font-medium text-text-primary">
                {days[dayIdx]}
                <span class="ml-2 tabular-nums text-text-secondary">{dayTotals[dayIdx]} {unitLabel}</span>
            </p>
            <div class="flex flex-col gap-0.5">
                {#each series as s}
                    {@const value = s.values[dayIdx] ?? 0}
                    <div class="flex items-center justify-between gap-4 text-xs">
                        <span class="flex items-center gap-1.5 text-text-secondary">
                            <span class="inline-block h-2.5 w-2.5 rounded-sm" style="background-color: {s.color};"></span>
                            {s.label}
                        </span>
                        <span class="tabular-nums text-text-primary">{value}</span>
                    </div>
                {/each}
            </div>
        </div>
    {/if}
</div>
