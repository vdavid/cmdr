<!-- Product § Retention: the split of paid subscriptions by status, plus a derived churn rate. -->
<script lang="ts">
    import type { SourceResult, DashboardSelection } from '$lib/server/types.js'
    import type { PaddleData } from '$lib/server/sources/paddle.js'
    import { formatNumber } from '$lib/format.js'
    import ErrorState from '$lib/components/ErrorState.svelte'
    import MetricRow from '$lib/components/MetricRow.svelte'
    import MetricTable from '$lib/components/MetricTable.svelte'
    import EmptyState from '$lib/components/EmptyState.svelte'
    import SectionDescription from '$lib/components/SectionDescription.svelte'
    import ExternalLinks from '$lib/components/ExternalLinks.svelte'

    let { paddle, selection }: { paddle: SourceResult<PaddleData>; selection: DashboardSelection } = $props()
</script>

<section class="rounded-xl border border-border bg-surface p-6">
    <div class="mb-1">
        <h2 class="text-lg font-semibold text-text-primary">Retention</h2>
        <p class="text-sm text-text-tertiary">Do they stay?</p>
    </div>
    <SectionDescription
        insight={'Use this to see whether paying customers stick around: the split of subscriptions by status (active, ' +
            'canceled, and so on).'}
        caveat={"From Paddle, so it's reliable for money. It covers paid subscriptions only, not free-tier engagement (that's " +
            "the Active use section and the funnel's D7)."}
    />

    {#if !paddle.ok}
        <ErrorState error={paddle.error} {selection} />
    {:else}
        {@const p = paddle.data}
        {@const statusEntries = Object.entries(p.subscriptionsByStatus)}
        {@const totalSubs = statusEntries.reduce((sum, entry) => sum + entry[1], 0)}
        {@const activeSubs = p.subscriptionsByStatus['active'] ?? 0}
        {@const canceledSubs = p.subscriptionsByStatus['canceled'] ?? 0}
        {@const churnDisplay = totalSubs > 0 ? `${((canceledSubs / totalSubs) * 100).toFixed(1)}%` : 'N/A'}

        <MetricRow
            metrics={[
                { label: 'Active subscriptions', value: formatNumber(activeSubs) },
                { label: 'Churn rate', value: churnDisplay },
            ]}
        />

        {#if statusEntries.length > 0}
            <div class="mt-4">
                <h3 class="mb-2 text-sm font-medium text-text-secondary">Subscriptions by status</h3>
                <MetricTable
                    items={statusEntries.map((entry) => ({ x: entry[0], y: entry[1] }))}
                    colLabel="Status"
                    colValue="Count"
                />
            </div>
        {:else}
            <EmptyState />
        {/if}

        <ExternalLinks links={[{ label: 'View in Paddle', href: 'https://vendors.paddle.com' }]} />
    {/if}
</section>
