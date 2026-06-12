<script lang="ts" module>
    import type { DownloadRow, UpdateActivityRow } from '$lib/server/sources/cloudflare.js'
    import { aggregateChannels } from '$lib/funnel.js'

    interface StackSeries {
        key: string
        label: string
        color: string
        values: number[]
    }

    // Download source colors: website is the product gold, Homebrew an amber, everything else grey.
    const SOURCE_STACK = [
        { key: 'website', label: 'Website', color: '#ffc206' },
        { key: 'homebrew', label: 'Homebrew', color: '#f0883e' },
        { key: 'other', label: 'Direct / other', color: '#71717a' },
    ]
    // Newest release gets the brightest color; the rest cycle, with anything older bucketed as grey.
    const VERSION_PALETTE = ['#ffc206', '#a78bfa', '#22d3ee', '#8faa3b', '#f0883e', '#f472b6']
    const COLOR_OLDER = '#71717a'

    /** Sorted unique day strings (YYYY-MM-DD, ascending) from a list of rows carrying a `day` field. */
    function uniqueDays(rows: Array<{ day: string }>): string[] {
        return [...new Set(rows.map((r) => r.day))].sort()
    }

    /** Aligns rows into a per-key map of per-day value arrays (one slot per entry in `days`). */
    function stackByDay<T>(
        rows: T[],
        days: string[],
        getDay: (r: T) => string,
        getKey: (r: T) => string,
        getValue: (r: T) => number
    ): Map<string, number[]> {
        const dayIndex = new Map(days.map((d, i) => [d, i]))
        const byKey = new Map<string, number[]>()
        for (const row of rows) {
            const di = dayIndex.get(getDay(row))
            if (di === undefined) continue
            const key = getKey(row)
            let arr = byKey.get(key)
            if (!arr) {
                arr = new Array(days.length).fill(0)
                byKey.set(key, arr)
            }
            arr[di] += getValue(row)
        }
        return byKey
    }

    /** Downloads stacked by source, using the deduped same-day-distinct count. */
    function downloadSourceSeries(rows: DownloadRow[], days: string[]): StackSeries[] {
        const byKey = stackByDay(
            rows,
            days,
            (r) => r.day,
            (r) => r.source,
            (r) => r.uniqueDownloads
        )
        return SOURCE_STACK.map((s) => ({ ...s, values: byKey.get(s.key) ?? new Array(days.length).fill(0) })).filter(
            (s) => s.values.some((v) => v > 0)
        )
    }

    /** Update activity stacked by the version each install was running when it checked. */
    function updateVersionSeries(rows: UpdateActivityRow[], days: string[]): StackSeries[] {
        const byKey = stackByDay(
            rows,
            days,
            (r) => r.day,
            (r) => r.version,
            (r) => r.updaters
        )
        const versions = [...byKey.keys()].sort(compareSemverDesc)
        const top = versions.slice(0, VERSION_PALETTE.length)
        const rest = versions.slice(VERSION_PALETTE.length)
        const series: StackSeries[] = top.map((v, i) => ({
            key: v,
            label: `v${v}`,
            color: VERSION_PALETTE[i],
            values: byKey.get(v) ?? new Array(days.length).fill(0),
        }))
        if (rest.length > 0) {
            const olderValues = new Array(days.length).fill(0)
            for (const v of rest) {
                const arr = byKey.get(v) ?? []
                for (let i = 0; i < days.length; i++) olderValues[i] += arr[i] ?? 0
            }
            series.push({ key: 'older', label: 'Older', color: COLOR_OLDER, values: olderValues })
        }
        return series
    }

    /** Aggregates rows by a string field, summing a numeric field. */
    function aggregateBy(
        rows: DownloadRow[],
        groupField: keyof DownloadRow,
        sumField: keyof DownloadRow
    ): Array<{ x: string; y: number }> {
        const map = new Map<string, number>()
        for (const row of rows) {
            const key = String(row[groupField])
            map.set(key, (map.get(key) ?? 0) + Number(row[sumField]))
        }
        return [...map.entries()]
            .map(([x, y]) => ({ x, y }))
            .sort((a, b) => b.y - a.y)
    }

    /** Returns sorted unique day strings and their unix timestamps from download rows. */
    function getDayAxis(rows: DownloadRow[]): { days: string[]; timestamps: number[] } {
        const days = [...new Set(rows.map((r) => r.day))].sort()
        const timestamps = days.map((d) => new Date(d).getTime() / 1000)
        return { days, timestamps }
    }

    /** Builds uPlot [timestamps[], values[]] for a filtered subset, aligned to the full day axis. */
    function buildTimeline(
        rows: DownloadRow[],
        allDays: string[],
        allTimestamps: number[]
    ): [number[], number[]] {
        const byDay = new Map<string, number>()
        for (const row of rows) {
            byDay.set(row.day, (byDay.get(row.day) ?? 0) + row.downloads)
        }
        return [allTimestamps, allDays.map((d) => byDay.get(d) ?? 0)]
    }

    /** Compares two semver strings, descending (higher version first). */
    function compareSemverDesc(a: string, b: string): number {
        const pa = a.split('.').map(Number)
        const pb = b.split('.').map(Number)
        for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
            const diff = (pb[i] ?? 0) - (pa[i] ?? 0)
            if (diff !== 0) return diff
        }
        return 0
    }

    /** Finds the max daily download value across a set of groups. */
    function maxDailyAcrossGroups(
        rows: DownloadRow[],
        groupField: keyof DownloadRow,
        groupKeys: string[],
        allDays: string[]
    ): number {
        let max = 1
        for (const key of groupKeys) {
            const byDay = new Map<string, number>()
            for (const row of rows) {
                if (String(row[groupField]) === key) {
                    byDay.set(row.day, (byDay.get(row.day) ?? 0) + row.downloads)
                }
            }
            for (const v of byDay.values()) {
                if (v > max) max = v
            }
        }
        return max
    }
</script>

