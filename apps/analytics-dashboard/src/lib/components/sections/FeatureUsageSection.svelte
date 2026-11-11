<!--
  Product § Feature usage: per feature (the `feature-status.json` ids), how many installs used it in the
  range, against the range's active installs. The mapping lives in the api-server
  (`featureUsageDefinitions`); this only renders what comes back.
-->
<script lang="ts">
    import type { SourceResult, DashboardSelection } from '#lib/server/types.js'
    import type { FeatureUsageData } from '#lib/feature-usage.js'
    import { formatFeatureShare, formatParts, sortFeatureUsage } from '#lib/feature-usage.js'
    import { formatNumber } from '#lib/format.js'
    import ErrorState from '#lib/components/ErrorState.svelte'
    import Methodology from '#lib/components/Methodology.svelte'
    import SectionDescription from '#lib/components/SectionDescription.svelte'

    const {
        featureUsage,
        selection,
    }: {
        featureUsage: SourceResult<FeatureUsageData>
        selection: DashboardSelection
    } = $props()
</script>

<section class="rounded-xl border border-border bg-surface p-6 md:col-span-2 xl:col-span-3">
    <div class="mb-1">
        <h2 class="text-lg font-semibold text-text-primary">Feature usage</h2>
        <p class="text-sm text-text-tertiary">Which features do people use?</p>
    </div>
    <SectionDescription
        insight="Use this in the per-release feature-status review: how many installs reached for each feature in the range, as a share of the installs that ran the app at all."
        caveat={'Counts installs with usage stats on, like every heartbeat number. An event rides the next heartbeat, ' +
            'so the last few hours are partial. "No data yet" means no release has sent that event yet, which is ' +
            'different from nobody using the feature.'}
    />

    {#if featureUsage.ok}
        {@const data = featureUsage.data}
        <p class="mt-3 text-sm text-text-secondary">
            {formatNumber(data.activeInstalls)} active
            {data.activeInstalls === 1 ? 'install' : 'installs'} since {data.since.slice(0, 10)}
        </p>
        <div class="mt-3 overflow-x-auto">
            <table class="w-full text-left text-sm">
                <thead>
                    <tr class="border-b border-border-subtle text-text-tertiary">
                        <th class="pr-4 pb-2 font-medium">Feature</th>
                        <th class="pr-4 pb-2 text-right font-medium">Installs</th>
                        <th class="pr-4 pb-2 text-right font-medium">Share</th>
                        <th class="pr-4 pb-2 text-right font-medium">Events</th>
                        <th class="pb-2 font-medium">Breakdown</th>
                    </tr>
                </thead>
                <tbody>
                    {#each sortFeatureUsage(data.features) as feature (feature.id)}
                        <tr class="border-b border-border-subtle/50" title={feature.sources.join(', ')}>
                            <td class="py-1.5 pr-4 text-text-primary">
                                {feature.name}
                                <span class="font-mono text-xs text-text-tertiary">{feature.id}</span>
                            </td>
                            {#if feature.everSeen}
                                <td class="py-1.5 pr-4 text-right text-text-secondary tabular-nums">
                                    {formatNumber(feature.installs)}
                                </td>
                                <td class="py-1.5 pr-4 text-right text-text-secondary tabular-nums">
                                    {formatFeatureShare(feature, data.activeInstalls)}
                                </td>
                                <td class="py-1.5 pr-4 text-right text-text-secondary tabular-nums">
                                    {formatNumber(feature.events)}
                                </td>
                                <td class="py-1.5 text-xs text-text-tertiary">{formatParts(feature.parts) || '–'}</td>
                            {:else}
                                <td class="py-1.5 text-text-tertiary" colspan="4">No data yet</td>
                            {/if}
                        </tr>
                    {/each}
                </tbody>
            </table>
        </div>
        <Methodology
            text={'An install counts toward a feature once it fired any of that feature\'s events in the range. ' +
                'Hover a row to see which events it counts. The share divides by installs that sent at least one ' +
                'heartbeat in the range.'}
        />
    {:else}
        <ErrorState error={featureUsage.error} {selection} />
    {/if}
</section>
