<script lang="ts" module>
    import type { DownloadRow } from '$lib/server/sources/cloudflare.js'

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

    let { data } = $props()

    const ranges = ['24h', '7d', '30d'] as const
    const downloadSyncKey = 'dl-timelines'

    // Color palette
    const COLOR_GOLD = '#ffc206'
    const COLOR_PURPLE = '#a78bfa'
    const COLOR_GREEN = '#8faa3b' // autumn-y green for veszelovszki.com

    /** Time range in seconds for the selected range. Used as default zoom for star charts. */
    const rangeSeconds: Record<string, number> = { '24h': 86400, '7d': 7 * 86400, '30d': 30 * 86400 }
    function starChartXMin(): number {
        return Date.now() / 1000 - (rangeSeconds[data.range] ?? 7 * 86400)
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
                        onclick={() => { zoomXMin = null; zoomXMax = null; if (r !== data.range) window.location.href = `?range=${r}` }}
                        class="rounded-md px-3 py-1 text-sm font-medium transition-colors
                            {data.range === r
                            ? 'bg-accent text-accent-contrast'
                            : 'text-text-secondary hover:text-text-primary'}"
                    >
                        {r}
                    </button>
                {/each}
            </div>
            <span class="text-xs text-text-tertiary">
                Updated {formatTime(data.updatedAt)}
            </span>
        </div>
    </header>

    <!-- Sections -->
    <div class="grid grid-cols-1 gap-6 md:grid-cols-2 xl:grid-cols-3">
        <!-- 1. Awareness -->
        <section class="rounded-xl border border-border bg-surface p-6">
            <div class="mb-4">
                <h2 class="text-lg font-semibold text-text-primary">Awareness</h2>
                <p class="text-sm text-text-tertiary">How many people see Cmdr content?</p>
            </div>

            {#if !data.umami.ok}
                {@render errorState(data.umami.error)}
            {:else}
                {@const umami = data.umami.data}
                {@const totalPageviews = umami.personalSite.pageviews.value + umami.website.pageviews.value}
                {@const prevPageviews = umami.personalSite.pageviews.prev + umami.website.pageviews.prev}
                {@const delta = formatDelta(totalPageviews, prevPageviews)}

                {@render metricRow([
                    { label: 'Total page views', value: formatNumber(totalPageviews), delta },
                    { label: 'veszelovszki.com views', value: formatNumber(umami.personalSite.pageviews.value), color: COLOR_GREEN },
                    { label: 'getcmdr.com views', value: formatNumber(umami.website.pageviews.value), color: COLOR_GOLD },
                ])}

                {#if umami.websiteReferrers.length > 0}
                    <div class="mt-4">
                        <h3 class="mb-2 text-sm font-medium text-text-secondary">Top referrers</h3>
                        {@render metricTable(umami.websiteReferrers.slice(0, 10), 'Source', 'Views')}
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
                    { label: 'cmdr on GitHub', href: 'https://github.com/vdavid/cmdr' },
                    { label: 'mtp-rs on GitHub', href: 'https://github.com/vdavid/mtp-rs' },
                ])}
            {/if}
        </section>

        <!-- 2. Interest -->
        <section class="rounded-xl border border-border bg-surface p-6">
            <div class="mb-4">
                <h2 class="text-lg font-semibold text-text-primary">Interest</h2>
                <p class="text-sm text-text-tertiary">How many engage with the product page?</p>
            </div>

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
            <div class="mb-4">
                <h2 class="text-lg font-semibold text-text-primary">Download</h2>
                <p class="text-sm text-text-tertiary">How many actually download?</p>
            </div>

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
                        {@const { days: allDays, timestamps: allTimestamps } = getDayAxis(cf.downloads)}
                        {@const versions = aggregateBy(cf.downloads, 'version', 'downloads').sort((a, b) => compareSemverDesc(a.x, b.x)).slice(0, 8)}
                        {@const versionMaxY = maxDailyAcrossGroups(cf.downloads, 'version', versions.map((v) => v.x), allDays)}

                        {@render metricRow([
                            { label: 'Downloads (Analytics Engine)', value: formatNumber(totalDownloads) },
                            ...(data.github.ok
                                ? [{ label: 'Downloads (GitHub, all-time)', value: formatNumber(data.github.data.totalDownloads) }]
                                : []),
                        ])}

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
                            <!-- Fewer than 2 days of data — show tables only -->
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
            <div class="mb-4">
                <h2 class="text-lg font-semibold text-text-primary">Active use</h2>
                <p class="text-sm text-text-tertiary">How many run the app?</p>
            </div>

            {#if !data.cloudflare.ok}
                {@render errorState(data.cloudflare.error)}
            {:else}
                {@const cf = data.cloudflare.data}
                {@const totalChecks = cf.updateChecks.reduce((sum, r) => sum + r.checks, 0)}

                {@render metricRow([
                    { label: 'Update checks (approximate active users)', value: formatNumber(totalChecks) },
                ])}

                {#if cf.updateChecks.length > 0}
                    <div class="mt-4">
                        <h3 class="mb-2 text-sm font-medium text-text-secondary">By version</h3>
                        {@render metricTable(
                            cf.updateChecks.slice(0, 10).map((r) => ({ x: r.version, y: r.checks })),
                            'Version',
                            'Checks'
                        )}
                    </div>
                {:else}
                    {@render emptyState()}
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
            <div class="mb-4">
                <h2 class="text-lg font-semibold text-text-primary">Payment</h2>
                <p class="text-sm text-text-tertiary">How many pay?</p>
            </div>

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
            <div class="mb-4">
                <h2 class="text-lg font-semibold text-text-primary">Retention</h2>
                <p class="text-sm text-text-tertiary">Do they stay?</p>
            </div>

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
    </div>
</div>

<!-- Shared snippets -->
{#snippet errorState(error: string)}
    <div class="rounded-lg border border-border-subtle bg-surface-elevated px-4 py-6 text-center">
        <p class="text-sm text-text-secondary">Couldn't load this data</p>
        <p class="mt-1 text-xs text-text-tertiary">{error}</p>
        <a href="?range={data.range}" class="mt-2 inline-block text-xs text-accent hover:text-accent-hover">
            Try again
        </a>
    </div>
{/snippet}

{#snippet emptyState()}
    <div class="rounded-lg border border-border-subtle bg-surface-elevated px-4 py-6 text-center">
        <p class="text-sm text-text-secondary">No data yet for this period</p>
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