<script lang="ts">
    import Chart from '$lib/components/Chart.svelte'
    import MiniTimeline from '$lib/components/MiniTimeline.svelte'
    import PieChart from '$lib/components/PieChart.svelte'
    import StackedBarChart from '$lib/components/StackedBarChart.svelte'
    import {
        countFeedbackWithReplyTo,
        tallyErrorReportsByField,
        errorReportsByDay,
    } from '$lib/feedback-and-errors.js'

    let { data } = $props()

    const ranges = ['today', '24h', '7d', '30d'] as const
    const downloadSyncKey = 'dl-timelines'

    /** True when a single specific UTC day is selected (vs a relative range). */
    const isDaySelected = $derived(data.selection.range === 'day')
    /** The selected specific day, or '' for the date input when a relative range is active. */
    const selectedDay = $derived(data.selection.day ?? '')

    /** Navigate to a relative range, clearing any specific-day selection. */
    function selectRange(range: string) {
        zoomXMin = null
        zoomXMax = null
        if (!(data.selection.range === range && !data.selection.day)) {
            window.location.href = `?range=${range}`
        }
    }

    /** Navigate to a single specific UTC day (or back to the default range when cleared). */
    function selectDay(day: string) {
        zoomXMin = null
        zoomXMax = null
        window.location.href = day ? `?day=${day}` : '?range=7d'
    }

    /** Today's UTC day as YYYY-MM-DD, the max selectable day (no future days). */
    const todayIso = new Date().toISOString().slice(0, 10)

    /** A short human label for the current selection, for the funnel-vs-sections note. */
    const selectionLabel = $derived(
        data.selection.range === 'day' ? (data.selection.day ?? 'a day') : data.selection.range,
    )

    /** Format a funnel cell: a real number, or an en dash when the value is unknown (null). */
    function funnelCell(value: number | null): string {
        return value === null ? '–' : formatNumber(value)
    }

    /** Format a D7 retention fraction as a percent, or an en dash when unknown (null / young cohort). */
    function funnelPercent(fraction: number | null): string {
        return fraction === null ? '–' : `${Math.round(fraction * 100)}%`
    }

    // Color palette
    const COLOR_GOLD = '#ffc206'
    const COLOR_PURPLE = '#a78bfa'
    const COLOR_GREEN = '#8faa3b' // autumn-y green for veszelovszki.com
    const COLOR_CYAN = '#22d3ee' // cyan for getprvw.com

    /** Time range in seconds for the selected range. Used as default zoom for star charts. */
    const rangeSeconds: Record<string, number> = { today: 86400, '24h': 86400, '7d': 7 * 86400, '30d': 30 * 86400 }
    function starChartXMin(): number {
        return Date.now() / 1000 - (rangeSeconds[data.selection.range] ?? 7 * 86400)
    }

    const regionNames = new Intl.DisplayNames(['en'], { type: 'region' })
    function formatCountry(code: string): string {
        try {
            const upper = code.toUpperCase()
            const name = regionNames.of(upper)
            return name && name !== upper ? `${name} (${upper})` : code
        } catch {
            return code
        }
    }

    let zoomXMin: number | null = $state(null)
    let zoomXMax: number | null = $state(null)
    let hoveredCountry: string | null = $state(null)
    let tooltipX = $state(0)
    let tooltipY = $state(0)

    // Data-point hover state
    let hoveredDayIdx: number | null = $state(null)
    let dayTooltipVisible = $state(false)
    let dayTooltipX = $state(0)
    let dayTooltipY = $state(0)

    function handleDayHover(idx: number | null) {
        hoveredDayIdx = idx
        dayTooltipVisible = idx != null
    }

    function handleDownloadWheel(e: WheelEvent, dataXMin: number, dataXMax: number) {
        e.preventDefault()
        const zoomFactor = e.deltaY > 0 ? 1.3 : 1 / 1.3
        const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
        const fraction = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width))

        const curMin = zoomXMin ?? dataXMin
        const curMax = zoomXMax ?? dataXMax
        const range = curMax - curMin
        const center = curMin + range * fraction
        const newRange = Math.min(range * zoomFactor, dataXMax - dataXMin)

        if (newRange >= dataXMax - dataXMin) {
            zoomXMin = null
            zoomXMax = null
        } else {
            zoomXMin = Math.max(dataXMin, center - newRange * fraction)
            zoomXMax = Math.min(dataXMax, zoomXMin + newRange)
        }
    }

    function formatNumber(n: number): string {
        return n.toLocaleString('en-US')
    }

    function formatCurrency(cents: string | number, currency = 'USD'): string {
        const value = Number(cents) / 100
        return new Intl.NumberFormat('en-US', { style: 'currency', currency }).format(value)
    }

    function formatDelta(current: number, previous: number): { text: string; positive: boolean } {
        if (previous === 0) return { text: 'N/A', positive: true }
        const pct = ((current - previous) / previous) * 100
        const sign = pct >= 0 ? '+' : ''
        return { text: `${sign}${pct.toFixed(1)}%`, positive: pct >= 0 }
    }

    function formatTime(iso: string): string {
        return new Date(iso).toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit' })
    }

    /** Converts daily rows ({day, views/count}) into uPlot's AlignedData format [timestamps[], values[]]. */
    function toChartData(rows: Array<{ day: string; views?: number; count?: number }>): [number[], number[]] {
        const timestamps = rows.map((r) => new Date(r.day).getTime() / 1000)
        const values = rows.map((r) => r.views ?? r.count ?? 0)
        return [timestamps, values]
    }
</script>

