<!--
  Product § Active use: daily active installs as a RANGE (heartbeat floor, update-check reach), the
  per-day "got the latest release" stacked bars (by the version each install was on), and license
  activation counts.
-->
<script lang="ts">
    import type { SourceResult, DashboardSelection } from '$lib/server/types.js'
    import type { CloudflareData } from '$lib/server/sources/cloudflare.js'
    import type { LicenseData } from '$lib/server/sources/license.js'
    import { uniqueDays, updateVersionSeries } from '$lib/chart-helpers.js'
    import { formatNumber } from '$lib/format.js'
    import { boundsByDay, formatBound, largestUnseenShare, latestBound } from '$lib/active-installs.js'
    import { COLOR_GOLD, COLOR_PURPLE } from '$lib/colors.js'
    import Chart from '$lib/components/Chart.svelte'
    import StackedBarChart from '$lib/components/StackedBarChart.svelte'
    import ErrorState from '$lib/components/ErrorState.svelte'
    import MetricRow from '$lib/components/MetricRow.svelte'
    import Methodology from '$lib/components/Methodology.svelte'
    import SectionDescription from '$lib/components/SectionDescription.svelte'
    import BetaEmptyState from '$lib/components/BetaEmptyState.svelte'
    import ExternalLinks from '$lib/components/ExternalLinks.svelte'

    const {
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
        insight="Use this for how many people run Cmdr, and how fast the fleet rolls onto each new release."
        caveat={'Active installs read as a range on purpose. Anyone who opts out of analytics sends nothing at all, ' +
            "not even an 'I opted out' bit, so a single number would claim a precision we don't have."}
    />

    {#if cloudflare.ok}
        {@const cf = cloudflare.data}
        {@const dau = cf.heartbeatDau}
        {@const bounds = boundsByDay(dau, cf.updateActivity)}
        {@const latest = latestBound(bounds)}
        {@const unseen = largestUnseenShare(bounds)}
        {@const peakDau = dau.reduce((max, r) => Math.max(max, r.dau), 0)}
        {@const totalBeats = dau.reduce((sum, r) => sum + r.beats, 0)}
        {@const totalDau = dau.reduce((sum, r) => sum + r.dau, 0)}
        {@const beatsPerActive = totalDau > 0 ? totalBeats / totalDau : 0}

        {#if dau.length > 0}
            <MetricRow
                metrics={[
                    { label: 'Active installs (latest day)', value: formatBound(latest), color: COLOR_GOLD },
                    { label: 'Peak confirmed running', value: formatNumber(peakDau) },
                    { label: 'Beats per active install', value: beatsPerActive.toFixed(1) },
                ]}
            />

            <p class="mt-3 text-xs leading-relaxed text-text-tertiary">
                The low end counts install ids we heard from, so those installs definitely ran Cmdr. The high end counts
                the separate addresses that checked for updates, which catches people who turned analytics off.
                {#if unseen !== null}
                    At the widest, {Math.round(unseen * 100)}% of the high end never sent a heartbeat.
                {/if}
            </p>

            <div class="mt-4">
                <h3 class="mb-2 text-sm font-medium text-text-secondary">Active installs per day</h3>
                <Chart
                    data={[
                        bounds.map((b) => new Date(b.day).getTime() / 1000),
                        bounds.map((b) => b.floor),
                        bounds.map((b) => b.reach),
                    ]}
                    labels={['Heard from', 'Checked for updates']}
                    colors={[COLOR_GOLD, COLOR_PURPLE]}
                    height={180}
                />
                <Methodology
                    text={'Heard from: distinct install ids on the hourly heartbeat, everyone who consented to ' +
                        'analytics. Checked for updates: distinct addresses that asked our server for the latest ' +
                        'version, a separate consent that opted-out installs still ride. Treat the high end as a ' +
                        'rough reach, not a ceiling: addresses are not installs, one office or household behind a ' +
                        'shared connection counts once, a changing home address counts a single install more than ' +
                        'once across days, and anyone who turned automatic update checks off never appears at all.'}
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
    {:else}
        <ErrorState error={cloudflare.error} {selection} />
    {/if}
</section>
