<!--
  The stats panel beside every Acquisition trend chart: the same grid everywhere, columns 90d / 30d / 7d,
  rows average, median, weekday and weekend averages, and the change vs. the previous same-length window.
  A cell without enough data shows a dash, never a zero.
-->
<script lang="ts">
    import type { WindowStats } from '#lib/acquisition-stats.js'
    import { statWindows } from '#lib/acquisition-stats.js'

    interface Props {
        /** One `WindowStats` per window, keyed by its length in days. */
        stats: Record<(typeof statWindows)[number], WindowStats>
        formatValue: (value: number) => string
        formatChange: (change: number) => string
        /** Whether a rise is good news (visitors) or bad (the bot meter). Colors the change row. */
        upIsGood?: boolean
        /** Extra metric-specific rows under the grid, one value per window (already formatted). */
        extraRows?: { label: string; values: string[] }[]
        /** Column header suffix, for metrics whose windows count cohorts rather than days. */
        windowUnit?: string
    }

    const { stats, formatValue, formatChange, upIsGood = true, extraRows = [], windowUnit = 'd' }: Props = $props()

    const rows: { label: string; pick: (s: WindowStats) => number | null }[] = [
        { label: 'Average', pick: (s) => s.average },
        { label: 'Median', pick: (s) => s.median },
        { label: 'Weekday avg', pick: (s) => s.weekdayAverage },
        { label: 'Weekend avg', pick: (s) => s.weekendAverage },
    ]

    function changeClass(change: number | null): string {
        if (change === null || Math.abs(change) < 1e-9) return 'text-text-secondary'
        return change > 0 === upIsGood ? 'text-success' : 'text-danger'
    }
</script>

<table class="w-full text-sm">
    <thead>
        <tr class="text-xs text-text-tertiary">
            <th class="pb-1 text-left font-medium"><span class="sr-only">Stat</span></th>
            {#each statWindows as w (w)}
                <th class="pb-1 text-right font-medium">{w}{windowUnit}</th>
            {/each}
        </tr>
    </thead>
    <tbody>
        {#each rows as row (row.label)}
            <tr class="border-t border-border-subtle">
                <th scope="row" class="py-1 text-left text-xs font-normal text-text-tertiary">{row.label}</th>
                {#each statWindows as w (w)}
                    {@const value = row.pick(stats[w])}
                    <td class="py-1 text-right text-text-primary tabular-nums">{value === null ? '–' : formatValue(value)}</td>
                {/each}
            </tr>
        {/each}
        <tr class="border-t border-border-subtle">
            <th scope="row" class="py-1 text-left text-xs font-normal text-text-tertiary">vs. previous</th>
            {#each statWindows as w (w)}
                {@const change = stats[w].change}
                <td class="py-1 text-right tabular-nums {changeClass(change)}">{change === null ? '–' : formatChange(change)}</td>
            {/each}
        </tr>
        {#each extraRows as row (row.label)}
            <tr class="border-t border-border">
                <th scope="row" class="py-1 text-left text-xs font-normal text-text-tertiary">{row.label}</th>
                {#each row.values as value, i (statWindows[i])}
                    <td class="py-1 text-right text-text-secondary tabular-nums">{value}</td>
                {/each}
            </tr>
        {/each}
    </tbody>
</table>
