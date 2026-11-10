<!--
  One day per slot, as columns (optionally stacked) or as a rate (daily dots plus a rolling line). Plain SVG
  rather than uPlot: the data is discrete per day, and the priorities are the exact number on each column,
  a faded partial today, grey "so far" points, and an exact-values tooltip on hover and keyboard focus.
  It's a slider over the days for assistive tech: arrow keys walk them, and each day's readout is its
  `aria-valuetext`, so a screen reader announces the same numbers the tooltip shows.
-->
<script lang="ts">
    import type { TentativePoint } from '#lib/acquisition-series.js'
    import { niceTicks } from '#lib/acquisition-stats.js'
    import { columnPath, labelWidth, lineRuns, shortDay, type ColumnSeries, type TooltipRow } from '#lib/daily-chart.js'

    interface Props {
        /** UTC days, oldest first. The last may be `today` (drawn faded). */
        days: string[]
        today: string
        /** What the chart plots, for its accessible name. */
        label: string
        /** Axis and column-label format. */
        formatValue: (value: number) => string
        /** The hover/focus readout for one day. */
        tooltip: (index: number) => TooltipRow[]
        /** Column mode: one or more stacked layers, bottom first. */
        series?: ColumnSeries[]
        /** Rate mode: the daily values (dots), the rolling rate (line), and young-cohort points (grey). */
        rate?: {
            color: string
            daily: (number | null)[]
            rolling: (number | null)[]
            rollingLabel: string
            tentative?: (TentativePoint | null)[]
        }
        height?: number
    }

    const { days, today, label, formatValue, tooltip, series = [], rate, height = 180 }: Props = $props()

    const tentativeColor = '#8a8a93'
    const padL = 44
    const padR = 8
    const padT = 18
    const padB = 22

    let width = $state(0)
    let active = $state<number | null>(null)

    const plotW = $derived(Math.max(0, width - padL - padR))
    const plotH = $derived(height - padT - padB)
    const band = $derived(days.length > 0 ? plotW / days.length : 0)
    const columnW = $derived(Math.max(1, Math.min(24, band - 2)))

    const totals = $derived(
        days.map((_, i) => {
            const known = series.map((s) => s.values[i]).filter((v): v is number => v !== null)
            return known.length > 0 ? known.reduce((a, b) => a + b, 0) : null
        }),
    )

    const ticks = $derived.by(() => {
        if (rate) {
            const all = [...rate.daily, ...rate.rolling, ...(rate.tentative ?? []).map((t) => t?.value ?? null)]
            return niceTicks(Math.max(0, ...all.filter((v): v is number => v !== null)))
        }
        return niceTicks(Math.max(0, ...totals.filter((v): v is number => v !== null)), { integer: true })
    })
    const yMax = $derived(ticks.at(-1) ?? 1)
    const labelEvery = $derived(Math.max(1, Math.ceil(days.length / Math.max(1, Math.floor(plotW / 48)))))

    function y(value: number): number {
        return padT + plotH - (value / yMax) * plotH
    }
    function cx(i: number): number {
        return padL + i * band + band / 2
    }

    /** Stacked segments for column `i`, bottom up, with a 2px surface gap between neighbors. */
    function segments(i: number): { key: string; color: string; path: string }[] {
        const out: { key: string; color: string; path: string }[] = []
        let base = 0
        const known = series.filter((s) => (s.values[i] ?? 0) > 0)
        known.forEach((s, k) => {
            const value = s.values[i] ?? 0
            const top = y(base + value)
            const bottom = y(base) - (k > 0 ? 2 : 0)
            const isTop = k === known.length - 1
            const x = cx(i) - columnW / 2
            const h = Math.max(0, bottom - top)
            out.push({
                key: s.key,
                color: s.color,
                path: isTop ? columnPath(x, top, columnW, h) : `M${String(x)},${String(top)}h${String(columnW)}v${String(h)}h${String(-columnW)}Z`,
            })
            base += value
        })
        return out
    }

    function linePath(values: (number | null)[]): string {
        return lineRuns(values)
            .map((run) => run.map((p, k) => `${k === 0 ? 'M' : 'L'}${String(cx(p.index))},${String(y(p.value))}`).join(''))
            .join('')
    }

    const activeRows = $derived(active === null ? [] : tooltip(active))
    const announcement = $derived(
        active === null
            ? ''
            : `${days[active]}${days[active] === today ? ' (today, partial)' : ''}: ${activeRows.map((r) => `${r.label} ${r.value}`).join(', ')}`,
    )

    function onKeydown(e: KeyboardEvent) {
        if (days.length === 0) return
        const last = days.length - 1
        const current = active ?? last
        const next =
            e.key === 'ArrowLeft' ? Math.max(0, current - 1)
            : e.key === 'ArrowRight' ? Math.min(last, current + 1)
            : e.key === 'Home' ? 0
            : e.key === 'End' ? last
            : null
        if (next === null) return
        e.preventDefault()
        active = next
    }

    const tooltipOnLeft = $derived(active !== null && cx(active) > width * 0.6)
