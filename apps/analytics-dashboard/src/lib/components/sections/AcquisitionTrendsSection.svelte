<!--
  The top of the Acquisition page: one row per funnel metric, each a daily chart plus the shared 90/30/7
  stats grid, scoped by the country chips. Every stat comes from `#lib/acquisition-stats.ts` (today
  excluded, rates pooled as ratios of sums); this component only picks inputs and formats.
-->
<script lang="ts">
    import type { SourceResult, DashboardSelection } from '#lib/server/types.js'
    import type { FunnelData } from '#lib/server/sources/funnel.js'
    import type { FunnelCountriesData } from '#lib/server/sources/funnel-countries.js'
    import type { TooltipRow } from '#lib/daily-chart.js'
    import {
        countStats,
        ratioStats,
        rollingRatio,
        statWindows,
        type CountPoint,
        type RatioPoint,
        type WindowStats,
    } from '#lib/acquisition-stats.js'
    import { buildTrendInputs, d7End } from '#lib/acquisition-series.js'
    import { chipCountries, safeRatio } from '#lib/acquisition-countries.js'
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
    import DailyChart from '../DailyChart.svelte'
    import StatsGrid from '../StatsGrid.svelte'
    import TrendRow from '../TrendRow.svelte'
    import InfoPopover from '../InfoPopover.svelte'
    import ErrorState from '../ErrorState.svelte'

    interface Props {
        funnel: SourceResult<FunnelData>
        funnelCountries: SourceResult<FunnelCountriesData>
        country: string | null
        selection: DashboardSelection
        todayIso: string
        /** Builds this page's URL with `country` set (or cleared with `null`), keeping the other params. */
        countryHref: (country: string | null) => string
    }

    const { funnel, funnelCountries, country, selection, todayIso, countryHref }: Props = $props()

    const chipCount = 8
    const rollingDays = 7

    /** How many complete days the charts show; today (partial) is drawn after them, faded. */
    let viewDays = $state<30 | 90>(30)

    const chips = $derived.by(() => {
        const top = funnelCountries.ok ? chipCountries(funnelCountries.data.rows, chipCount) : []
        return country && !top.includes(country) ? [...top, country] : top
    })

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

    const rows = $derived(funnel.ok ? funnel.data.rows : [])
    const inputs = $derived(buildTrendInputs(rows, todayIso))
    /** The first row the charts show: `viewDays` complete days, plus today when it's there. */
    const start = $derived(Math.max(0, rows.length - (viewDays + (rows.at(-1)?.date === todayIso ? 1 : 0))))
    const days = $derived(inputs.days.slice(start))
    const rolling = $derived({
        visitorToClick: rollingComplete(inputs.visitorToClick),
        clickToInstall: rollingComplete(inputs.clickToInstall),
        botMeter: rollingComplete(inputs.botMeter),
        // Non-final cohorts carry no numerator, so the D7 line only pools final cohorts.
        d7: rollingRatio(inputs.d7, rollingDays),
    })

    /** The visible slice of a full-history series. */
    function view<T>(values: T[]): T[] {
        return values.slice(start)
    }

    function cell(value: number | null, format: (n: number) => string = formatNumber): string {
        return value === null ? '–' : format(value)
    }
</script>

