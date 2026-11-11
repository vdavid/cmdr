<!--
  The top of the Acquisition page: one row per funnel metric, each a daily chart plus the shared 90/30/7
  stats grid, scoped by the country chips. The chips filter in the browser (`#lib/acquisition-by-country.ts`
  over the one preloaded funnel), so switching is instant. Every stat comes from `#lib/acquisition-stats.ts`
  (today excluded, rates pooled as ratios of sums); this component only picks inputs and formats.
-->
<script lang="ts">
    import type { SourceResult, DashboardSelection } from '#lib/server/types.js'
    import { rollingDays, type ViewWindow } from '#lib/time-range.js'
    import type { FunnelData, FunnelRow } from '#lib/server/sources/funnel.js'
    import { breakdownRows, type ColumnSeries, type TooltipRow } from '#lib/daily-chart.js'
    import {
        countStats,
        ratioStats,
        rollingRatio,
        statWindows,
        type CountPoint,
        type RatioPoint,
        type WindowStats,
    } from '#lib/acquisition-stats.js'
    import { buildTrendInputs, d7End, d28End, rateBands } from '#lib/acquisition-series.js'
    import { safeRatio } from '#lib/acquisition-countries.js'
    import {
        bandsLabel,
        countryChips,
        countryLabel,
        countrySeries,
        countryStackEntries,
        installCountryEntries,
        installSplit,
        otherCountries,
        rowsForCountry,
        splitSeries,
        stackByCountry,
    } from '#lib/acquisition-by-country.js'
    import { COLOR_OTHER_COUNTRIES, countryColor } from '#lib/country-colors.js'
    import {
        formatAverage,
        formatCountry,
        formatMultiple,
        formatMultipleChange,
        formatNumber,
        formatPercent,
        formatPointChange,
        formatRelativeChange,
    } from '#lib/format.js'
    import { COLOR_GOLD, COLOR_HOMEBREW, COLOR_UNKNOWN } from '#lib/colors.js'
    import { aggregateChannels, aggregateReferers, aggregateUaFamilies, type ChannelCount } from '#lib/funnel.js'
    import { foldSlices, type PieInput } from '#lib/pie-slices.js'
    import DailyChart from '../DailyChart.svelte'
    import DonutChart from '../DonutChart.svelte'
    import StatsGrid from '../StatsGrid.svelte'
    import TrendRow from '../TrendRow.svelte'
    import InfoPopover from '../InfoPopover.svelte'
    import ErrorState from '../ErrorState.svelte'

    interface Props {
        funnel: SourceResult<FunnelData>
        /** The selected country chip, or `null` for all countries. */
        country: string | null
        /** Switch the chip: the page filters in the browser and keeps `?country=` in sync. */
        onselectcountry: (country: string | null) => void
        /**
         * The days the charts and pies show (`viewWindow`): the picker's range plus today (drawn faded), or a
         * past window. The stats grids ignore it and stay on the windows ending today.
         */
        window: ViewWindow
        selection: DashboardSelection
        todayIso: string
    }

    const { funnel, country, onselectcountry, window: shown, selection, todayIso }: Props = $props()
    const tentativeColor = '#8a8a93'

    const data = $derived(funnel.ok ? funnel.data : null)
    const countries = $derived(data?.countries ?? { top: [], extra: null, umami: false, worker: false, breakdowns: false })
    const chips = $derived(countries.extra ? [...countries.top, countries.extra] : countries.top)
    /** The chips in display order: by visitors over the window in view, each with that count. */
    const rankedChips = $derived(data ? countryChips(data, shown) : [])
    /** The rows the charts and stats read: the totals, or one country's. */
    const rows = $derived(data ? (country ? rowsForCountry(data, country) : data.rows) : [])
    /** Each top country's rows, for the country stacks (only drawn for all countries). */
    const perCountry = $derived(data && !country ? countries.top.map((code) => ({ code, rows: rowsForCountry(data, code) })) : [])
    /** The color a single-series chart wears: gold for all countries, the country's own when filtered. */
    const seriesColor = $derived(country ? countryColor(country) : COLOR_GOLD)

    type StatsByWindow = Record<(typeof statWindows)[number], WindowStats>

    function countStatsAll(points: CountPoint[]): StatsByWindow {
        return { 90: countStats(points, 90, todayIso), 30: countStats(points, 30, todayIso), 7: countStats(points, 7, todayIso) }
    }
    function ratioStatsAll(points: RatioPoint[], end = todayIso): StatsByWindow {
        return { 90: ratioStats(points, 90, end), 30: ratioStats(points, 30, end), 7: ratioStats(points, 7, end) }
    }

    /** The rolling rate over complete days only; today's partial day gets no rolling point. */
    function rollingComplete(points: RatioPoint[]): (number | null)[] {
        return rollingRatio(points.map((p) => (p.date >= todayIso ? { ...p, num: null } : p)), rollingDays)
    }

    function daily(points: RatioPoint[]): (number | null)[] {
        return points.map((p) => safeRatio(p.num, p.den))
    }

    const inputs = $derived(buildTrendInputs(rows, todayIso))
    /**
     * The rows in `shown`, as indexes into the full history (oldest first, one row per day): `start` is the
     * first drawn, `stop` one past the last. Series index with `start + i` from there.
     */
    const start = $derived.by(() => {
        const i = rows.findIndex((r) => r.date >= shown.start)
        return i === -1 ? rows.length : i
    })
    const stop = $derived(rows.findLastIndex((r) => r.date <= shown.end) + 1)
    const days = $derived(inputs.days.slice(start, stop))
    const rolling = $derived({
        visitorToClick: rollingComplete(inputs.visitorToClick),
        clickToInstall: rollingComplete(inputs.clickToInstall),
        botMeter: rollingComplete(inputs.botMeter),
        // Non-final cohorts carry no numerator, so the retention lines only pool final cohorts.
        d7: rollingRatio(inputs.d7, rollingDays),
        d28: rollingRatio(inputs.d28, rollingDays),
    })

    /** The visible slice of a full-history series. */
    function view<T>(values: T[]): T[] {
        return values.slice(start, stop)
    }

    function cell(value: number | null, format: (n: number) => string = formatNumber): string {
        return value === null ? '–' : format(value)
    }

    /**
     * A count stacked by the top countries plus "Other", when all countries are shown and Umami answered per
     * country; otherwise `null` (the chart draws one series).
     */
    function stackFor(pick: (r: FunnelRow) => number | null, perCountryKnown: boolean): ColumnSeries[] | null {
        if (country || !perCountryKnown || perCountry.length === 0) return null
        return countrySeries(
            stackByCountry(
                view(rows.map(pick)),
                perCountry.map((c) => ({ code: c.code, values: view(c.rows.map(pick)) })),
            ),
        )
    }

    /** The tooltip for a country stack: the total, then the day's countries, biggest first (`breakdownRows`). */
    function stackTooltip(stack: ColumnSeries[], total: number | null, i: number, totalLabel: string): TooltipRow[] {
        return [{ label: totalLabel, value: cell(total) }, ...breakdownRows(countryStackEntries(stack, i), total)]
    }

    /** A retention chart's readout: a final cohort's rate and counts, or a young one's "so far" point. */
    function retentionTooltip(
        name: string,
        points: RatioPoint[],
        tentative: ({ value: number; label: string } | null)[],
        line: (number | null)[],
        soFar: (r: FunnelRow) => { retained: number } | null,
        i: number,
    ): TooltipRow[] {
        const r = rows[start + i]
        const p = points[start + i]
        const t = tentative[start + i]
        if (p.num === null) {
            return [
                {
                    label: t ? t.label : name,
                    value: t ? `${formatPercent(t.value)} (${cell(soFar(r)?.retained ?? null)} of ${cell(r.newInstalls)})` : 'too young',
                    color: tentativeColor,
                    mark: 'dot',
                },
            ]
        }
        return [
            { label: name, value: cell(safeRatio(p.num, p.den), formatPercent), color: seriesColor, mark: 'dot' },
            { label: 'Retained / cohort', value: `${cell(p.num)} / ${cell(p.den)}` },
            { label: '7-cohort rate', value: cell(line[start + i], formatPercent), color: seriesColor, mark: 'line' },
        ]
    }

    const visitorStack = $derived(stackFor((r) => r.visitors, countries.umami))
    const clickStack = $derived(stackFor((r) => r.downloadClicks, countries.umami))

    /** The new-installs split bar's right half, when all countries are shown and both splits exist. */
    const installRight = $derived.by(() => {
        if (country || !countries.worker || !inputs.has.channels || perCountry.length === 0) return null
        return installSplit(
            view(inputs.installsByChannel),
            perCountry.map((c) => ({ code: c.code, channels: view(c.rows.map((r) => r.newInstallsByChannel)) })),
        )
    })

    /**
     * The country-stacked bar charts (visitors, clicks, new installs) draw twice the default 180px: a slice
     * under 4px folds into "Other" (`foldBelowPx`), so the extra height is what lets more countries keep
     * their own slice on an ordinary day.
     */
    const stackedChartHeight = 360

    /** A ranked breakdown as pie entries, naming the no-value bucket so it can't pass for a channel. */
    function pieEntries(counts: ChannelCount[]): PieInput[] {
        return counts.map((c) => ({ key: c.ref, label: c.ref === '(none)' ? '(none / unknown)' : c.ref, count: c.count }))
    }

    /**
     * The three download breakdowns under the clicks chart, over the same days the charts show (`shown`).
     * The per-day maps arrive with the funnel's rows, so the country filter re-slices in the browser.
     */
    const breakdowns = $derived.by(() => {
        const inRange = rows.slice(start, stop)
        const ua = aggregateUaFamilies(inRange)
        return [
            {
                title: 'Channels',
                info: channelsInfo,
                empty: 'No downloads with a channel in this range.',
                headline: null,
                slices: foldSlices(pieEntries(aggregateChannels(inRange)), { neutral: ['(none)'] }),
            },
            {
                title: 'Download referrers',
                info: referrersInfo,
                empty: 'No downloads with a referer in this range.',
                headline: null,
                slices: foldSlices(pieEntries(aggregateReferers(inRange)), { neutral: ['(none)'] }),
            },
            {
                title: 'Downloads by client',
                info: clientsInfo,
                empty: 'No downloads to classify in this range.',
                headline:
                    ua.total > 0
                        ? `Human installs: ${formatNumber(ua.humanInstalls)} of ${formatNumber(ua.total)} server downloads`
                        : null,
                slices: foldSlices(
                    [
                        { key: 'human', label: 'Human', count: ua.human },
                        { key: 'bot', label: 'Bot / impossible', count: ua.bot },
                        { key: 'unknown', label: 'Unknown', count: ua.unknown },
                    ],
                    { neutral: ['unknown'], minShare: 0 },
                ),
            },
        ]
    })