<div class="mx-auto max-w-[1800px] px-6 pb-8 pt-14">
    <!-- Header (sticky) -->
    <header class="fixed inset-x-0 top-0 z-40 flex items-center justify-between gap-4 border-b border-border bg-surface/90 px-6 py-2 backdrop-blur-sm">
        <h1 class="text-lg font-bold text-text-primary">Cmdr analytics</h1>

        <div class="flex items-center gap-3">
            <div class="flex rounded-lg border border-border bg-surface p-0.5">
                {#each ranges as r}
                    <button
                        onclick={() => selectRange(r)}
                        class="rounded-md px-3 py-1 text-sm font-medium transition-colors
                            {!isDaySelected && data.selection.range === r
                            ? 'bg-accent text-accent-contrast'
                            : 'text-text-secondary hover:text-text-primary'}"
                    >
                        {r}
                    </button>
                {/each}
            </div>
            <input
                type="date"
                max={todayIso}
                value={selectedDay}
                onchange={(e) => selectDay((e.currentTarget as HTMLInputElement).value)}
                aria-label="View a specific UTC day"
                class="rounded-lg border px-2 py-1 text-sm transition-colors
                    {isDaySelected
                    ? 'border-accent bg-accent/10 text-text-primary'
                    : 'border-border bg-surface text-text-secondary hover:text-text-primary'}"
            />
            <span class="text-xs text-text-tertiary">
                Updated {formatTime(data.updatedAt)}
            </span>
        </div>
    </header>

    <!-- Daily funnel (full width, always the last 30 UTC days, independent of the range picker) -->
    <section class="mb-6 rounded-xl border border-border bg-surface p-6">
        <div class="mb-1">
            <h2 class="text-lg font-semibold text-text-primary">Daily funnel</h2>
            <p class="text-sm text-text-tertiary">The last 30 days, one row per UTC day, newest first.</p>
        </div>
        {@render sectionDescription(
            "Use this to watch the whole acquisition path day by day: site visitors, download clicks, real server " +
                "downloads, new installs, week-one retention, signups, and purchases, all lined up so you can spot where " +
                "a day fell off.",
            "All days are UTC, and today's row is partial (it's still going). A dash means we couldn't get that cell " +
                "(not a zero). Click a day to filter the sections below to it.",
        )}
        {#if !data.funnel.ok}
            {@render errorState(data.funnel.error)}
        {:else}
            {@const rows = data.funnel.data.rows}
            <div class="overflow-x-auto">
                <table class="w-full text-sm">
                    <thead>
                        <tr class="border-b border-border text-left text-xs text-text-tertiary">
                            <th class="py-2 pr-4 font-medium">Day</th>
                            <th class="py-2 pr-4 text-right font-medium">Visitors</th>
                            <th class="py-2 pr-4 text-right font-medium">Download clicks</th>
                            <th class="py-2 pr-4 text-right font-medium">Server downloads</th>
                            <th class="py-2 pr-4 text-right font-medium">New installs</th>
                            <th class="py-2 pr-4 text-right font-medium">D7 retained</th>
                            <th class="py-2 pr-4 text-right font-medium">Signups</th>
                            <th class="py-2 text-right font-medium">Purchases</th>
                        </tr>
                    </thead>
                    <tbody>
                        {#each [...rows].reverse() as row (row.date)}
                            {@const isToday = row.date === todayIso}
                            {@const isActiveDay = isDaySelected && row.date === selectedDay}
                            <tr
                                class="cursor-pointer border-b border-border-subtle transition-colors hover:bg-surface-elevated
                                    {isActiveDay ? 'bg-accent/10' : ''}"
                                onclick={() => selectDay(row.date)}
                            >
                                <td class="py-1.5 pr-4 font-medium text-text-primary">
                                    {row.date}{#if isToday}<span class="ml-1 text-xs font-normal text-text-tertiary">(today, partial)</span>{/if}
                                </td>
                                <td class="py-1.5 pr-4 text-right tabular-nums text-text-secondary">{funnelCell(row.visitors)}</td>
                                <td class="py-1.5 pr-4 text-right tabular-nums text-text-secondary">{funnelCell(row.downloadClicks)}</td>
                                <td class="py-1.5 pr-4 text-right tabular-nums text-text-secondary">{funnelCell(row.serverDownloads)}</td>
                                <td class="py-1.5 pr-4 text-right tabular-nums text-text-secondary">{funnelCell(row.newInstalls)}</td>
                                <td class="py-1.5 pr-4 text-right tabular-nums text-text-secondary">
                                    {funnelPercent(row.d7Retention)}{#if row.d7Retained !== null}<span class="ml-1 text-xs text-text-tertiary">({row.d7Retained})</span>{/if}
                                </td>
                                <td class="py-1.5 pr-4 text-right tabular-nums text-text-secondary">{funnelCell(row.newsletterSignups)}</td>
                                <td class="py-1.5 text-right tabular-nums text-text-secondary">{funnelCell(row.purchases)}</td>
                            </tr>
                        {/each}
                    </tbody>
                </table>
            </div>
            {@render methodology(
                "Visitors and download clicks come from Umami (cookieless, in-browser). Server downloads, new installs, " +
                    "DAU, and D7 come from the app's own telemetry (D1); signups from Listmonk; purchases from Paddle. " +
                    "Clicks and server downloads won't match: server downloads also include Homebrew, direct links, and " +
                    "GitHub-page traffic, and bot user agents are filtered but imperfectly. D7 needs a cohort at least 8 " +
                    "days old, so recent rows show a dash there.",
            )}

            <!-- Channels: server downloads rolled up by first-touch ref over the whole 30-day window. -->
            {@const channels = aggregateChannels(rows)}
            <div class="mt-6 border-t border-border-subtle pt-4">
                <h3 class="mb-1 text-sm font-medium text-text-secondary">Channels (last 30 days)</h3>
                {@render sectionDescription(
                    "Use this to see which channels drove downloads: a download's ref is the channel the visitor first " +
                        "arrived from (a UTM source or campaign, else the referring site).",
                    "Ref is first-touch per browser visit and comes from the URL only, so return visits and cross-device " +
                        "journeys (read on the phone, download on the Mac) carry no ref and land in \"(none)\". Homebrew and " +
                        "direct links have none too, and rows before 2026-06-12 predate the column. So treat \"(none)\" as " +
                        "\"channel unknown\", not \"direct\". All days UTC.",
                )}
                {#if channels.length === 0}
                    <p class="text-sm text-text-tertiary">No downloads with a channel yet.</p>
                {:else}
                    {@render metricTable(
                        channels.map((c) => ({ x: c.ref === '(none)' ? '(none / unknown)' : c.ref, y: c.count })),
                        'Channel',
                        'Downloads',
                    )}
                {/if}
            </div>
        {/if}
    </section>

    <!-- Sections -->
    <div class="grid grid-cols-1 gap-6 md:grid-cols-2 xl:grid-cols-3">
        <!-- 1. Awareness -->
        <section class="rounded-xl border border-border bg-surface p-6">
            <div class="mb-1">
                <h2 class="text-lg font-semibold text-text-primary">Awareness</h2>
                <p class="text-sm text-text-tertiary">How many people see Cmdr content?</p>
            </div>
            {@render sectionDescription(
                "Use this to gauge top-of-funnel reach across the three sites, and which days or sites are growing.",
                "Umami is cookieless and proxied, so it dodges most adblockers, but it still undercounts by a few percent, " +
                    "and one person on two devices counts twice.",
            )}

            {#if !data.umami.ok}
                {@render errorState(data.umami.error)}
            {:else}
                {@const umami = data.umami.data}
                {@const totalPageviews = umami.personalSite.pageviews.value + umami.website.pageviews.value + umami.prvw.pageviews.value}
                {@const prevPageviews = umami.personalSite.pageviews.prev + umami.website.pageviews.prev + umami.prvw.pageviews.prev}
                {@const delta = formatDelta(totalPageviews, prevPageviews)}

                {@render metricRow([
                    { label: 'Total page views', value: formatNumber(totalPageviews), delta },
                    { label: 'veszelovszki.com views', value: formatNumber(umami.personalSite.pageviews.value), color: COLOR_GREEN },
                    { label: 'getcmdr.com views', value: formatNumber(umami.website.pageviews.value), color: COLOR_GOLD },
                    { label: 'getprvw.com views', value: formatNumber(umami.prvw.pageviews.value), color: COLOR_CYAN },
                ])}

                {#if umami.websiteReferrers.length > 0}
                    <div class="mt-4">
                        <h3 class="mb-2 text-sm font-medium text-text-secondary">Top referrers (getcmdr.com)</h3>
                        {@render metricTable(umami.websiteReferrers.slice(0, 10), 'Source', 'Views')}
                    </div>
                {/if}

                {#if umami.prvwReferrers.length > 0}
                    <div class="mt-4">
                        <h3 class="mb-2 text-sm font-medium text-text-secondary">Top referrers (getprvw.com)</h3>
                        {@render metricTable(umami.prvwReferrers.slice(0, 10), 'Source', 'Views')}
                    </div>
                {/if}

                {#if data.githubStars.ok}
                    {@const stars = data.githubStars.data}
                    {@const repoColors: Record<string, string> = { 'vdavid/cmdr': COLOR_GOLD, 'vdavid/mtp-rs': COLOR_PURPLE }}
                    <div class="mt-4">
                        <h3 class="mb-2 text-sm font-medium text-text-secondary">GitHub stars</h3>
                        {@render metricRow(
                            stars.repos.map((r) => ({ label: r.repo, value: formatNumber(r.totalStars), color: repoColors[r.repo] }))
                        )}
                        {#each stars.repos as repo}
                            {@const c = repoColors[repo.repo] ?? COLOR_GOLD}
                            {#if repo.daily.length > 1}
                                <div class="mt-2">
                                    <Chart
                                        data={[
                                            repo.daily.map((d) => new Date(d.day).getTime() / 1000),
                                            repo.daily.map((d) => d.cumulative),
                                        ]}
                                        labels={[repo.repo]}
                                        colors={[c]}
                                        height={120}
                                        xMin={starChartXMin()}
                                        xMax={Date.now() / 1000}
                                    />
                                </div>
                            {/if}
                        {/each}
                    </div>
                {/if}

                {@render externalLinks([
                    { label: 'View veszelovszki.com in Umami', href: 'https://anal.veszelovszki.com' },
                    { label: 'View getcmdr.com in Umami', href: 'https://anal.veszelovszki.com' },
                    { label: 'View getprvw.com in Umami', href: 'https://anal.veszelovszki.com' },
                    { label: 'cmdr on GitHub', href: 'https://github.com/vdavid/cmdr' },
                    { label: 'mtp-rs on GitHub', href: 'https://github.com/vdavid/mtp-rs' },
                ])}
            {/if}
        </section>

        <!-- 2. Interest -->
        <section class="rounded-xl border border-border bg-surface p-6">
            <div class="mb-1">
                <h2 class="text-lg font-semibold text-text-primary">Interest</h2>
                <p class="text-sm text-text-tertiary">How many engage with the product page?</p>
            </div>
            {@render sectionDescription(
                "Use this to see getcmdr.com engagement over time, and to cross-check the two trackers against each other.",
                "Umami is cookieless and undercounts a bit; PostHog needs its own client-side script to load, so adblockers " +
                    "trim it more. Treat the two as independent estimates, not one exact number.",
            )}

            {#if !data.umami.ok && !data.posthog.ok}
                {@render errorState(
                    [!data.umami.ok ? data.umami.error : '', !data.posthog.ok ? data.posthog.error : '']
                        .filter(Boolean)
                        .join('; ')
                )}
            {:else}
                <div class="space-y-4">
                    {#if data.umami.ok}
                        {@const umami = data.umami.data}
                        {@const delta = formatDelta(umami.website.pageviews.value, umami.website.pageviews.prev)}

                        {@render metricRow([
                            { label: 'getcmdr.com page views', value: formatNumber(umami.website.pageviews.value), delta },
                            { label: 'Unique visitors', value: formatNumber(umami.website.visitors.value) },
                        ])}

                        {#if data.posthog.ok && data.posthog.data.dailyPageviews.length > 0}
                            <Chart data={toChartData(data.posthog.data.dailyPageviews)} labels={['Page views']} height={180} />
                        {/if}

                        {#if umami.downloadEvents.length > 0}
                            <div>
                                <h3 class="mb-2 text-sm font-medium text-text-secondary">Download button clicks</h3>
                                {@render metricTable(umami.downloadEvents.slice(0, 10), 'Event', 'Count')}
                            </div>
                        {/if}

                        {#if umami.websitePages.length > 0}
                            <div>
                                <h3 class="mb-2 text-sm font-medium text-text-secondary">Top pages</h3>
                                {@render metricTable(umami.websitePages.slice(0, 10), 'Page', 'Views')}
                            </div>
                        {/if}
                    {:else if !data.umami.ok}
                        {@render errorState(data.umami.error)}
                    {/if}
                </div>

                {@render externalLinks([
                    { label: 'View in Umami', href: 'https://anal.veszelovszki.com' },
                    { label: 'View in PostHog', href: 'https://eu.posthog.com/project/136072' },
                ])}
            {/if}
        </section>

        <!-- 3. Download -->
        <section class="rounded-xl border border-border bg-surface p-6">
            <div class="mb-1">
                <h2 class="text-lg font-semibold text-text-primary">Download</h2>
                <p class="text-sm text-text-tertiary">How many actually download?</p>
            </div>
            {@render sectionDescription(
                "Use this to see real new installs by source (website, Homebrew, other) and which release and platform " +
                    "people are grabbing.",
                "These are server downloads from the app's own endpoint, deduped per day by a daily-rotating hashed IP, " +
                    "with bot user agents dropped (imperfectly). It won't match the Umami download clicks above, which fire " +
                    "in-browser. Rows before 2026-06-11 have no source attribution.",
            )}

            {#if !data.cloudflare.ok && !data.github.ok}
                {@render errorState(
                    [!data.cloudflare.ok ? data.cloudflare.error : '', !data.github.ok ? data.github.error : '']
                        .filter(Boolean)
                        .join('; ')
                )}
            {:else}
                <div class="space-y-4">
                    {#if data.cloudflare.ok}
                        {@const cf = data.cloudflare.data}
                        {@const totalDownloads = cf.downloads.reduce((sum, r) => sum + r.downloads, 0)}
                        {@const totalNewInstalls = cf.downloads.reduce((sum, r) => sum + r.uniqueDownloads, 0)}
                        {@const { days: allDays, timestamps: allTimestamps } = getDayAxis(cf.downloads)}
                        {@const sourceSeries = downloadSourceSeries(cf.downloads, allDays)}
                        {@const versions = aggregateBy(cf.downloads, 'version', 'downloads').sort((a, b) => compareSemverDesc(a.x, b.x)).slice(0, 8)}
                        {@const versionMaxY = maxDailyAcrossGroups(cf.downloads, 'version', versions.map((v) => v.x), allDays)}

                        {@render metricRow([
                            { label: 'New installs (deduped)', value: formatNumber(totalNewInstalls), color: COLOR_GOLD },
                            { label: 'Download requests (raw)', value: formatNumber(totalDownloads) },
                            ...(data.github.ok
                                ? [{ label: 'Downloads (GitHub, all-time)', value: formatNumber(data.github.data.totalDownloads) }]
                                : []),
                        ])}

                        {#if cf.downloads.length > 0}
                            <div class="mt-5">
                                <h3 class="mb-2 text-sm font-medium text-text-secondary">New installs per day, by source</h3>
                                <StackedBarChart days={allDays} series={sourceSeries} unitLabel="new installs" />
                                {@render methodology(
                                    'Counts downloads of the macOS DMG through getcmdr.com (download endpoint), deduplicated to distinct ' +
                                    'people per day via a daily-rotating hashed IP, with bot and link-preview hits dropped by user agent. ' +
                                    'Website = the getcmdr.com download button, Homebrew = `brew install --cask cmdr`, Direct / other = links ' +
                                    'shared elsewhere. In-app auto-updates never count here (they fetch from GitHub, not this endpoint). ' +
                                    'Hover a bar for exact numbers.'
                                )}
                            </div>
                        {/if}

                        {#if cf.downloads.length > 0 && allDays.length > 1}
                            <!-- svelte-ignore a11y_no_static_element_interactions -->
                            <div
                                class="grid gap-4 md:grid-cols-3"
                                onwheel={(e) => handleDownloadWheel(e, allTimestamps[0], allTimestamps[allTimestamps.length - 1])}
                                onmousemove={(e: MouseEvent) => { dayTooltipX = e.clientX; dayTooltipY = e.clientY }}
                            >
                                <div>
                                    <h3 class="mb-2 text-sm font-medium text-text-secondary">By version</h3>
                                    {#each versions as version}
                                        {@const timelineData = buildTimeline(
                                            cf.downloads.filter((r) => r.version === version.x),
                                            allDays,
                                            allTimestamps
                                        )}
                                        <div class="mb-1">
                                            <div class="flex items-baseline justify-between text-xs">
                                                <span class="text-text-primary">{version.x}</span>
                                                <span class="tabular-nums text-text-secondary">{formatNumber(version.y)}</span>
                                            </div>
                                            <MiniTimeline data={timelineData} height={48} maxY={versionMaxY} xMin={zoomXMin} xMax={zoomXMax} syncKey={downloadSyncKey} onhover={handleDayHover} />
                                        </div>
                                    {/each}
                                    <p class="text-xs text-text-tertiary">Scroll to zoom timeline</p>
                                </div>
                                <div>
                                    <h3 class="mb-2 text-sm font-medium text-text-secondary">By architecture</h3>
                                    {@render metricTable(aggregateBy(cf.downloads, 'arch', 'downloads').slice(0, 8), 'Architecture', 'Downloads')}
                                </div>
                                <div class="relative">
                                    <h3 class="mb-2 text-sm font-medium text-text-secondary">By country</h3>
                                    {@render countryTable(cf.downloads, allDays, allTimestamps)}
                                </div>
                            </div>

                            <!-- Data-point hover tooltip with pie charts -->
                            {#if dayTooltipVisible && hoveredDayIdx != null && hoveredDayIdx < allDays.length}
                                {@const day = allDays[hoveredDayIdx]}
                                {@const dayRows = cf.downloads.filter((r) => r.day === day)}
                                {@const dayTotal = dayRows.reduce((sum, r) => sum + r.downloads, 0)}
                                {#if dayTotal > 0}
                                    <div
                                        class="pointer-events-none fixed z-50 rounded-lg border border-border bg-surface-elevated p-3 shadow-lg"
                                        style="left: {dayTooltipX + 16}px; top: {Math.max(16, dayTooltipY - 100)}px;"
                                    >
                                        <p class="mb-2 text-sm font-medium text-text-primary">
                                            {day}
                                            <span class="ml-2 tabular-nums text-text-secondary">{formatNumber(dayTotal)} downloads</span>
                                        </p>
                                        <div class="flex items-start gap-5">
                                            <div>
                                                <p class="mb-1 text-xs font-medium text-text-tertiary">Architecture</p>
                                                <PieChart slices={aggregateBy(dayRows, 'arch', 'downloads').map((a) => ({ label: a.x, value: a.y }))} />
                                            </div>
                                            <div>
                                                <p class="mb-1 text-xs font-medium text-text-tertiary">Country</p>
                                                <PieChart slices={aggregateBy(dayRows, 'country', 'downloads').slice(0, 8).map((c) => ({ label: c.x.toUpperCase(), value: c.y }))} />
                                            </div>
                                        </div>
                                    </div>
                                {/if}
                            {/if}
                        {:else if cf.downloads.length > 0}
                            <!-- Fewer than 2 days of data: show tables only -->
                            <div class="grid gap-4 md:grid-cols-3">
                                <div>
                                    <h3 class="mb-2 text-sm font-medium text-text-secondary">By version</h3>
                                    {@render metricTable(aggregateBy(cf.downloads, 'version', 'downloads').sort((a, b) => compareSemverDesc(a.x, b.x)).slice(0, 8), 'Version', 'Downloads')}
                                </div>
                                <div>
                                    <h3 class="mb-2 text-sm font-medium text-text-secondary">By architecture</h3>
                                    {@render metricTable(aggregateBy(cf.downloads, 'arch', 'downloads').slice(0, 8), 'Architecture', 'Downloads')}
                                </div>
                                <div>
                                    <h3 class="mb-2 text-sm font-medium text-text-secondary">By country</h3>
                                    {@render metricTable(
                                        aggregateBy(cf.downloads, 'country', 'downloads').slice(0, 8).map((item) => ({ ...item, x: formatCountry(item.x) })),
                                        'Country', 'Downloads'
                                    )}
                                </div>
                            </div>
                        {/if}
                    {:else if !data.cloudflare.ok}
                        {@render errorState(data.cloudflare.error)}
                    {/if}

                    {#if data.github.ok && data.github.data.releases.length > 0}
                        <div>
                            <h3 class="mb-2 text-sm font-medium text-text-secondary">GitHub releases</h3>
                            <div class="overflow-x-auto">
                                <table class="w-full text-left text-sm">
                                    <thead>
                                        <tr class="border-b border-border-subtle text-text-tertiary">
                                            <th class="pb-2 pr-4 font-medium">Release</th>
                                            <th class="pb-2 text-right font-medium tabular-nums">Downloads</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {#each data.github.data.releases.slice(0, 5) as release}
                                            <tr class="border-b border-border-subtle/50">
                                                <td class="py-1.5 pr-4 text-text-primary">{release.tagName}</td>
                                                <td class="py-1.5 text-right tabular-nums text-text-secondary">{formatNumber(release.totalDownloads)}</td>
                                            </tr>
                                        {/each}
                                    </tbody>
                                </table>
                            </div>
                        </div>
                    {/if}
                </div>

                {@render externalLinks([
                    { label: 'View in Cloudflare', href: 'https://dash.cloudflare.com' },
                    { label: 'View on GitHub', href: 'https://github.com/vdavid/cmdr/releases' },
                ])}
            {/if}
        </section>

        <!-- 4. Active use -->
        <section class="rounded-xl border border-border bg-surface p-6">
            <div class="mb-1">
                <h2 class="text-lg font-semibold text-text-primary">Active use</h2>
                <p class="text-sm text-text-tertiary">How many run the app?</p>
            </div>
            {@render sectionDescription(
                "Use this for true daily active installs and how fast the fleet rolls onto each new release.",
                "DAU here comes from the hourly heartbeat (distinct install ids per day), the trustworthy active-use number. " +
                    "The older update-check count is a weaker proxy (it only fires when the updater runs). Debug builds and " +
                    "anyone who opts out of analytics are excluded.",
            )}

            {#if !data.cloudflare.ok}
                {@render errorState(data.cloudflare.error)}
            {:else}
                {@const cf = data.cloudflare.data}
                {@const dau = cf.heartbeatDau}
                {@const latestDau = dau.length > 0 ? dau[dau.length - 1].dau : 0}
                {@const peakDau = dau.reduce((max, r) => Math.max(max, r.dau), 0)}
                {@const totalBeats = dau.reduce((sum, r) => sum + r.beats, 0)}
                {@const totalDau = dau.reduce((sum, r) => sum + r.dau, 0)}
                {@const beatsPerActive = totalDau > 0 ? totalBeats / totalDau : 0}

                {#if dau.length > 0}
                    {@render metricRow([
                        { label: 'Daily active installs (latest day)', value: formatNumber(latestDau), color: COLOR_GOLD },
                        { label: 'Peak daily active', value: formatNumber(peakDau) },
                        { label: 'Beats per active install', value: beatsPerActive.toFixed(1) },
                    ])}

                    <div class="mt-4">
                        <h3 class="mb-2 text-sm font-medium text-text-secondary">Daily active installs</h3>
                        <Chart
                            data={[dau.map((r) => new Date(r.date).getTime() / 1000), dau.map((r) => r.dau)]}
                            labels={['Active installs']}
                            colors={[COLOR_GOLD]}
                            height={180}
                        />
                    </div>
                {:else}
                    {@render betaEmptyState()}
                {/if}

                {@const updateActivity = cf.updateActivity}
                {@const updateDays = uniqueDays(updateActivity)}
                {@const updateSeries = updateVersionSeries(updateActivity, updateDays)}
                {#if updateActivity.length > 0}
                    <div class="mt-6 border-t border-border-subtle pt-5">
                        <h3 class="mb-2 text-sm font-medium text-text-secondary">Got the latest release per day, by version</h3>
                        <StackedBarChart days={updateDays} series={updateSeries} unitLabel="installs" height={140} />
                        {@render methodology(
                            "Counts running installs with auto-update on that checked for updates each day (the app's update " +
                            'check hits our server, then redirects to the latest release), deduplicated to distinct installs per ' +
                            'day via a daily-rotating hashed IP. Stacked by the version each install was on when it checked, so you ' +
                            'see the fleet roll onto a new release. Separate from new installs above: these are existing users updating ' +
                            'in place, not fresh downloads. Hover a bar for exact numbers.'
                        )}
                    </div>
                {/if}

                {#if data.license.ok}
                    {@const lic = data.license.data}
                    <div class="mt-4 flex gap-6">
                        <div>
                            <p class="text-xs text-text-tertiary">Total activations</p>
                            <p class="text-lg font-semibold tabular-nums text-text-primary">{formatNumber(lic.totalActivations)}</p>
                        </div>
                        {#if lic.activeDevices !== null}
                            <div>
                                <p class="text-xs text-text-tertiary">Active devices</p>
                                <p class="text-lg font-semibold tabular-nums text-text-primary">{formatNumber(lic.activeDevices)}</p>
                            </div>
                        {/if}
                    </div>
                {/if}

                {@render externalLinks([
                    { label: 'View in Cloudflare', href: 'https://dash.cloudflare.com' },
                ])}
            {/if}
        </section>

        <!-- 5. Payment -->
        <section class="rounded-xl border border-border bg-surface p-6">
            <div class="mb-1">
                <h2 class="text-lg font-semibold text-text-primary">Payment</h2>
                <p class="text-sm text-text-tertiary">How many pay?</p>
            </div>
            {@render sectionDescription(
                "Use this for completed purchases and the revenue they brought in over the selected window.",
                "Paddle is the source of truth for money, so trust these numbers, just expect a little webhook lag near the " +
                    "present.",
            )}

            {#if !data.paddle.ok}
                {@render errorState(data.paddle.error)}
            {:else}
                {@const paddle = data.paddle.data}
                {@const totalRevenue = paddle.transactions.reduce((sum, t) => sum + Number(t.total), 0)}
                {@const currency = paddle.transactions[0]?.currencyCode ?? 'USD'}

                {@render metricRow([
                    { label: 'Revenue', value: formatCurrency(totalRevenue, currency) },
                    { label: 'Transactions', value: formatNumber(paddle.transactions.length) },
                    { label: 'Active subscriptions', value: formatNumber(paddle.activeSubscriptions.length) },
                ])}

                {#if paddle.transactions.length > 0}
                    <div class="mt-4">
                        <h3 class="mb-2 text-sm font-medium text-text-secondary">Recent transactions</h3>
                        <div class="overflow-x-auto">
                            <table class="w-full text-left text-sm">
                                <thead>
                                    <tr class="border-b border-border-subtle text-text-tertiary">
                                        <th class="pb-2 pr-4 font-medium">Date</th>
                                        <th class="pb-2 pr-4 font-medium">Status</th>
                                        <th class="pb-2 text-right font-medium">Amount</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {#each paddle.transactions.slice(0, 10) as txn}
                                        <tr class="border-b border-border-subtle/50">
                                            <td class="py-1.5 pr-4 tabular-nums text-text-primary">{txn.createdAt.split('T')[0]}</td>
                                            <td class="py-1.5 pr-4 text-text-secondary">{txn.status}</td>
                                            <td class="py-1.5 text-right tabular-nums text-text-secondary">{formatCurrency(txn.total, txn.currencyCode)}</td>
                                        </tr>
                                    {/each}
                                </tbody>
                            </table>
                        </div>
                    </div>
                {:else}
                    {@render emptyState()}
                {/if}

                {@render externalLinks([
                    { label: 'View in Paddle', href: 'https://vendors.paddle.com' },
                ])}
            {/if}
        </section>

        <!-- 6. Retention -->
        <section class="rounded-xl border border-border bg-surface p-6">
            <div class="mb-1">
                <h2 class="text-lg font-semibold text-text-primary">Retention</h2>
                <p class="text-sm text-text-tertiary">Do they stay?</p>
            </div>
            {@render sectionDescription(
                "Use this to see whether paying customers stick around: the split of subscriptions by status (active, " +
                    "canceled, and so on).",
                "From Paddle, so it's reliable for money. It covers paid subscriptions only, not free-tier engagement (that's " +
                    "the Active use section and the funnel's D7).",
            )}

            {#if !data.paddle.ok}
                {@render errorState(data.paddle.error)}
            {:else}
                {@const paddle = data.paddle.data}
                {@const statusEntries = Object.entries(paddle.subscriptionsByStatus)}
                {@const totalSubs = statusEntries.reduce((sum, entry) => sum + entry[1], 0)}
                {@const activeSubs = paddle.subscriptionsByStatus['active'] ?? 0}
                {@const canceledSubs = paddle.subscriptionsByStatus['canceled'] ?? 0}
                {@const churnDisplay = totalSubs > 0 ? `${((canceledSubs / totalSubs) * 100).toFixed(1)}%` : 'N/A'}

                {@render metricRow([
                    { label: 'Active subscriptions', value: formatNumber(activeSubs) },
                    { label: 'Churn rate', value: churnDisplay },
                ])}

                {#if statusEntries.length > 0}
                    <div class="mt-4">
                        <h3 class="mb-2 text-sm font-medium text-text-secondary">Subscriptions by status</h3>
                        {@render metricTable(
                            statusEntries.map((entry) => ({ x: entry[0], y: entry[1] })),
                            'Status',
                            'Count'
                        )}
                    </div>
                {:else}
                    {@render emptyState()}
                {/if}

                {@render externalLinks([
                    { label: 'View in Paddle', href: 'https://vendors.paddle.com' },
                ])}
            {/if}
        </section>

        <!-- 7. Feedback & errors -->
        <section class="rounded-xl border border-border bg-surface p-6">
            <div class="mb-1">
                <h2 class="text-lg font-semibold text-text-primary">Feedback &amp; errors</h2>
                <p class="text-sm text-text-tertiary">What are users telling us?</p>
            </div>
            {@render sectionDescription(
                "Use this to read what people sent through in-app feedback and to see error-report bundles roll in, so you " +
                    "catch pain points fast.",
                "Feedback carries no install id (it's unjoinable to analytics), and error reports use a separate diagnostics " +
                    "id. Both are low-volume, so a short window can look empty even when things are fine.",
            )}

            {#if !data.feedbackAndErrors.ok}
                {@render errorState(data.feedbackAndErrors.error)}
            {:else}
                {@const fe = data.feedbackAndErrors.data}
                {@const awaitingReply = countFeedbackWithReplyTo(fe.feedback)}
                {@const errorsPerDay = errorReportsByDay(fe.errorReports)}
                {@const errorsByKind = tallyErrorReportsByField(fe.errorReports, 'kind')}
                {@const errorsByVersion = tallyErrorReportsByField(fe.errorReports, 'appVersion')}

                {@render metricRow([
                    { label: 'Feedback messages', value: formatNumber(fe.feedback.length), color: COLOR_GOLD },
                    { label: 'Awaiting reply', value: formatNumber(awaitingReply) },
                    { label: 'Error reports', value: formatNumber(fe.errorReports.length) },
                ])}

                {#if errorsPerDay.length > 1}
                    <div class="mt-4">
                        <h3 class="mb-2 text-sm font-medium text-text-secondary">Error reports per day</h3>
                        <Chart
                            data={[errorsPerDay.map((d) => new Date(d.date).getTime() / 1000), errorsPerDay.map((d) => d.count)]}
                            labels={['Error reports']}
                            colors={[COLOR_GOLD]}
                            height={160}
                        />
                    </div>
                {/if}

                {#if fe.errorReports.length > 0}
                    <div class="mt-4 grid gap-4 md:grid-cols-2">
                        <div>
                            <h3 class="mb-2 text-sm font-medium text-text-secondary">By kind</h3>
                            {@render metricTable(errorsByKind.map((e) => ({ x: e.key, y: e.count })), 'Kind', 'Reports')}
                        </div>
                        <div>
                            <h3 class="mb-2 text-sm font-medium text-text-secondary">By version</h3>
                            {@render metricTable(errorsByVersion.map((e) => ({ x: e.key, y: e.count })), 'Version', 'Reports')}
                        </div>
                    </div>
                {/if}

                {#if fe.feedback.length > 0}
                    <div class="mt-4">
                        <h3 class="mb-2 text-sm font-medium text-text-secondary">Recent feedback</h3>
                        <div class="space-y-2">
                            {#each fe.feedback.slice(0, 15) as msg}
                                <div class="rounded-lg border border-border-subtle bg-surface-elevated px-3 py-2">
                                    <p class="whitespace-pre-wrap text-sm text-text-primary">{msg.feedback}</p>
                                    <div class="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-text-tertiary">
                                        <span class="tabular-nums">{msg.createdAt.split(' ')[0]}</span>
                                        <span>v{msg.appVersion}</span>
                                        <span>{msg.osVersion}</span>
                                        {#if msg.email}
                                            <a
                                                class="text-accent hover:text-accent-hover"
                                                href="mailto:{msg.email}?subject=Re%3A%20your%20Cmdr%20feedback"
                                            >
                                                Reply to {msg.email}
                                            </a>
                                        {/if}
                                    </div>
                                </div>
                            {/each}
                        </div>
                    </div>
                {:else}
                    {@render emptyState()}
                {/if}

                {#if fe.errorReports.length > 0}
                    <div class="mt-4">
                        <h3 class="mb-2 text-sm font-medium text-text-secondary">Recent error reports</h3>
                        <div class="overflow-x-auto">
                            <table class="w-full text-left text-sm">
                                <thead>
                                    <tr class="border-b border-border-subtle text-text-tertiary">
                                        <th class="pb-2 pr-4 font-medium">ID</th>
                                        <th class="pb-2 pr-4 font-medium">Kind</th>
                                        <th class="pb-2 pr-4 font-medium">Version</th>
                                        <th class="pb-2 text-right font-medium">Date</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {#each fe.errorReports.slice(0, 15) as report}
                                        <tr class="border-b border-border-subtle/50">
                                            <td class="py-1.5 pr-4 font-mono text-text-primary">{report.id}</td>
                                            <td class="py-1.5 pr-4 text-text-secondary">{report.kind}</td>
                                            <td class="py-1.5 pr-4 tabular-nums text-text-secondary">{report.appVersion}</td>
                                            <td class="py-1.5 text-right tabular-nums text-text-tertiary">{report.date}</td>
                                        </tr>
                                    {/each}
                                </tbody>
                            </table>
                        </div>
                    </div>
                {/if}

                {@render externalLinks([{ label: 'View bundles in Cloudflare R2', href: 'https://dash.cloudflare.com' }])}
            {/if}
        </section>
    </div>
</div>

<!-- Shared snippets -->
{#snippet errorState(error: string)}
    <div class="rounded-lg border border-border-subtle bg-surface-elevated px-4 py-6 text-center">
        <p class="text-sm text-text-secondary">Couldn't load this data</p>
        <p class="mt-1 text-xs text-text-tertiary">{error}</p>
        <a
            href={isDaySelected ? `?day=${selectedDay}` : `?range=${data.selection.range}`}
            class="mt-2 inline-block text-xs text-accent hover:text-accent-hover"
        >
            Try again
        </a>
    </div>
{/snippet}

{#snippet emptyState()}
    <div class="rounded-lg border border-border-subtle bg-surface-elevated px-4 py-6 text-center">
        <p class="text-sm text-text-secondary">No data yet for this period</p>
    </div>
{/snippet}

<!-- A small "how this is measured" note under a chart, so no number on the dashboard is a black box. -->
{#snippet methodology(text: string)}
    <p class="mt-2 text-xs leading-relaxed text-text-tertiary">{text}</p>
{/snippet}

<!--
  A section's "what is this, and how much to trust it" blurb, shown right under the section heading.
  `insight` is the one-line "use this to..." purpose; `caveat` is the reliability footnote (optional).
-->
{#snippet sectionDescription(insight: string, caveat?: string)}
    <p class="mb-4 text-xs leading-relaxed text-text-secondary">
        {insight}
        {#if caveat}<span class="text-text-tertiary">{caveat}</span>{/if}
    </p>
{/snippet}

{#snippet betaEmptyState()}
    <div class="rounded-lg border border-border-subtle bg-surface-elevated px-4 py-6 text-center">
        <p class="text-sm text-text-secondary">Daily active installs will appear here as beta testers update</p>
        <p class="mt-1 text-xs text-text-tertiary">
            The heartbeat starts empty at release and fills as testers run the new build.
        </p>
    </div>
{/snippet}

{#snippet metricRow(metrics: Array<{ label: string; value: string; delta?: { text: string; positive: boolean }; color?: string }>)}
    <div class="flex flex-wrap gap-6">
        {#each metrics as metric}
            <div>
                <p class="flex items-center gap-1.5 text-xs text-text-tertiary">
                    {#if metric.color}
                        <span class="inline-block h-2 w-2 rounded-full" style="background: {metric.color}"></span>
                    {/if}
                    {metric.label}
                </p>
                <div class="flex items-baseline gap-2">
                    <p class="text-2xl font-bold tabular-nums text-text-primary">{metric.value}</p>
                    {#if metric.delta}
                        <span class="text-sm tabular-nums {metric.delta.positive ? 'text-success' : 'text-danger'}">
                            {metric.delta.text}
                        </span>
                    {/if}
                </div>
            </div>
        {/each}
    </div>
{/snippet}

{#snippet metricTable(items: Array<{ x: string; y: number }>, colLabel: string, colValue: string)}
    <div class="overflow-x-auto">
        <table class="w-full text-left text-sm">
            <thead>
                <tr class="border-b border-border-subtle text-text-tertiary">
                    <th class="pb-2 pr-4 font-medium">{colLabel}</th>
                    <th class="pb-2 text-right font-medium">{colValue}</th>
                </tr>
            </thead>
            <tbody>
                {#each items as item}
                    <tr class="border-b border-border-subtle/50">
                        <td class="py-1.5 pr-4 text-text-primary">{item.x || '(direct)'}</td>
                        <td class="py-1.5 text-right tabular-nums text-text-secondary">{formatNumber(item.y)}</td>
                    </tr>
                {/each}
            </tbody>
        </table>
    </div>
{/snippet}

{#snippet countryTable(downloads: DownloadRow[], allDays: string[], allTimestamps: number[])}
    {@const countries = aggregateBy(downloads, 'country', 'downloads').slice(0, 8)}
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
                        onmouseenter={(e: MouseEvent) => { hoveredCountry = item.x; tooltipX = e.clientX; tooltipY = e.clientY }}
                        onmouseleave={() => { hoveredCountry = null }}
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
{/snippet}

{#snippet externalLinks(links: Array<{ label: string; href: string }>)}
    <div class="mt-4 flex flex-wrap gap-3 border-t border-border-subtle pt-3">
        {#each links as link}
            <a
                href={link.href}
                target="_blank"
                rel="noopener noreferrer"
                class="text-xs text-text-tertiary transition-colors hover:text-accent"
            >
                {link.label} &#x2197;
            </a>
        {/each}
    </div>
{/snippet}