</script>

<div
    class="relative w-full rounded-md focus-visible:ring-2 focus-visible:ring-accent focus-visible:outline-none"
    bind:clientWidth={width}
    tabindex="0"
    role="slider"
    aria-label="{label}. Use the arrow keys to read each day."
    aria-valuemin={0}
    aria-valuemax={Math.max(0, days.length - 1)}
    aria-valuenow={active ?? Math.max(0, days.length - 1)}
    aria-valuetext={announcement || `${String(days.length)} days`}
    onkeydown={onKeydown}
    onfocus={() => (active ??= days.length - 1)}
    onblur={() => (active = null)}
    onpointerleave={() => (active = null)}
>
    {#if width > 0}
        <svg {width} {height} class="block" aria-hidden="true">
            <!-- Recessive hairline grid and y labels -->
            {#each ticks as tick (tick)}
                <line x1={padL} x2={width - padR} y1={y(tick)} y2={y(tick)} stroke="#2e2d2a" stroke-width="1" />
                <text x={padL - 6} y={y(tick) + 3} text-anchor="end" class="fill-text-tertiary text-[10px] tabular-nums">
                    {formatValue(tick)}
                </text>
            {/each}

            <!-- X labels; one that would crowd the "today" label at the end is skipped -->
            {#each days as day, i (day)}
                {#if day === today || (i % labelEvery === 0 && !(days.at(-1) === today && days.length - 1 - i < labelEvery))}
                    <text x={cx(i)} y={height - 6} text-anchor="middle" class="fill-text-tertiary text-[10px]">
                        {day === today ? 'today' : shortDay(day)}
                    </text>
                {/if}
            {/each}

            <!-- Crosshair (rate) or column lift (columns) for the active day -->
            {#if active !== null}
                {#if rate}
                    <line x1={cx(active)} x2={cx(active)} y1={padT} y2={padT + plotH} stroke="#6b6b73" stroke-width="1" />
                {:else}
                    <rect x={padL + active * band} y={padT} width={band} height={plotH} fill="#fafafa" opacity="0.05" />
                {/if}
            {/if}

            {#if rate}
                <path d={linePath(rate.rolling)} fill="none" stroke={rate.color} stroke-width="2" stroke-linejoin="round" stroke-linecap="round" />
                {#each rate.daily as value, i (days[i])}
                    {#if value !== null}
                        <circle
                            cx={cx(i)}
                            cy={y(value)}
                            r="4"
                            fill={rate.color}
                            fill-opacity={days[i] === today ? 0.3 : 0.6}
                            stroke="#1a1917"
                            stroke-width="2"
                        />
                    {/if}
                {/each}
                {#each rate.tentative ?? [] as point, i (days[i])}
                    {#if point}
                        <circle cx={cx(i)} cy={y(point.value)} r="4" fill={tentativeColor} fill-opacity={days[i] === today ? 0.4 : 0.9} stroke="#1a1917" stroke-width="2" />
                    {/if}
                {/each}
                <!-- Label the newest young-cohort point; the tooltip names the rest. -->
                {@const lastTentative = (rate.tentative ?? []).findLastIndex((t) => t !== null)}
                {#if lastTentative >= 0}
                    {@const point = rate.tentative?.[lastTentative]}
                    {#if point}
                        <text x={cx(lastTentative)} y={y(point.value) - 8} text-anchor="end" class="fill-text-tertiary text-[10px]">{point.label}</text>
                    {/if}
                {/if}
            {:else}
                {#each days as day, i (day)}
                    <g opacity={day === today ? 0.35 : 1}>
                        {#each segments(i) as segment (segment.key)}
                            <path d={segment.path} fill={segment.color} />
                        {/each}
                    </g>
                    {@const total = totals[i]}
                    {#if total !== null && total > 0 && labelWidth(formatValue(total)) <= band}
                        <text
                            x={cx(i)}
                            y={y(total) - 4}
                            text-anchor="middle"
                            class="fill-text-secondary text-[10px] tabular-nums"
                            opacity={day === today ? 0.5 : 1}
                        >
                            {formatValue(total)}
                        </text>
                    {/if}
                {/each}
            {/if}

            <!-- Hit targets: the whole day slot, bigger than any mark -->
            {#each days as day, i (day)}
                <rect
                    x={padL + i * band}
                    y={padT}
                    width={band}
                    height={plotH}
                    fill="transparent"
                    role="presentation"
                    onpointerenter={() => (active = i)}
                />
            {/each}
        </svg>
    {/if}

    {#if active !== null}
        <div
            class="pointer-events-none absolute top-0 z-20 min-w-40 rounded-lg border border-border bg-surface-elevated p-2.5 text-xs shadow-lg"
            style="{tooltipOnLeft ? 'right' : 'left'}: {tooltipOnLeft ? width - cx(active) + 10 : cx(active) + 10}px;"
            aria-hidden="true"
        >
            <p class="mb-1 font-medium text-text-primary">
                {days[active]}{#if days[active] === today}<span class="ml-1 font-normal text-text-tertiary">(today, partial)</span>{/if}
            </p>
            {#each activeRows as row (row.label)}
                <div class="flex items-center justify-between gap-4">
                    <span class="flex items-center gap-1.5 text-text-tertiary">
                        {#if row.color}
                            {#if row.mark === 'line'}
                                <span class="inline-block h-0.5 w-3 rounded-sm" style="background-color: {row.color};"></span>
                            {:else if row.mark === 'dot'}
                                <span class="inline-block size-2 rounded-full" style="background-color: {row.color};"></span>
                            {:else}
                                <span class="inline-block size-2 rounded-sm" style="background-color: {row.color};"></span>
                            {/if}
                        {/if}
                        {row.label}
                    </span>
                    <span class="font-semibold text-text-primary tabular-nums">{row.value}</span>
                </div>
            {/each}
        </div>
    {/if}

    <!-- Legend: always for 2+ series, and for the rate's dot/line/grey encodings -->
    {#if series.length > 1 || rate}
        <div class="mt-1 flex flex-wrap gap-x-3 gap-y-1 pl-11 text-xs text-text-secondary">
            {#if rate}
                <span class="flex items-center gap-1.5">
                    <span class="inline-block size-2 rounded-full opacity-60" style="background-color: {rate.color};"></span>Daily
                </span>
                <span class="flex items-center gap-1.5">
                    <span class="inline-block h-0.5 w-3 rounded-sm" style="background-color: {rate.color};"></span>{rate.rollingLabel}
                </span>
                {#if rate.tentative?.some((t) => t !== null)}
                    <span class="flex items-center gap-1.5">
                        <span class="inline-block size-2 rounded-full" style="background-color: {tentativeColor};"></span>Too young for D7, retained so far
                    </span>
                {/if}
            {:else}
                {#each series as s (s.key)}
                    <span class="flex items-center gap-1.5">
                        <span class="inline-block size-2.5 rounded-sm" style="background-color: {s.color};"></span>{s.label}
                    </span>
                {/each}
            {/if}
        </div>
    {/if}
</div>