<section class="mb-6 rounded-xl border border-border bg-surface p-4 sm:p-6">
    <div class="mb-4 flex flex-wrap items-center justify-between gap-3">
        <h2 class="flex items-center gap-2 text-lg font-semibold text-text-primary">
            Acquisition trends
            <InfoPopover topic="reading these charts">
                Every day is a UTC day. Today is still going, so it's drawn faded and left out of every stat. Columns
                carry their exact number where it fits; hover or focus a chart (then use the arrow keys) for each
                day's exact values. Each stats panel covers the last 90, 30, and 7 complete days, and "vs. previous"
                compares with the same-length window before it. Rates pool as a ratio of sums (all clicks ÷ all
                visitors), never an average of daily rates; the median is the one per-day rate stat. A dash means we
                couldn't get that number, never a zero.
            </InfoPopover>
        </h2>
        <div class="flex rounded-lg border border-border p-0.5 text-xs" role="group" aria-label="Chart window">
            {#each [30, 90] as const as option (option)}
                <button
                    type="button"
                    class="rounded-md px-2.5 py-1 {viewDays === option
                        ? 'bg-accent text-accent-contrast'
                        : 'text-text-secondary hover:text-text-primary'}"
                    aria-pressed={viewDays === option}
                    onclick={() => (viewDays = option)}
                >
                    {option} days
                </button>
            {/each}
        </div>
    </div>

    <!-- Country filter: re-scopes every chart and stat below -->
    <nav class="mb-2 flex flex-wrap gap-1.5" aria-label="Filter by country">
        <!-- eslint-disable svelte/no-navigation-without-resolve -- `countryHref` builds every href with resolve('/'); the rule can't follow it through the prop -->
        <a
            href={countryHref(null)}
            class="rounded-full border px-3 py-1 text-xs {country === null
                ? 'border-accent bg-accent/15 text-text-primary'
                : 'border-border text-text-secondary hover:text-text-primary'}"
            aria-current={country === null ? 'page' : undefined}>All</a
        >
        {#each chips as code (code)}
            <a
                href={countryHref(code)}
                class="rounded-full border px-3 py-1 text-xs {country === code
                    ? 'border-accent bg-accent/15 text-text-primary'
                    : 'border-border text-text-secondary hover:text-text-primary'}"
                aria-current={country === code ? 'page' : undefined}>{formatCountry(code)}</a
            >
        {/each}
        <!-- eslint-enable svelte/no-navigation-without-resolve -->
    </nav>
    {#if country}
        <p class="mb-2 text-xs text-text-tertiary">
            Showing {formatCountry(country)} only. Small numbers swing a lot day to day, so lean on the 30d and 90d
            columns.
        </p>
    {/if}

    {#if funnel.ok}
        {@const umamiDown = rows.every((r) => r.visitors === null)}
        {@const workerReason =
            funnel.data.workerStatus === 'country-unsupported'
                ? "Not available yet for one country: the live api-server doesn't filter by country until its update lands."
                : funnel.data.workerStatus === 'unavailable'
                  ? "Couldn't load this from the api-server."
                  : null}
        {@const umamiReason = umamiDown ? "Couldn't load this from Umami." : null}
        {@const bothReason = umamiReason ?? workerReason}

        <!-- 1. Visitors -->
        {@const visitorStats = countStatsAll(inputs.visitors)}
        {@const ppv = ratioStatsAll(inputs.pageviewsPerVisitor)}
        <TrendRow title="Visitors" unavailable={umamiReason}>
            {#snippet info()}
                Unique visitors to getcmdr.com per UTC day, from Umami (cookieless, in the browser). Ad blockers hide some
                visitors, and one person on two devices counts twice. Pageviews per visitor pools all pageviews ÷ all
                visitors in the window.
            {/snippet}
            {#snippet chart()}
                <DailyChart
                    {days}
                    today={todayIso}
                    label="Daily visitors"
                    formatValue={formatNumber}
                    series={[{ key: 'visitors', label: 'Visitors', color: COLOR_GOLD, values: view(inputs.visitors.map((p) => p.value)) }]}
                    tooltip={(i: number): TooltipRow[] => {
                        const r = rows[start + i]
                        return [
                            { label: 'Visitors', value: cell(r.visitors), color: COLOR_GOLD },
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
                Download-button clicks ÷ visitors. Dots are each day's rate; the line is the 7-day rate, pooled as the
                week's clicks ÷ the week's visitors, so a quiet day with one visitor can't swing it. Both sides come from
                Umami.
            {/snippet}
            {#snippet chart()}
                <DailyChart
                    {days}
                    today={todayIso}
                    label="Daily visitor to click conversion"
                    formatValue={formatPercent}
                    rate={{
                        color: COLOR_GOLD,
                        daily: view(daily(inputs.visitorToClick)),
                        rolling: view(rolling.visitorToClick),
                        rollingLabel: '7-day rate',
                    }}
                    tooltip={(i: number): TooltipRow[] => {
                        const p = inputs.visitorToClick[start + i]
                        return [
                            { label: 'Daily rate', value: cell(safeRatio(p.num, p.den), formatPercent), color: COLOR_GOLD, mark: 'dot' },
                            { label: 'Clicks / visitors', value: `${cell(p.num)} / ${cell(p.den)}` },
                            { label: '7-day rate', value: cell(rolling.visitorToClick[start + i], formatPercent), color: COLOR_GOLD, mark: 'line' },
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
                The website's download button firing Umami's <code>download</code> event. It's in the browser, so it misses
                Homebrew and direct links, and it won't match server downloads: those also count Homebrew, direct links,
                GitHub-page traffic, and bots the user-agent filter misses.
            {/snippet}
            {#snippet chart()}
                <DailyChart
                    {days}
                    today={todayIso}
                    label="Daily download clicks"
                    formatValue={formatNumber}
                    series={[{ key: 'clicks', label: 'Clicks', color: COLOR_GOLD, values: view(inputs.clicks.map((p) => p.value)) }]}
                    tooltip={(i: number): TooltipRow[] => [{ label: 'Download clicks', value: cell(inputs.clicks[start + i].value), color: COLOR_GOLD }]}
                />
            {/snippet}
            {#snippet stats()}
                <StatsGrid stats={countStatsAll(inputs.clicks)} formatValue={formatAverage} formatChange={formatRelativeChange} />
            {/snippet}
        </TrendRow>

        <!-- 4. Click → install conversion -->
        <TrendRow title="Click → install conversion" unavailable={bothReason}>
            {#snippet info()}
                New installs ÷ download clicks, daily dots plus the pooled 7-day rate. It can pass 100%: Homebrew and
                direct-link installs never click the button, and an install can first launch days after its download. So
                read the trend, not the level.
            {/snippet}
            {#snippet chart()}
                <DailyChart
                    {days}
                    today={todayIso}
                    label="Daily click to install conversion"
                    formatValue={formatPercent}
                    rate={{
                        color: COLOR_GOLD,
                        daily: view(daily(inputs.clickToInstall)),
                        rolling: view(rolling.clickToInstall),
                        rollingLabel: '7-day rate',
                    }}
                    tooltip={(i: number): TooltipRow[] => {
                        const p = inputs.clickToInstall[start + i]
                        return [
                            { label: 'Daily rate', value: cell(safeRatio(p.num, p.den), formatPercent), color: COLOR_GOLD, mark: 'dot' },
                            { label: 'Installs / clicks', value: `${cell(p.num)} / ${cell(p.den)}` },
                            { label: '7-day rate', value: cell(rolling.clickToInstall[start + i], formatPercent), color: COLOR_GOLD, mark: 'line' },
                        ]
                    }}
                />
            {/snippet}
            {#snippet stats()}
                <StatsGrid stats={ratioStatsAll(inputs.clickToInstall)} formatValue={formatPercent} formatChange={formatPointChange} />
            {/snippet}
        </TrendRow>

        <!-- 5. New installs, by channel -->
        <TrendRow title="New installs" unavailable={workerReason}>
            {#snippet info()}
                Installs whose first-ever heartbeat landed that day: people who actually launched the app (the app's own
                telemetry). It misses installs that opted out of analytics, and debug builds. The channel is what the app
                reported on that first beat: Homebrew, direct (the DMG), or unknown (an older build, or a value the
                server doesn't know).
            {/snippet}
            {#snippet chart()}
                {@const channels = view(inputs.installsByChannel)}
                <DailyChart
                    {days}
                    today={todayIso}
                    label="Daily new installs by channel"
                    formatValue={formatNumber}
                    series={inputs.has.channels
                        ? [
                              { key: 'direct', label: 'Direct', color: COLOR_GOLD, values: channels.map((c) => c?.direct ?? null) },
                              { key: 'homebrew', label: 'Homebrew', color: COLOR_HOMEBREW, values: channels.map((c) => c?.homebrew ?? null) },
                              { key: 'unknown', label: 'Unknown', color: COLOR_UNKNOWN, values: channels.map((c) => c?.unknown ?? null) },
                          ]
                        : [{ key: 'installs', label: 'New installs', color: COLOR_GOLD, values: view(inputs.installs.map((p) => p.value)) }]}
                    tooltip={(i: number): TooltipRow[] => {
                        const r = rows[start + i]
                        const c = r.newInstallsByChannel
                        return [
                            { label: 'New installs', value: cell(r.newInstalls) },
                            ...(c
                                ? [
                                      { label: 'Direct', value: formatNumber(c.direct), color: COLOR_GOLD },
                                      { label: 'Homebrew', value: formatNumber(c.homebrew), color: COLOR_HOMEBREW },
                                      { label: 'Unknown', value: formatNumber(c.unknown), color: COLOR_UNKNOWN },
                                  ]
                                : []),
                        ]
                    }}
                />
                {#if !inputs.has.channels}
                    <p class="mt-1 pl-11 text-xs text-text-tertiary">The split by channel isn't available yet.</p>
                {/if}
            {/snippet}
            {#snippet stats()}
                <StatsGrid stats={countStatsAll(inputs.installs)} formatValue={formatAverage} formatChange={formatRelativeChange} />
            {/snippet}
        </TrendRow>

        <!-- 6. D7 retention -->
        <TrendRow title="D7 retention" unavailable={workerReason}>
            {#snippet info()}
                The share of a day's new installs that sent any heartbeat on their 7th day after install (the api-server
                owns the definition). A cohort needs to be at least eight days old to have a D7, so the newest week shows
                grey "so far" points instead: retained as of day N, which can only grow. The line pools the last seven
                final cohorts. Stats pool cohorts too (all retained ÷ all installs) and count back from the newest final
                cohort, so "7d" is the seven newest cohorts that have a D7.
            {/snippet}
            {#snippet chart()}
                <DailyChart
                    {days}
                    today={todayIso}
                    label="D7 retention by install day"
                    formatValue={formatPercent}
                    rate={{
                        color: COLOR_GOLD,
                        daily: view(daily(inputs.d7)),
                        rolling: view(rolling.d7),
                        rollingLabel: '7-cohort rate',
                        tentative: inputs.has.retentionSoFar ? view(inputs.d7Tentative) : undefined,
                    }}
                    tooltip={(i: number): TooltipRow[] => {
                        const r = rows[start + i]
                        const p = inputs.d7[start + i]
                        const t = inputs.d7Tentative[start + i]
                        if (p.num === null) {
                            return [
                                {
                                    label: t ? t.label : 'D7',
                                    value: t ? `${formatPercent(t.value)} (${cell(r.retentionSoFar?.retained ?? null)} of ${cell(r.newInstalls)})` : 'too young',
                                    color: '#8a8a93',
                                    mark: 'dot',
                                },
                            ]
                        }
                        return [
                            { label: 'D7', value: cell(safeRatio(p.num, p.den), formatPercent), color: COLOR_GOLD, mark: 'dot' },
                            { label: 'Retained / cohort', value: `${cell(p.num)} / ${cell(p.den)}` },
                            { label: '7-cohort rate', value: cell(rolling.d7[start + i], formatPercent), color: COLOR_GOLD, mark: 'line' },
                        ]
                    }}
                />
                {#if !inputs.has.retentionSoFar}
                    <p class="mt-1 pl-11 text-xs text-text-tertiary">"So far" points for young cohorts aren't available yet.</p>
                {/if}
            {/snippet}
            {#snippet stats()}
                <StatsGrid stats={ratioStatsAll(inputs.d7, d7End(todayIso))} formatValue={formatPercent} formatChange={formatPointChange} />
            {/snippet}
        </TrendRow>

        <!-- 7. Bot meter -->
        <TrendRow title="Bot meter" unavailable={workerReason}>
            {#snippet info()}
                Server downloads ÷ new installs. Most DMG requests are mirrors, scrapers, and bots the user-agent filter
                doesn't catch, so this sits well above 1×. A sudden spike means a scraper changed; a slow drift down is
                good news (fewer bots, or more people actually launching the app).
            {/snippet}
            {#snippet chart()}
                <DailyChart
                    {days}
                    today={todayIso}
                    label="Daily server downloads per new install"
                    formatValue={formatMultiple}
                    rate={{
                        color: COLOR_GOLD,
                        daily: view(daily(inputs.botMeter)),
                        rolling: view(rolling.botMeter),
                        rollingLabel: '7-day ratio',
                    }}
                    tooltip={(i: number): TooltipRow[] => {
                        const p = inputs.botMeter[start + i]
                        return [
                            { label: 'Daily ratio', value: cell(safeRatio(p.num, p.den), formatMultiple), color: COLOR_GOLD, mark: 'dot' },
                            { label: 'Downloads / installs', value: `${cell(p.num)} / ${cell(p.den)}` },
                            { label: '7-day ratio', value: cell(rolling.botMeter[start + i], formatMultiple), color: COLOR_GOLD, mark: 'line' },
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
