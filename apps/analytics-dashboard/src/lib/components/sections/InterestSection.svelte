<!-- Acquisition § Interest: getcmdr.com engagement (Umami + PostHog), download clicks, and top pages. -->
<script lang="ts">
    import type { SourceResult, DashboardSelection } from '$lib/server/types.js'
    import type { UmamiData } from '$lib/server/sources/umami.js'
    import type { PostHogData } from '$lib/server/sources/posthog.js'
    import { formatNumber, formatDelta, toChartData } from '$lib/format.js'
    import Chart from '$lib/components/Chart.svelte'
    import ErrorState from '$lib/components/ErrorState.svelte'
    import MetricRow from '$lib/components/MetricRow.svelte'
    import MetricTable from '$lib/components/MetricTable.svelte'
    import SectionDescription from '$lib/components/SectionDescription.svelte'
    import ExternalLinks from '$lib/components/ExternalLinks.svelte'

    let {
        umami,
        posthog,
        selection,
    }: {
        umami: SourceResult<UmamiData>
        posthog: SourceResult<PostHogData>
        selection: DashboardSelection
    } = $props()
</script>

<section class="rounded-xl border border-border bg-surface p-6">
    <div class="mb-1">
        <h2 class="text-lg font-semibold text-text-primary">Interest</h2>
        <p class="text-sm text-text-tertiary">How many engage with the product page?</p>
    </div>
    <SectionDescription
        insight="Use this to see getcmdr.com engagement over time, and to cross-check the two trackers against each other."
        caveat={'Umami is cookieless and undercounts a bit; PostHog needs its own client-side script to load, so adblockers ' +
            'trim it more. Treat the two as independent estimates, not one exact number.'}
    />

    {#if !umami.ok && !posthog.ok}
        <ErrorState
            error={[!umami.ok ? umami.error : '', !posthog.ok ? posthog.error : ''].filter(Boolean).join('; ')}
            {selection}
        />
    {:else}
        <div class="space-y-4">
            {#if umami.ok}
                {@const u = umami.data}
                {@const delta = formatDelta(u.website.pageviews.value, u.website.pageviews.prev)}

                <MetricRow
                    metrics={[
                        { label: 'getcmdr.com page views', value: formatNumber(u.website.pageviews.value), delta },
                        { label: 'Unique visitors', value: formatNumber(u.website.visitors.value) },
                    ]}
                />

                {#if posthog.ok && posthog.data.dailyPageviews.length > 0}
                    <Chart data={toChartData(posthog.data.dailyPageviews)} labels={['Page views']} height={180} />
                {/if}

                {#if u.downloadEvents.length > 0}
                    <div>
                        <h3 class="mb-2 text-sm font-medium text-text-secondary">Download button clicks</h3>
                        <MetricTable items={u.downloadEvents.slice(0, 10)} colLabel="Event" colValue="Count" />
                    </div>
                {/if}

                {#if u.websitePages.length > 0}
                    <div>
                        <h3 class="mb-2 text-sm font-medium text-text-secondary">Top pages</h3>
                        <MetricTable items={u.websitePages.slice(0, 10)} colLabel="Page" colValue="Views" />
                    </div>
                {/if}
            {:else if !umami.ok}
                <ErrorState error={umami.error} {selection} />
            {/if}
        </div>

        <ExternalLinks
            links={[
                { label: 'View in Umami', href: 'https://anal.veszelovszki.com' },
                { label: 'View in PostHog', href: 'https://eu.posthog.com/project/136072' },
            ]}
        />
    {/if}
</section>