</script>

<!-- The download breakdowns' (i) content, top-level so `breakdowns` can carry it -->
{#snippet downloadLogSource()}
    <p><span class="info-label">Source:</span> the api-server's download log, UTC days.</p>
{/snippet}

{#snippet channelsInfo()}
    <p>Which channel drove each server download.</p>
    <ul>
        <li>The channel is where the visitor first arrived from: a UTM source or campaign, else the referring site.</li>
        <li>
            It's first-touch per browser visit and comes from the URL only, so return visits and cross-device journeys
            (read on the phone, download on the Mac) carry none.
        </li>
        <li>Homebrew and direct links have none either, and rows before 2026-06-12 predate the column.</li>
        <li>So read "(none / unknown)" as "channel unknown". It doesn't mean direct.</li>
    </ul>
    {@render downloadLogSource()}
{/snippet}

{#snippet referrersInfo()}
    <p>The Referer host of each /download request.</p>
    <ul>
        <li>
            It's captured on every hit (the channel only comes from the website button), so it shows where the no-ref
            downloads came from: AlternativeTo, a directory, GitHub, Reddit, or a forum.
        </li>
        <li>
            "(none / unknown)" means no usable referer: a typed URL, a privacy browser, a stripped referrer, Homebrew or
            curl, or rows before 2026-06-25.
        </li>
        <li>Each download counts here and in Channels, sliced differently.</li>
    </ul>
    {@render downloadLogSource()}
{/snippet}

{#snippet clientsInfo()}
    <p>The user-agent family of each /download request.</p>
    <ul>
        <li>
            Cmdr is macOS-only, so a Windows, Android, or Linux client can't install the .dmg. That's the "Bot /
            impossible" slice, the one high-confidence exclusion.
        </li>
        <li>"Human installs" is every download minus those.</li>
        <li>
            "Human" (a Mac browser, Homebrew, or curl/wget) means "could be a real install". The scraper spoofs Mac
            browser user agents too, so it's no proof.
        </li>
        <li>"Unknown" (no or unrecognized user agent, including rows before we captured it) stays counted, since we can't tell.</li>
    </ul>
    {@render downloadLogSource()}
{/snippet}

<section class="mb-6 rounded-xl border border-border bg-surface p-4 sm:p-6">
    <div class="mb-4 flex flex-wrap items-center justify-between gap-3">
        <h2 class="flex items-center gap-2 text-lg font-semibold text-text-primary">
            Acquisition trends
            <InfoPopover topic="reading these charts">
                <p>One row per funnel step: a daily chart, then its 90, 30, and 7-day stats.</p>
                <ul>
                    <li>Every day is a UTC day. Today is still going, so it's faded and left out of every stat.</li>
                    <li>The charts show the dates in the header; step back with ‹ and ›.</li>
                    <li>
                        The stats always cover the last 90, 30, and 7 complete days, even on an earlier period. "Vs.
                        previous" compares with the same-length window before.
                    </li>
                    <li>Hover or focus a chart (then use the arrow keys) for each day's exact values.</li>
                </ul>
                <p>
                    <span class="info-label">Rates</span> pool as a ratio of sums (all clicks ÷ all visitors), so a quiet
                    day can't skew them. The median is the one per-day rate stat.
                </p>
                <p>A dash means we couldn't get that number. A real zero shows as 0.</p>
            </InfoPopover>
        </h2>
    </div>

    <!-- Country filter: re-scopes every chart and stat below, in the browser -->
    <div class="mb-2 flex flex-wrap gap-1.5" role="group" aria-label="Filter by country">
        {#each [{ code: null, visitors: null }, ...rankedChips] as { code, visitors } (code ?? 'all')}
            {@const selected = country === code}
            <button
                type="button"
                class="flex items-center gap-1.5 rounded-full border px-3 py-1 text-xs {selected
                    ? 'border-accent bg-accent/15 text-text-primary'
                    : 'border-border text-text-secondary hover:text-text-primary'}"
                aria-pressed={selected}
                onclick={() => { onselectcountry(code); }}
            >
                {#if code}
                    <span class="inline-block size-2 rounded-full" style="background-color: {countryColor(code)};"></span>
                    {formatCountry(code)}{visitors === null ? '' : ` - ${formatNumber(visitors)}`}
                {:else}
                    All
                {/if}
            </button>
        {/each}
    </div>
    {#if country}
        <p class="mb-2 text-xs text-text-tertiary">
            Showing {formatCountry(country)} only. Small numbers swing a lot day to day, so lean on the 30d and 90d
            columns. Per-country installs count only installs whose first heartbeat carried a country, so they start
            small.
        </p>
    {/if}

    {#if funnel.ok}
        {@const umamiAsked = countries.umami && (country === null || chips.includes(country))}
        {@const umamiDown = rows.every((r) => r.visitors === null)}
        {@const workerReason =
            funnel.data.workerStatus === 'unavailable'
                ? "Couldn't load this from the api-server."
                : country && !countries.worker
                  ? "Not available yet for one country: the live api-server doesn't split by country until its update lands."
                  : null}
        {@const umamiReason = umamiDown
            ? country && !umamiAsked
                ? "Not available for this country: Umami's daily numbers load for the top countries only."
                : "Couldn't load this from Umami."
            : null}
        {@const bothReason = umamiReason ?? workerReason}

        <!-- 1. Visitors -->
        {@const visitorStats = countStatsAll(inputs.visitors)}
        {@const ppv = ratioStatsAll(inputs.pageviewsPerVisitor)}
        <TrendRow title="Visitors" unavailable={umamiReason}>
            {#snippet info()}
                <p>Unique visitors to getcmdr.com per UTC day.</p>
                <ul>
                    <li>
                        Across all countries, each column stacks the top {countries.top.length} countries by 90-day visitors,
                        each in its own color, with the rest as grey "Other".
                    </li>
                    <li>Pageviews per visitor pools all pageviews ÷ all visitors in the window.</li>
                    <li>Ad blockers hide some visitors, and one person on two devices counts twice.</li>
                </ul>
                <p><span class="info-label">Source:</span> Umami (cookieless, in the browser).</p>
            {/snippet}
            {#snippet chart()}
                <DailyChart
                    height={stackedChartHeight}
                    {days}
                    today={todayIso}
                    label="Daily visitors"
                    formatValue={formatNumber}
                    series={visitorStack ?? [{ key: 'visitors', label: 'Visitors', color: seriesColor, values: view(inputs.visitors.map((p) => p.value)) }]}
                    tooltip={(i: number): TooltipRow[] => {
                        const r = rows[start + i]
                        return [
                            ...(visitorStack
                                ? stackTooltip(visitorStack, r.visitors, i, 'Visitors')
                                : [{ label: 'Visitors', value: cell(r.visitors), color: seriesColor }]),
                            { label: 'Pageviews', value: cell(r.pageviews) },
                            { label: 'Pageviews per visitor', value: cell(safeRatio(r.pageviews, r.visitors), formatAverage) },
                        ]
                    }}
                />
            {/snippet}
            {#snippet stats()}
                <StatsGrid
                    stats={visitorStats}
                    formatValue={formatAverage}
                    formatChange={formatRelativeChange}
                    extraRows={[{ label: 'Pageviews / visitor', values: statWindows.map((w) => cell(ppv[w].average, formatAverage)) }]}
                />
            {/snippet}
        </TrendRow>

        <!-- 2. Visitor → click conversion -->
        <TrendRow title="Visitor → click conversion" unavailable={umamiReason}>
            {#snippet info()}
                <p>Download-button clicks ÷ visitors.</p>
                <ul>
                    <li>Dots are each day's rate.</li>
                    <li>
                        The line is the 7-day rate: the week's clicks ÷ the week's visitors, so a quiet day with one
                        visitor can't swing it.
                    </li>
                    <li>The bands mark our targets: under 5% is weak, 12% and up is strong.</li>
                </ul>
                <p><span class="info-label">Source:</span> Umami, for both sides.</p>
            {/snippet}
            {#snippet chart()}
                <DailyChart
                    {days}
                    today={todayIso}
                    label="Daily visitor to click conversion"
                    formatValue={formatPercent}
                    rate={{
                        color: seriesColor,
                        daily: view(daily(inputs.visitorToClick)),
                        rolling: view(rolling.visitorToClick),
                        rollingLabel: '7-day rate',
                        bands: rateBands.visitorToClick,
                        bandsLabel: bandsLabel(rateBands.visitorToClick, formatPercent),
                    }}
                    tooltip={(i: number): TooltipRow[] => {
                        const p = inputs.visitorToClick[start + i]
                        return [
                            { label: 'Daily rate', value: cell(safeRatio(p.num, p.den), formatPercent), color: seriesColor, mark: 'dot' },
                            { label: 'Clicks / visitors', value: `${cell(p.num)} / ${cell(p.den)}` },
                            { label: '7-day rate', value: cell(rolling.visitorToClick[start + i], formatPercent), color: seriesColor, mark: 'line' },
                        ]
                    }}
                />
            {/snippet}
            {#snippet stats()}
                <StatsGrid stats={ratioStatsAll(inputs.visitorToClick)} formatValue={formatPercent} formatChange={formatPointChange} />
            {/snippet}
        </TrendRow>

        <!-- 3. Download clicks -->
        <TrendRow title="Download clicks" unavailable={umamiReason}>
            {#snippet info()}
                <p>Clicks on the website's download button (Umami's <code>download</code> event).</p>
                <ul>
                    <li>Across all countries, columns stack by country like the visitors chart.</li>
                    <li>It runs in the browser, so it misses Homebrew and direct links.</li>
                    <li>
                        It won't match server downloads: those also count Homebrew, direct links, GitHub-page traffic,
                        and bots the user-agent filter misses.
                    </li>
                </ul>
                <p><span class="info-label">Source:</span> Umami.</p>
            {/snippet}
            {#snippet chart()}
                <DailyChart
                    height={stackedChartHeight}
                    {days}
                    today={todayIso}
                    label="Daily download clicks"
                    formatValue={formatNumber}
                    series={clickStack ?? [{ key: 'clicks', label: 'Clicks', color: seriesColor, values: view(inputs.clicks.map((p) => p.value)) }]}
                    tooltip={(i: number): TooltipRow[] => {
                        const total = inputs.clicks[start + i].value
                        return clickStack
                            ? stackTooltip(clickStack, total, i, 'Download clicks')
                            : [{ label: 'Download clicks', value: cell(total), color: seriesColor }]
                    }}
                />
            {/snippet}
            {#snippet stats()}
                <StatsGrid stats={countStatsAll(inputs.clicks)} formatValue={formatAverage} formatChange={formatRelativeChange} />
            {/snippet}
        </TrendRow>

        <!-- Where the range's server downloads came from: three pies over the charts' window -->
        {@const breakdownReason =
            workerReason ??
            (country && !countries.breakdowns
                ? "Not available yet for one country: the live api-server doesn't split these by country until its update lands."
                : null)}
        <div class="grid grid-cols-1 gap-6 border-t border-border-subtle py-5 md:grid-cols-3">
            {#each breakdowns as b (b.title)}
                <div class="min-w-0 rounded-xl border border-border bg-surface p-4 sm:p-6">
                    <h3 class="mb-3 flex items-center gap-2 text-sm font-medium text-text-primary">
                        {b.title}
                        <InfoPopover topic={b.title}>{@render b.info()}</InfoPopover>
                    </h3>
                    {#if breakdownReason}
                        <p class="text-sm text-text-tertiary">{breakdownReason}</p>
                    {:else if b.slices.length === 0}
                        <p class="text-sm text-text-tertiary">{b.empty}</p>
                    {:else}
                        {#if b.headline}<p class="mb-3 text-xs text-text-secondary">{b.headline}</p>{/if}
                        <DonutChart slices={b.slices} label={b.title} unit="downloads" />
                    {/if}
                </div>
            {/each}
        </div>

        <!-- 4. Click → install conversion -->
        <TrendRow title="Click → install conversion" unavailable={bothReason}>
            {#snippet info()}
                {#if inputs.has.channels}
                    <p>Direct installs ÷ download clicks.</p>
                    <ul>
                        <li>Dots are each day's rate; the line is the pooled 7-day rate.</li>
                        <li>Homebrew installs stay out, because they never click the website's button.</li>
                        <li>
                            Installs from builds too old to report their channel ("unknown") count as direct, since
                            nearly all of those came from the DMG.
                        </li>
                        <li>An install can first launch days after its click, so read the trend.</li>
                        <li>Targets: under 50% is weak, 70% and up is strong.</li>
                    </ul>
                {:else}
                    <p>New installs ÷ download clicks.</p>
                    <ul>
                        <li>Dots are each day's rate; the line is the pooled 7-day rate.</li>
                        <li>
                            It can pass 100%: Homebrew and direct-link installs never click the button, and an install
                            can first launch days after its download.
                        </li>
                        <li>So read the trend more than the level.</li>
                    </ul>
                {/if}
                <p><span class="info-label">Source:</span> clicks from Umami, installs from the app's telemetry.</p>
            {/snippet}
            {#snippet chart()}
                <DailyChart
                    {days}
                    today={todayIso}
                    label="Daily click to install conversion"
                    formatValue={formatPercent}
                    rate={{
                        color: seriesColor,
                        daily: view(daily(inputs.clickToInstall)),
                        rolling: view(rolling.clickToInstall),
                        rollingLabel: '7-day rate',
                        bands: rateBands.clickToInstall,
                        bandsLabel: bandsLabel(rateBands.clickToInstall, formatPercent),
                    }}
                    tooltip={(i: number): TooltipRow[] => {
                        const p = inputs.clickToInstall[start + i]
                        const direct = rows[start + i].newInstallsByChannel !== null
                        return [
                            { label: 'Daily rate', value: cell(safeRatio(p.num, p.den), formatPercent), color: seriesColor, mark: 'dot' },
                            { label: direct ? 'Non-Homebrew installs / clicks' : 'Installs / clicks', value: `${cell(p.num)} / ${cell(p.den)}` },
                            { label: '7-day rate', value: cell(rolling.clickToInstall[start + i], formatPercent), color: seriesColor, mark: 'line' },
                        ]
                    }}
                />
            {/snippet}
            {#snippet stats()}
                <StatsGrid stats={ratioStatsAll(inputs.clickToInstall)} formatValue={formatPercent} formatChange={formatPointChange} />
            {/snippet}
        </TrendRow>

        <!-- 5. New installs: by channel (left half) and by country within each channel (right half) -->
        <TrendRow title="New installs" unavailable={workerReason}>
            {#snippet info()}
                <p>Installs whose first-ever heartbeat landed that day: people who actually launched the app.</p>
                <ul>
                    <li>
                        The channel is what the app reported on that first beat: Homebrew, direct (the DMG), or unknown
                        (an older build, or a value the server doesn't know).
                    </li>
                    <li>
                        Across all countries, each day has two halves of the same height. The left stacks by channel, the
                        right by country within each channel, so the channel boundaries line up.
                    </li>
                    <li>A country's Homebrew segment is a lighter tint of its color, and its unknown-channel segment is striped.</li>
                    <li>
                        Installs count toward a country only when their first heartbeat carried one, so "Other" also holds
                        the installs with no country.
                    </li>
                    <li>It misses installs that opted out of analytics, and debug builds.</li>
                </ul>
                <p><span class="info-label">Source:</span> the app's own telemetry, via the api-server.</p>
            {/snippet}
            {#snippet chart()}
                {@const channelSplit = view(inputs.installsByChannel)}
                {@const right = installRight ? splitSeries(installRight) : undefined}
                <DailyChart
                    height={stackedChartHeight}
                    {days}
                    today={todayIso}
                    label="Daily new installs by channel{right ? ' and by country' : ''}"
                    formatValue={formatNumber}
                    series={inputs.has.channels
                        ? [
                              { key: 'direct', label: 'Direct', color: country ? seriesColor : COLOR_GOLD, values: channelSplit.map((c) => c?.direct ?? null) },
                              { key: 'homebrew', label: 'Homebrew', color: COLOR_HOMEBREW, values: channelSplit.map((c) => c?.homebrew ?? null) },
                              { key: 'unknown', label: 'Unknown', color: COLOR_UNKNOWN, values: channelSplit.map((c) => c?.unknown ?? null) },
                          ]
                        : [{ key: 'installs', label: 'New installs', color: seriesColor, values: view(inputs.installs.map((p) => p.value)) }]}
                    splitSeries={right}
                    legend={right
                        ? [
                              { label: 'Direct', color: COLOR_GOLD },
                              { label: 'Homebrew', color: COLOR_HOMEBREW },
                              { label: 'Unknown', color: COLOR_UNKNOWN },
                              ...countries.top.map((code) => ({ label: countryLabel(code), color: countryColor(code) })),
                              { label: 'Other', color: COLOR_OTHER_COUNTRIES },
                          ]
                        : undefined}
                    tooltip={(i: number): TooltipRow[] => {
                        const r = rows[start + i]
                        const c = r.newInstallsByChannel
                        const byCountry = installRight
                            ? breakdownRows(installCountryEntries(installRight, [...countries.top, otherCountries], i), r.newInstalls)
                            : []
                        // The channels keep the stack's order (bottom first), so they read like the left half.
                        const byChannel = c
                            ? breakdownRows(
                                  [
                                      { label: 'Direct', count: c.direct, color: country ? seriesColor : COLOR_GOLD },
                                      { label: 'Homebrew', count: c.homebrew, color: COLOR_HOMEBREW },
                                      { label: 'Unknown', count: c.unknown, color: COLOR_UNKNOWN },
                                  ],
                                  r.newInstalls,
                                  { sort: false },
                              )
                            : []
                        return [
                            { label: 'New installs', value: cell(r.newInstalls) },
                            ...byChannel,
                            ...(byCountry.length > 0 ? [{ label: 'By country', value: '', heading: true }, ...byCountry] : []),
                        ]
                    }}
                />
                {#if right}
                    <p class="mt-1 pl-11 text-xs text-text-tertiary">
                        Left half: by channel. Right half: by country within each channel, lighter for Homebrew and
                        striped for unknown.
                    </p>
                {:else if !inputs.has.channels}
                    <p class="mt-1 pl-11 text-xs text-text-tertiary">The split by channel isn't available yet.</p>
                {:else if !country && !countries.worker}
                    <p class="mt-1 pl-11 text-xs text-text-tertiary">The split by country isn't available yet.</p>
                {/if}
            {/snippet}
            {#snippet stats()}
                <StatsGrid stats={countStatsAll(inputs.installs)} formatValue={formatAverage} formatChange={formatRelativeChange} />
            {/snippet}
        </TrendRow>

        <!-- 6. D7 retention -->
        <TrendRow title="D7 retention" unavailable={workerReason}>
            {#snippet info()}
                <p>The share of a day's new installs that sent any heartbeat on their 7th day after install.</p>
                <ul>
                    <li>
                        A cohort needs to be at least eight days old to have a D7, so the newest week shows grey "so far"
                        points instead: retained as of day N, which can only grow.
                    </li>
                    <li>The line pools the last seven final cohorts.</li>
                    <li>
                        Stats pool cohorts too (all retained ÷ all installs) and count back from the newest final cohort,
                        so "7d" is the seven newest cohorts that have a D7.
                    </li>
                    <li>Targets: under 15% is weak, 25% and up is strong.</li>
                </ul>
                <p><span class="info-label">Source:</span> the api-server, which owns the definition.</p>
            {/snippet}
            {#snippet chart()}
                <DailyChart
                    {days}
                    today={todayIso}
                    label="D7 retention by install day"
                    formatValue={formatPercent}
                    rate={{
                        color: seriesColor,
                        daily: view(daily(inputs.d7)),
                        rolling: view(rolling.d7),
                        rollingLabel: '7-cohort rate',
                        tentative: inputs.has.retentionSoFar ? view(inputs.d7Tentative) : undefined,
                        bands: rateBands.d7,
                        bandsLabel: bandsLabel(rateBands.d7, formatPercent),
                    }}
                    tooltip={(i: number): TooltipRow[] => retentionTooltip('D7', inputs.d7, inputs.d7Tentative, rolling.d7, (r) => r.retentionSoFar, i)}
                />
                {#if !inputs.has.retentionSoFar}
                    <p class="mt-1 pl-11 text-xs text-text-tertiary">"So far" points for young cohorts aren't available yet.</p>
                {/if}
            {/snippet}
            {#snippet stats()}
                <StatsGrid stats={ratioStatsAll(inputs.d7, d7End(todayIso))} formatValue={formatPercent} formatChange={formatPointChange} />
            {/snippet}
        </TrendRow>

        <!-- 7. D28 retention -->
        <TrendRow title="D28 retention" unavailable={workerReason ?? (inputs.has.d28 ? null : 'Not available yet: the live api-server sends D28 once its update lands.')}>
            {#snippet info()}
                <p>The share of a day's new installs that sent any heartbeat on their 28th day after install.</p>
                <ul>
                    <li>
                        A cohort needs to be at least 29 days old to have a D28, so cohorts 8–28 days old show grey "so
                        far" points: retained as of day N, which can only grow.
                    </li>
                    <li>The line pools the last seven final cohorts, and the stats count back from the newest final cohort, as for D7.</li>
                    <li>Targets: under 10% is weak, 20% and up is strong.</li>
                </ul>
                <p><span class="info-label">Source:</span> the api-server, which owns the definition.</p>
            {/snippet}
            {#snippet chart()}
                <DailyChart
                    {days}
                    today={todayIso}
                    label="D28 retention by install day"
                    formatValue={formatPercent}
                    rate={{
                        color: seriesColor,
                        daily: view(daily(inputs.d28)),
                        rolling: view(rolling.d28),
                        rollingLabel: '7-cohort rate',
                        tentative: inputs.has.d28SoFar ? view(inputs.d28Tentative) : undefined,
                        bands: rateBands.d28,
                        bandsLabel: bandsLabel(rateBands.d28, formatPercent),
                    }}
                    tooltip={(i: number): TooltipRow[] => retentionTooltip('D28', inputs.d28, inputs.d28Tentative, rolling.d28, (r) => r.d28SoFar, i)}
                />
            {/snippet}
            {#snippet stats()}
                <StatsGrid stats={ratioStatsAll(inputs.d28, d28End(todayIso))} formatValue={formatPercent} formatChange={formatPointChange} />
            {/snippet}
        </TrendRow>

        <!-- 8. Bot meter -->
        <TrendRow title="Bot meter" unavailable={workerReason}>
            {#snippet info()}
                <p>Server downloads ÷ new installs.</p>
                <ul>
                    <li>
                        Most DMG requests are mirrors, scrapers, and bots the user-agent filter doesn't catch, so this sits
                        well above 1×.
                    </li>
                    <li>A sudden spike means a scraper changed.</li>
                    <li>A slow drift down is good news: fewer bots, or more people actually launching the app.</li>
                </ul>
                <p><span class="info-label">Source:</span> the api-server's download log and the app's telemetry.</p>
            {/snippet}
            {#snippet chart()}
                <DailyChart
                    {days}
                    today={todayIso}
                    label="Daily server downloads per new install"
                    formatValue={formatMultiple}
                    rate={{
                        color: seriesColor,
                        daily: view(daily(inputs.botMeter)),
                        rolling: view(rolling.botMeter),
                        rollingLabel: '7-day ratio',
                    }}
                    tooltip={(i: number): TooltipRow[] => {
                        const p = inputs.botMeter[start + i]
                        return [
                            { label: 'Daily ratio', value: cell(safeRatio(p.num, p.den), formatMultiple), color: seriesColor, mark: 'dot' },
                            { label: 'Downloads / installs', value: `${cell(p.num)} / ${cell(p.den)}` },
                            { label: '7-day ratio', value: cell(rolling.botMeter[start + i], formatMultiple), color: seriesColor, mark: 'line' },
                        ]
                    }}
                />
            {/snippet}
            {#snippet stats()}
                <StatsGrid
                    stats={ratioStatsAll(inputs.botMeter)}
                    formatValue={formatMultiple}
                    formatChange={formatMultipleChange}
                    upIsGood={false}
                />
            {/snippet}
        </TrendRow>
    {:else}
        <ErrorState error={funnel.error} {selection} />
    {/if}
</section>
