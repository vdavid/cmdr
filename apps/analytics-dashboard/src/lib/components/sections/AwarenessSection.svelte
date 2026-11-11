<!-- Acquisition § Awareness: top-of-funnel reach across the three sites, a referrer pie per site, and GitHub stars. -->
<script lang="ts">
    import type { SourceResult, DashboardSelection } from '#lib/server/types.js'
    import type { UmamiSiteUrls } from '#lib/server/fetch-all.js'
    import type { UmamiData } from '#lib/server/sources/umami.js'
    import type { GitHubStarsData } from '#lib/server/sources/github.js'
    import { formatNumber, formatDelta } from '#lib/format.js'
    import { COLOR_GOLD, COLOR_PURPLE, COLOR_GREEN, COLOR_CYAN } from '#lib/colors.js'
    import Chart from '#lib/components/Chart.svelte'
    import ErrorState from '#lib/components/ErrorState.svelte'
    import MetricRow from '#lib/components/MetricRow.svelte'
    import DonutChart from '#lib/components/DonutChart.svelte'
    import { referrerSlices } from '#lib/referrer-slices.js'
    import SectionDescription from '#lib/components/SectionDescription.svelte'
    import ExternalLinks from '#lib/components/ExternalLinks.svelte'

    const {
        umami,
        umamiSiteUrls,
        githubStars,
        selection,
        starChartXMin,
    }: {
        umami: SourceResult<UmamiData>
        umamiSiteUrls: UmamiSiteUrls
        githubStars: SourceResult<GitHubStarsData>
        selection: DashboardSelection
        starChartXMin: number
    } = $props()

    const repoColors: Record<string, string> = { 'getcmdr/cmdr': COLOR_GOLD, 'vdavid/mtp-rs': COLOR_PURPLE }

    // A site with no configured website ID drops out rather than linking to a broken dashboard.
    const umamiLinks = $derived(
        [
            { label: 'View getcmdr.com in Umami', href: umamiSiteUrls.website },
            { label: 'View veszelovszki.com in Umami', href: umamiSiteUrls.personalSite },
            { label: 'View getprvw.com in Umami', href: umamiSiteUrls.prvw },
        ].filter((l): l is { label: string; href: string } => l.href !== null),
    )
</script>

<section class="rounded-xl border border-border bg-surface p-6">
    <div class="mb-1">
        <h2 class="text-lg font-semibold text-text-primary">Awareness</h2>
        <p class="text-sm text-text-tertiary">How many people see Cmdr content?</p>
    </div>
    <SectionDescription
        insight="Use this to gauge top-of-funnel reach across the three sites, and which days or sites are growing."
        caveat={'Umami is cookieless and proxied, so it dodges most adblockers, but it still undercounts by a few percent, ' +
            'and one person on two devices counts twice.'}
    />

    {#if umami.ok}
        {@const u = umami.data}
        {@const totalPageviews = u.personalSite.pageviews.value + u.website.pageviews.value + u.prvw.pageviews.value}
        {@const prevPageviews = u.personalSite.pageviews.prev + u.website.pageviews.prev + u.prvw.pageviews.prev}
        {@const delta = formatDelta(totalPageviews, prevPageviews)}

        {@const sites = [
            { name: 'getcmdr.com', color: COLOR_GOLD, stats: u.website, referrers: u.websiteReferrers },
            { name: 'getprvw.com', color: COLOR_CYAN, stats: u.prvw, referrers: u.prvwReferrers },
            { name: 'veszelovszki.com', color: COLOR_GREEN, stats: u.personalSite, referrers: u.personalSiteReferrers },
        ]}

        <MetricRow metrics={[{ label: 'Total page views', value: formatNumber(totalPageviews), delta }]} />

        <!-- Per site: its views up top, then where its visitors came from. DETAILS.md § "Awareness referrer pies" -->
        <div class="mt-4 grid grid-cols-1 gap-6 md:grid-cols-3">
            {#each sites as site (site.name)}
                {@const slices = referrerSlices(site.referrers)}
                {@const referred = slices.reduce((sum, s) => sum + s.count, 0)}
                {@const views = site.stats.pageviews}
                <div class="min-w-0 rounded-xl border border-border bg-surface p-4 sm:p-6">
                    <div class="mb-3 flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
                        <h3 class="flex items-center gap-2 text-sm font-medium text-text-primary">
                            <span class="inline-block size-2 rounded-full" style="background: {site.color}"></span>
                            {site.name}
                        </h3>
                        <p class="flex flex-wrap items-baseline gap-x-1.5">
                            <span class="text-2xl font-bold text-text-primary tabular-nums">{formatNumber(views.value)}</span>
                            <span class="text-xs text-text-tertiary">views</span>
                            {#if views.prev > 0}
                                {@const d = formatDelta(views.value, views.prev)}
                                <span class="text-xs tabular-nums {d.positive ? 'text-success' : 'text-danger'}">{d.text}</span>
                            {/if}
                        </p>
                    </div>
                    {#if slices.length === 0}
                        <p class="text-sm text-text-tertiary">No visitors came from another site in this window.</p>
                    {:else}
                        <p class="mb-3 text-xs text-text-secondary">
                            Top referrers: {formatNumber(referred)} of {formatNumber(site.stats.visitors.value)} visitors came
                            from another site.
                        </p>
                        <DonutChart slices={slices} label="Referrers to {site.name}" unit="visitors" />
                    {/if}
                </div>
            {/each}
        </div>

        {#if githubStars.ok}
            {@const stars = githubStars.data}
            <div class="mt-4">
                <h3 class="mb-2 text-sm font-medium text-text-secondary">GitHub stars</h3>
                <MetricRow
                    metrics={stars.repos.map((r) => ({
                        label: r.repo,
                        value: formatNumber(r.totalStars),
                        color: repoColors[r.repo],
                    }))}
                />
                {#each stars.repos as repo (repo.repo)}
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
                                xMin={starChartXMin}
                                xMax={Date.now() / 1000}
                            />
                        </div>
                    {/if}
                {/each}
            </div>
        {/if}

        <ExternalLinks
            links={[
                ...umamiLinks,
                { label: 'cmdr on GitHub', href: 'https://github.com/getcmdr/cmdr' },
                { label: 'mtp-rs on GitHub', href: 'https://github.com/vdavid/mtp-rs' },
            ]}
        />
    {:else}
        <ErrorState error={umami.error} {selection} />
    {/if}
</section>
