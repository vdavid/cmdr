<!-- Acquisition § Awareness: top-of-funnel reach across the three sites, top referrers, and GitHub stars. -->
<script lang="ts">
    import type { SourceResult, DashboardSelection } from '$lib/server/types.js'
    import type { UmamiData } from '$lib/server/sources/umami.js'
    import type { GitHubStarsData } from '$lib/server/sources/github.js'
    import { formatNumber, formatDelta } from '$lib/format.js'
    import { COLOR_GOLD, COLOR_PURPLE, COLOR_GREEN, COLOR_CYAN } from '$lib/colors.js'
    import Chart from '$lib/components/Chart.svelte'
    import ErrorState from '$lib/components/ErrorState.svelte'
    import MetricRow from '$lib/components/MetricRow.svelte'
    import MetricTable from '$lib/components/MetricTable.svelte'
    import SectionDescription from '$lib/components/SectionDescription.svelte'
    import ExternalLinks from '$lib/components/ExternalLinks.svelte'

    let {
        umami,
        githubStars,
        selection,
        starChartXMin,
    }: {
        umami: SourceResult<UmamiData>
        githubStars: SourceResult<GitHubStarsData>
        selection: DashboardSelection
        starChartXMin: number
    } = $props()

    const repoColors: Record<string, string> = { 'vdavid/cmdr': COLOR_GOLD, 'vdavid/mtp-rs': COLOR_PURPLE }
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

    {#if !umami.ok}
        <ErrorState error={umami.error} {selection} />
    {:else}
        {@const u = umami.data}
        {@const totalPageviews = u.personalSite.pageviews.value + u.website.pageviews.value + u.prvw.pageviews.value}
        {@const prevPageviews = u.personalSite.pageviews.prev + u.website.pageviews.prev + u.prvw.pageviews.prev}
        {@const delta = formatDelta(totalPageviews, prevPageviews)}

        <MetricRow
            metrics={[
                { label: 'Total page views', value: formatNumber(totalPageviews), delta },
                { label: 'veszelovszki.com views', value: formatNumber(u.personalSite.pageviews.value), color: COLOR_GREEN },
                { label: 'getcmdr.com views', value: formatNumber(u.website.pageviews.value), color: COLOR_GOLD },
                { label: 'getprvw.com views', value: formatNumber(u.prvw.pageviews.value), color: COLOR_CYAN },
            ]}
        />

        {#if u.websiteReferrers.length > 0}
            <div class="mt-4">
                <h3 class="mb-2 text-sm font-medium text-text-secondary">Top referrers (getcmdr.com)</h3>
                <MetricTable items={u.websiteReferrers.slice(0, 10)} colLabel="Source" colValue="Views" />
            </div>
        {/if}

        {#if u.prvwReferrers.length > 0}
            <div class="mt-4">
                <h3 class="mb-2 text-sm font-medium text-text-secondary">Top referrers (getprvw.com)</h3>
                <MetricTable items={u.prvwReferrers.slice(0, 10)} colLabel="Source" colValue="Views" />
            </div>
        {/if}

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
                { label: 'View veszelovszki.com in Umami', href: 'https://anal.veszelovszki.com' },
                { label: 'View getcmdr.com in Umami', href: 'https://anal.veszelovszki.com' },
                { label: 'View getprvw.com in Umami', href: 'https://anal.veszelovszki.com' },
                { label: 'cmdr on GitHub', href: 'https://github.com/vdavid/cmdr' },
                { label: 'mtp-rs on GitHub', href: 'https://github.com/vdavid/mtp-rs' },
            ]}
        />
    {/if}
</section>
