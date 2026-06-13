<!-- Product § Payment: completed Paddle purchases, revenue, and a recent-transactions table. -->
<script lang="ts">
    import type { SourceResult, DashboardSelection } from '$lib/server/types.js'
    import type { PaddleData } from '$lib/server/sources/paddle.js'
    import { formatNumber, formatCurrency } from '$lib/format.js'
    import ErrorState from '$lib/components/ErrorState.svelte'
    import MetricRow from '$lib/components/MetricRow.svelte'
    import EmptyState from '$lib/components/EmptyState.svelte'
    import SectionDescription from '$lib/components/SectionDescription.svelte'
    import ExternalLinks from '$lib/components/ExternalLinks.svelte'

    let { paddle, selection }: { paddle: SourceResult<PaddleData>; selection: DashboardSelection } = $props()
</script>

<section class="rounded-xl border border-border bg-surface p-6">
    <div class="mb-1">
        <h2 class="text-lg font-semibold text-text-primary">Payment</h2>
        <p class="text-sm text-text-tertiary">How many pay?</p>
    </div>
    <SectionDescription
        insight="Use this for completed purchases and the revenue they brought in over the selected window."
        caveat={'Paddle is the source of truth for money, so trust these numbers, just expect a little webhook lag near the ' +
            'present.'}
    />

    {#if !paddle.ok}
        <ErrorState error={paddle.error} {selection} />
    {:else}
        {@const p = paddle.data}
        {@const totalRevenue = p.transactions.reduce((sum, t) => sum + Number(t.total), 0)}
        {@const currency = p.transactions[0]?.currencyCode ?? 'USD'}

        <MetricRow
            metrics={[
                { label: 'Revenue', value: formatCurrency(totalRevenue, currency) },
                { label: 'Transactions', value: formatNumber(p.transactions.length) },
                { label: 'Active subscriptions', value: formatNumber(p.activeSubscriptions.length) },
            ]}
        />

        {#if p.transactions.length > 0}
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
                            {#each p.transactions.slice(0, 10) as txn}
                                <tr class="border-b border-border-subtle/50">
                                    <td class="py-1.5 pr-4 tabular-nums text-text-primary">{txn.createdAt.split('T')[0]}</td>
                                    <td class="py-1.5 pr-4 text-text-secondary">{txn.status}</td>
                                    <td class="py-1.5 text-right tabular-nums text-text-secondary"
                                        >{formatCurrency(txn.total, txn.currencyCode)}</td
                                    >
                                </tr>
                            {/each}
                        </tbody>
                    </table>
                </div>
            </div>
        {:else}
            <EmptyState />
        {/if}

        <ExternalLinks links={[{ label: 'View in Paddle', href: 'https://vendors.paddle.com' }]} />
    {/if}
</section>
