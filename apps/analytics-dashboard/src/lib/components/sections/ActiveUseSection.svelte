<!--
  Product § Active use: true daily active installs from the heartbeat, the per-day "got the latest
  release" stacked bars (by the version each install was on), and license activation counts.
-->
<script lang="ts">
    import type { SourceResult, DashboardSelection } from '$lib/server/types.js'
    import type { CloudflareData } from '$lib/server/sources/cloudflare.js'
    import type { LicenseData } from '$lib/server/sources/license.js'
    import { uniqueDays, updateVersionSeries } from '$lib/chart-helpers.js'
    import { formatNumber } from '$lib/format.js'
    import { COLOR_GOLD } from '$lib/colors.js'
    import Chart from '$lib/components/Chart.svelte'
    import StackedBarChart from '$lib/components/StackedBarChart.svelte'
    import ErrorState from '$lib/components/ErrorState.svelte'
    import MetricRow from '$lib/components/MetricRow.svelte'
    import Methodology from '$lib/components/Methodology.svelte'
    import SectionDescription from '$lib/components/SectionDescription.svelte'
    import BetaEmptyState from '$lib/components/BetaEmptyState.svelte'
    import ExternalLinks from '$lib/components/ExternalLinks.svelte'

    let {
        cloudflare,
        license,
        selection,
    }: {
        cloudflare: SourceResult<CloudflareData>
        license: SourceResult<LicenseData>
        selection: DashboardSelection
    } = $props()
</script>

<section class="rounded-xl border border-border bg-surface p-6">
    <div class="mb-1">
        <h2 class="text-lg font-semibold text-text-primary">Active use</h2>
        <p class="text-sm text-text-tertiary">How many run the app?</p>
    </div>
    <SectionDescription
        insight="Use this for true daily active installs and how fast the fleet rolls onto each new release."
        caveat={'DAU here comes from the hourly heartbeat (distinct install ids per day), the trustworthy active-use number. ' +
            'The older update-check count is a weaker proxy (it only fires when the updater runs). Debug builds and ' +
            'anyone who opts out of analytics are excluded.'}
    />

    {#if !cloudflare.ok}
        <ErrorState error={cloudflare.error} {selection} />
    {:else}
        {@const cf = cloudflare.data}
        {@const dau = cf.heartbeatDau}
        {@const latestDau = dau.length > 0 ? dau[dau.length - 1].dau : 0}
        {@const peakDau = dau.reduce((max, r) => Math.max(max, r.dau), 0)}
        {@const totalBeats = dau.reduce((sum, r) => sum + r.beats, 0)}
        {@const totalDau = dau.reduce((sum, r) => sum + r.dau, 0)}
        {@const beatsPerActive = totalDau > 0 ? totalBeats / totalDau : 0}

        {#if dau.length > 0}
            <MetricRow
                metrics={[
                    { label: 'Daily active installs (latest day)', value: formatNumber(latestDau), color: COLOR_GOLD },
                    { label: 'Peak daily active', value: formatNumber(peakDau) },
                    { label: 'Beats per active install', value: beatsPerActive.toFixed(1) },
                ]}
            />

            <div class="mt-4">
                <h3 class="mb-2 text-sm font-medium text-text-secondary">Daily active installs</h3>
                <Chart
                    data={[dau.map((r) => new Date(r.date).getTime() / 1000), dau.map((r) => r.dau)]}
                    labels={['Active installs']}
                    colors={[COLOR_GOLD]}
                    height={180}
                />
            </div>
        {:else}
            <BetaEmptyState />
        {/if}

        {@const updateActivity = cf.updateActivity}
        {@const updateDays = uniqueDays(updateActivity)}
        {@const updateSeries = updateVersionSeries(updateActivity, updateDays)}
        {#if updateActivity.length > 0}
            <div class="mt-6 border-t border-border-subtle pt-5">
                <h3 class="mb-2 text-sm font-medium text-text-secondary">Got the latest release per day, by version</h3>
                <StackedBarChart days={updateDays} series={updateSeries} unitLabel="installs" height={140} />
                <Methodology
                    text={"Counts running installs with auto-update on that checked for updates each day (the app's update " +
                        'check hits our server, then redirects to the latest release), deduplicated to distinct installs per ' +
                        'day via a daily-rotating hashed IP. Stacked by the version each install was on when it checked, so you ' +
                        'see the fleet roll onto a new release. Separate from new installs above: these are existing users updating ' +
                        'in place, not fresh downloads. Hover a bar for exact numbers.'}
                />
            </div>
        {/if}

        {#if license.ok}
            {@const lic = license.data}
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

        <ExternalLinks links={[{ label: 'View in Cloudflare', href: 'https://dash.cloudflare.com' }]} />
    {/if}
</section>
