<!--
  The per-country funnel for the last 30 days, under the trend charts: Umami's visitors and clicks next to
  the api-server's downloads, installs, and pooled D7. Each country name filters the charts above to it.
-->
<script lang="ts">
    import type { SourceResult, DashboardSelection } from '#lib/server/types.js'
    import type { FunnelCountriesData } from '#lib/server/sources/funnel-countries.js'
    import { safeRatio, unknownCountry } from '#lib/acquisition-countries.js'
    import { formatCountry, formatNumber, formatPercent } from '#lib/format.js'
    import InfoPopover from '../InfoPopover.svelte'
    import ErrorState from '../ErrorState.svelte'

    interface Props {
        funnelCountries: SourceResult<FunnelCountriesData>
        country: string | null
        selection: DashboardSelection
        countryHref: (country: string | null) => string
    }

    const { funnelCountries, country, selection, countryHref }: Props = $props()

    /** Rows past this sit behind a "Show all" disclosure. */
    const shownRows = 20

    function cell(value: number | null, format: (n: number) => string = formatNumber): string {
        return value === null ? '–' : format(value)
    }
</script>

<section class="mb-6 rounded-xl border border-border bg-surface p-4 sm:p-6">
    <h2 class="mb-1 flex items-center gap-2 text-lg font-semibold text-text-primary">
        By country, last 30 days
        <InfoPopover topic="the country table">
            The last 30 UTC days including today. Visitors and clicks come from Umami, by the visitor's IP country;
            downloads, installs, and D7 come from the api-server (downloads by the request's country, installs by the
            country on the install's heartbeat). Clicks are looked up for the top 12 visitor countries only, so the rest
            show a dash. "(unknown)" is everything without a usable country. Rates pool as ratios of sums.
        </InfoPopover>
    </h2>
    <p class="mb-3 text-sm text-text-tertiary">
        Small per-country numbers are noisy: one person more or less moves a rate a lot. Pick a country to filter the
        charts above.
    </p>

    {#if funnelCountries.ok}
        {@const data = funnelCountries.data}
        {@const rows = data.rows.filter(
            (r) => (r.visitors ?? 0) > 0 || (r.serverDownloads ?? 0) > 0 || (r.newInstalls ?? 0) > 0,
        )}
        {#if !data.workerAvailable}
            <p class="mb-2 text-xs text-text-tertiary">
                Downloads, installs, and D7 by country aren't available yet (the api-server's update hasn't landed).
            </p>
        {/if}
        {#if !data.umamiAvailable}
            <p class="mb-2 text-xs text-text-tertiary">Couldn't load visitors and clicks from Umami.</p>
        {/if}
        {#snippet countryRow(r: (typeof rows)[number])}
            <tr class="border-b border-border-subtle {r.country === country ? 'bg-accent/10' : ''}">
                <th scope="row" class="py-1.5 pr-4 text-left font-medium text-text-primary">
                    {#if r.country === unknownCountry}
                        {r.country}
                    {:else}
                        <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- `countryHref` builds it with resolve('/') -->
                        <a class="hover:text-accent" href={countryHref(r.country)}>{formatCountry(r.country)}</a>
                    {/if}
                </th>
                <td class="py-1.5 pr-4 text-right text-text-secondary tabular-nums">{cell(r.visitors)}</td>
                <td class="py-1.5 pr-4 text-right text-text-secondary tabular-nums">{cell(r.clicks)}</td>
                <td class="py-1.5 pr-4 text-right text-text-secondary tabular-nums">{cell(safeRatio(r.clicks, r.visitors), formatPercent)}</td>
                <td class="py-1.5 pr-4 text-right text-text-secondary tabular-nums">{cell(r.serverDownloads)}</td>
                <td class="py-1.5 pr-4 text-right text-text-secondary tabular-nums">{cell(r.newInstalls)}</td>
                <td class="py-1.5 pr-4 text-right text-text-secondary tabular-nums">{cell(safeRatio(r.newInstalls, r.clicks), formatPercent)}</td>
                <td class="py-1.5 text-right text-text-secondary tabular-nums">
                    {cell(safeRatio(r.d7Retained, r.d7Cohort), formatPercent)}{#if r.d7Cohort}<span
                            class="ml-1 text-xs text-text-tertiary">({r.d7Retained}/{r.d7Cohort})</span
                        >{/if}
                </td>
            </tr>
        {/snippet}
        {#snippet head()}
            <thead>
                <tr class="border-b border-border text-xs text-text-tertiary">
                    <th class="py-2 pr-4 text-left font-medium">Country</th>
                    <th class="py-2 pr-4 text-right font-medium">Visitors</th>
                    <th class="py-2 pr-4 text-right font-medium">Clicks</th>
                    <th class="py-2 pr-4 text-right font-medium">Visitor → click</th>
                    <th class="py-2 pr-4 text-right font-medium">
                        <span class="inline-flex items-center gap-1">
                            Server downloads
                            <InfoPopover topic="server downloads" align="end">
                                Every DMG request the api-server logged, bots included. A country with many downloads
                                and few installs is likely a scraper.
                            </InfoPopover>
                        </span>
                    </th>
                    <th class="py-2 pr-4 text-right font-medium">Installs</th>
                    <th class="py-2 pr-4 text-right font-medium">Click → install</th>
                    <th class="py-2 text-right font-medium">
                        <span class="inline-flex items-center gap-1">
                            D7 (pooled)
                            <InfoPopover topic="pooled D7" align="end">
                                All D7-retained installs ÷ all installs in the cohorts old enough to have a D7, with the
                                raw counts beside it.
                            </InfoPopover>
                        </span>
                    </th>
                </tr>
            </thead>
        {/snippet}
        {#if rows.length === 0}
            <p class="text-sm text-text-tertiary">No visitors or downloads in the last 30 days.</p>
        {:else}
            <div class="overflow-x-auto">
                <table class="w-full text-sm">
                    {@render head()}
                    <tbody>
                        {#each rows.slice(0, shownRows) as r (r.country)}
                            {@render countryRow(r)}
                        {/each}
                    </tbody>
                </table>
            </div>
            {#if rows.length > shownRows}
                <details class="mt-2">
                    <summary class="cursor-pointer text-xs text-text-secondary hover:text-text-primary">
                        Show the other {rows.length - shownRows} countries
                    </summary>
                    <div class="overflow-x-auto">
                        <table class="w-full text-sm">
                            {@render head()}
                            <tbody>
                                {#each rows.slice(shownRows) as r (r.country)}
                                    {@render countryRow(r)}
                                {/each}
                            </tbody>
                        </table>
                    </div>
                </details>
            {/if}
        {/if}
    {:else}
        <ErrorState error={funnelCountries.error} {selection} />
    {/if}
</section>
