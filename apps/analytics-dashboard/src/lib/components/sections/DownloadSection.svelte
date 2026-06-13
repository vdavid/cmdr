<!--
  Acquisition § Download: real new installs by source (stacked bars), per-version mini-timelines,
  by-architecture and by-country tables, a per-day pie-chart hover tooltip (architecture + country),
  and GitHub release download counts. Owns the timeline zoom window (scroll to zoom) and the day-hover
  tooltip state; the country breakdown lives in CountryTable and shares the zoom window.
-->
<script lang="ts">
    import type { SourceResult, DashboardSelection } from '$lib/server/types.js'
    import type { CloudflareData } from '$lib/server/sources/cloudflare.js'
    import type { GitHubData } from '$lib/server/sources/github.js'
    import {
        aggregateBy,
        buildTimeline,
        compareSemverDesc,
        downloadSourceSeries,
        getDayAxis,
        maxDailyAcrossGroups,
    } from '$lib/chart-helpers.js'
    import { formatNumber, formatCountry } from '$lib/format.js'
    import { COLOR_GOLD } from '$lib/colors.js'
    import Chart from '$lib/components/Chart.svelte'
    import MiniTimeline from '$lib/components/MiniTimeline.svelte'
    import PieChart from '$lib/components/PieChart.svelte'
    import StackedBarChart from '$lib/components/StackedBarChart.svelte'
    import CountryTable from '$lib/components/CountryTable.svelte'
    import ErrorState from '$lib/components/ErrorState.svelte'
    import MetricRow from '$lib/components/MetricRow.svelte'
    import MetricTable from '$lib/components/MetricTable.svelte'
    import Methodology from '$lib/components/Methodology.svelte'
    import SectionDescription from '$lib/components/SectionDescription.svelte'
    import ExternalLinks from '$lib/components/ExternalLinks.svelte'

    let {
        cloudflare,
        github,
        selection,
    }: {
        cloudflare: SourceResult<CloudflareData>
        github: SourceResult<GitHubData>
        selection: DashboardSelection
    } = $props()

    const downloadSyncKey = 'dl-timelines'

    let zoomXMin: number | null = $state(null)
    let zoomXMax: number | null = $state(null)

    // Data-point hover state (the per-day pie tooltip).
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
</script>

<section class="rounded-xl border border-border bg-surface p-6">
    <div class="mb-1">
        <h2 class="text-lg font-semibold text-text-primary">Download</h2>
        <p class="text-sm text-text-tertiary">How many actually download?</p>
    </div>
    <SectionDescription
        insight={'Use this to see real new installs by source (website, Homebrew, other) and which release and platform ' +
            'people are grabbing.'}
        caveat={"These are server downloads from the app's own endpoint, deduped per day by a daily-rotating hashed IP, " +
            "with bot user agents dropped (imperfectly). It won't match the Umami download clicks above, which fire " +
            'in-browser. Rows before 2026-06-11 have no source attribution.'}
    />

    {#if !cloudflare.ok && !github.ok}
        <ErrorState
            error={[!cloudflare.ok ? cloudflare.error : '', !github.ok ? github.error : ''].filter(Boolean).join('; ')}
            {selection}
        />
    {:else}
        <div class="space-y-4">
            {#if cloudflare.ok}
                {@const cf = cloudflare.data}
                {@const totalDownloads = cf.downloads.reduce((sum, r) => sum + r.downloads, 0)}
                {@const totalNewInstalls = cf.downloads.reduce((sum, r) => sum + r.uniqueDownloads, 0)}
                {@const { days: allDays, timestamps: allTimestamps } = getDayAxis(cf.downloads)}
                {@const sourceSeries = downloadSourceSeries(cf.downloads, allDays)}
                {@const versions = aggregateBy(cf.downloads, 'version', 'downloads')
                    .sort((a, b) => compareSemverDesc(a.x, b.x))
                    .slice(0, 8)}
                {@const versionMaxY = maxDailyAcrossGroups(cf.downloads, 'version', versions.map((v) => v.x), allDays)}

                <MetricRow
                    metrics={[
                        { label: 'New installs (deduped)', value: formatNumber(totalNewInstalls), color: COLOR_GOLD },
                        { label: 'Download requests (raw)', value: formatNumber(totalDownloads) },
                        ...(github.ok
                            ? [{ label: 'Downloads (GitHub, all-time)', value: formatNumber(github.data.totalDownloads) }]
                            : []),
                    ]}
                />

                {#if cf.downloads.length > 0}
                    <div class="mt-5">
                        <h3 class="mb-2 text-sm font-medium text-text-secondary">New installs per day, by source</h3>
                        <StackedBarChart days={allDays} series={sourceSeries} unitLabel="new installs" />
                        <Methodology
                            text={'Counts downloads of the macOS DMG through getcmdr.com (download endpoint), deduplicated to distinct ' +
                                'people per day via a daily-rotating hashed IP, with bot and link-preview hits dropped by user agent. ' +
                                'Website = the getcmdr.com download button, Homebrew = `brew install --cask cmdr`, Direct / other = links ' +
                                'shared elsewhere. In-app auto-updates never count here (they fetch from GitHub, not this endpoint). ' +
                                'Hover a bar for exact numbers.'}
                        />
                    </div>
                {/if}

                {#if cf.downloads.length > 0 && allDays.length > 1}
                    <!-- svelte-ignore a11y_no_static_element_interactions -->
                    <div
                        class="grid gap-4 md:grid-cols-3"
                        onwheel={(e) => handleDownloadWheel(e, allTimestamps[0], allTimestamps[allTimestamps.length - 1])}
                        onmousemove={(e: MouseEvent) => {
                            dayTooltipX = e.clientX
                            dayTooltipY = e.clientY
                        }}
                    >
                        <div>
                            <h3 class="mb-2 text-sm font-medium text-text-secondary">By version</h3>
                            {#each versions as version}
                                {@const timelineData = buildTimeline(
                                    cf.downloads.filter((r) => r.version === version.x),
                                    allDays,
                                    allTimestamps,
                                )}
                                <div class="mb-1">
                                    <div class="flex items-baseline justify-between text-xs">
                                        <span class="text-text-primary">{version.x}</span>
                                        <span class="tabular-nums text-text-secondary">{formatNumber(version.y)}</span>
                                    </div>
                                    <MiniTimeline
                                        data={timelineData}
                                        height={48}
                                        maxY={versionMaxY}
                                        xMin={zoomXMin}
                                        xMax={zoomXMax}
                                        syncKey={downloadSyncKey}
                                        onhover={handleDayHover}
                                    />
                                </div>
                            {/each}
                            <p class="text-xs text-text-tertiary">Scroll to zoom timeline</p>
                        </div>
                        <div>
                            <h3 class="mb-2 text-sm font-medium text-text-secondary">By architecture</h3>
                            <MetricTable
                                items={aggregateBy(cf.downloads, 'arch', 'downloads').slice(0, 8)}
                                colLabel="Architecture"
                                colValue="Downloads"
                            />
                        </div>
                        <div class="relative">
                            <h3 class="mb-2 text-sm font-medium text-text-secondary">By country</h3>
                            <CountryTable downloads={cf.downloads} {allDays} {allTimestamps} {zoomXMin} {zoomXMax} />
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
                                        <PieChart
                                            slices={aggregateBy(dayRows, 'arch', 'downloads').map((a) => ({
                                                label: a.x,
                                                value: a.y,
                                            }))}
                                        />
                                    </div>
                                    <div>
                                        <p class="mb-1 text-xs font-medium text-text-tertiary">Country</p>
                                        <PieChart
                                            slices={aggregateBy(dayRows, 'country', 'downloads')
                                                .slice(0, 8)
                                                .map((c) => ({ label: c.x.toUpperCase(), value: c.y }))}
                                        />
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
                            <MetricTable
                                items={aggregateBy(cf.downloads, 'version', 'downloads')
                                    .sort((a, b) => compareSemverDesc(a.x, b.x))
                                    .slice(0, 8)}
                                colLabel="Version"
                                colValue="Downloads"
                            />
                        </div>
                        <div>
                            <h3 class="mb-2 text-sm font-medium text-text-secondary">By architecture</h3>
                            <MetricTable
                                items={aggregateBy(cf.downloads, 'arch', 'downloads').slice(0, 8)}
                                colLabel="Architecture"
                                colValue="Downloads"
                            />
                        </div>
                        <div>
                            <h3 class="mb-2 text-sm font-medium text-text-secondary">By country</h3>
                            <MetricTable
                                items={aggregateBy(cf.downloads, 'country', 'downloads')
                                    .slice(0, 8)
                                    .map((item) => ({ ...item, x: formatCountry(item.x) }))}
                                colLabel="Country"
                                colValue="Downloads"
                            />
                        </div>
                    </div>
                {/if}
            {:else if !cloudflare.ok}
                <ErrorState error={cloudflare.error} {selection} />
            {/if}

            {#if github.ok && github.data.releases.length > 0}
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
                                {#each github.data.releases.slice(0, 5) as release}
                                    <tr class="border-b border-border-subtle/50">
                                        <td class="py-1.5 pr-4 text-text-primary">{release.tagName}</td>
                                        <td class="py-1.5 text-right tabular-nums text-text-secondary"
                                            >{formatNumber(release.totalDownloads)}</td
                                        >
                                    </tr>
                                {/each}
                            </tbody>
                        </table>
                    </div>
                </div>
            {/if}
        </div>

        <ExternalLinks
            links={[
                { label: 'View in Cloudflare', href: 'https://dash.cloudflare.com' },
                { label: 'View on GitHub', href: 'https://github.com/vdavid/cmdr/releases' },
            ]}
        />
    {/if}
</section>
