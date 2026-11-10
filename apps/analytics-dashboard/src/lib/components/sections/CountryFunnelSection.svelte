<!--
  The per-country funnel under the trend charts: a world map of visitors beside (or, on narrow screens,
  above) the table of Umami's visitors and clicks next to the api-server's downloads, installs, and pooled
  D7 and D28. Counts wear the fixed log color scale (`#lib/count-color-scale.ts`); each country name filters
  the charts above to it.
-->
<script lang="ts">
    import type { SourceResult, DashboardSelection } from '#lib/server/types.js'
    import type { FunnelCountriesData } from '#lib/server/sources/funnel-countries.js'
    import { type CountryFunnelRow, safeRatio, unknownCountry } from '#lib/acquisition-countries.js'
    import { shapeCountryTable } from '#lib/country-table.js'
    import { mapIdsForCountry } from '#lib/country-geo.js'
    import { COUNT_SCALE_PURPLE_AT, countColor, exceedsCountScale } from '#lib/count-color-scale.js'
    import { formatCountry, formatNumber, formatPercent } from '#lib/format.js'
    import { countryColor } from '#lib/country-colors.js'
    import InfoPopover from '../InfoPopover.svelte'
    import ErrorState from '../ErrorState.svelte'
    import WorldMap from '../WorldMap.svelte'

    interface Props {
        funnelCountries: SourceResult<FunnelCountriesData>
        country: string | null
        selection: DashboardSelection
        countryHref: (country: string | null) => string
        /** The window the numbers cover, for the title. */
        days?: number
        /** Switch the page's country filter in place. Without it, a country link does a full page load. */
        onselectcountry?: (country: string | null) => void
    }

    const { funnelCountries, country, selection, countryHref, days = 30, onselectcountry }: Props = $props()

    /** A plain click switches the filter in place; a modified click (new tab) keeps the link's own behavior. */
    function handleCountryClick(event: MouseEvent, code: string) {
        if (!onselectcountry || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return
        event.preventDefault()
        onselectcountry(code)
    }

    /** Countries past this fold into one "Other" row. */
    const topCount = 20

    /** The numbers every row shows, a real country or the "Other" sum. */
    type Counts = Omit<CountryFunnelRow, 'country'>

    function cell(value: number | null, format: (n: number) => string = formatNumber): string {
        return value === null ? '–' : format(value)
    }

    function pooled(retained: number | null, cohort: number | null): string {
        const rate = cell(safeRatio(retained, cohort), formatPercent)
        return cohort ? `${rate} (${String(retained)}/${String(cohort)})` : rate
    }

    /** The tooltip's lines, in the table's column order, so the two never disagree. */
    function metricLines(r: Counts): { label: string; value: string }[] {
        return [
            { label: 'Visitors', value: cell(r.visitors) },
            { label: 'Visitor → click', value: cell(safeRatio(r.clicks, r.visitors), formatPercent) },
            { label: 'Clicks', value: cell(r.clicks) },
            { label: 'Server downloads', value: cell(r.serverDownloads) },
            { label: 'Click → install', value: cell(safeRatio(r.newInstalls, r.clicks), formatPercent) },
            { label: 'Installs', value: cell(r.newInstalls) },
            { label: 'D7 (pooled)', value: pooled(r.d7Retained, r.d7Cohort) },
            { label: 'D28 (pooled)', value: pooled(r.d28Retained, r.d28Cohort) },
        ]
    }

    const scaleTop = formatNumber(COUNT_SCALE_PURPLE_AT)
    const scaleNext = formatNumber(COUNT_SCALE_PURPLE_AT * 10)
</script>

{#snippet countChip(value: number | null)}
    {#if value === null}
        <span class="text-text-tertiary">–</span>
    {:else if value === 0}
        <span class="text-text-tertiary">0</span>
    {:else}
        <span
            class="count-chip rounded-sm px-1.5 py-0.5 text-text-primary"
            style:--color-count={countColor(value)}>{formatNumber(value)}</span
        >
    {/if}
{/snippet}

{#snippet countCells(r: Counts)}
    <td class="py-1.5 pr-3 text-right tabular-nums">{@render countChip(r.visitors)}</td>
    <td class="py-1.5 pr-3 text-right text-text-secondary tabular-nums">{cell(safeRatio(r.clicks, r.visitors), formatPercent)}</td>
    <td class="py-1.5 pr-3 text-right tabular-nums">{@render countChip(r.clicks)}</td>
    <td class="py-1.5 pr-3 text-right tabular-nums">{@render countChip(r.serverDownloads)}</td>
    <td class="py-1.5 pr-3 text-right text-text-secondary tabular-nums">{cell(safeRatio(r.newInstalls, r.clicks), formatPercent)}</td>
    <td class="py-1.5 pr-3 text-right tabular-nums">{@render countChip(r.newInstalls)}</td>
    {#each [[r.d7Retained, r.d7Cohort], [r.d28Retained, r.d28Cohort]] as [retained, cohort], i (i)}
        <td class="py-1.5 text-right whitespace-nowrap text-text-secondary tabular-nums {i === 0 ? 'pr-3' : ''}">
            {cell(safeRatio(retained, cohort), formatPercent)}{#if cohort}<span class="ml-1 text-xs text-text-tertiary"
                    >({retained}/{cohort})</span
                >{/if}
        </td>
    {/each}
{/snippet}

{#snippet headerWithInfo(label: string, topic: string, text: string)}
    <th class="py-2 pr-3 text-right font-medium last:pr-0">
        <span class="inline-flex items-center gap-1">
            {label}
            <InfoPopover {topic} align="end">{text}</InfoPopover>
        </span>
    </th>
{/snippet}

<section class="mb-6 rounded-xl border border-border bg-surface p-4 sm:p-6">
    <h2 class="mb-1 flex items-center gap-2 text-lg font-semibold text-text-primary">
        By country, last {days} days
        <InfoPopover topic="the country table">
            The last {days} UTC days including today. Visitors and clicks come from Umami, by the visitor's IP country;
            downloads, installs, D7, and D28 come from the api-server (downloads by the request's country, installs by
            the country on the install's heartbeat). "(unknown)" is everything without a usable country. Rates pool as ratios of sums. Counts
            are colored on a fixed log scale: red is 1, yellow 10, cyan 100, and purple {scaleTop}.
        </InfoPopover>
    </h2>
    <p class="mb-3 text-sm text-text-tertiary">
        Small per-country numbers are noisy: one person more or less moves a rate a lot. Pick a country to filter the
        charts above.
    </p>

    {#if funnelCountries.ok}
        {@const data = funnelCountries.data}
        {@const table = shapeCountryTable(data.rows, topCount)}
        {@const realRows = data.rows.filter((r) => r.country !== unknownCountry)}
        {@const rowsByCountry = new Map(realRows.map((r) => [r.country, r]))}
        {@const visitorsIn = (code: string): number | null => {
            const row = rowsByCountry.get(code)
            return row ? row.visitors : data.umamiAvailable ? 0 : null
        }}
        {@const notOnMap = realRows.filter((r) => (r.visitors ?? 0) > 0 && mapIdsForCountry(r.country).length === 0)}

        {#if exceedsCountScale(data.rows.map((r) => r.visitors))}
            <div role="status" class="mb-3 rounded-lg border border-warning/50 bg-warning/10 px-3 py-2 text-sm text-text-primary">
                <span class="font-semibold text-warning">Time to raise the scale:</span> visitor counts now pass {scaleTop}
                (the purple end of the scale). Ask an agent to raise <code class="text-xs">COUNT_SCALE_PURPLE_AT</code> in
                <code class="text-xs">src/lib/count-color-scale.ts</code> to {scaleNext}.
            </div>
        {/if}
        {#if !data.workerAvailable}
            <p class="mb-2 text-xs text-text-tertiary">
                Downloads, installs, and retention by country aren't available yet (the api-server's update hasn't
                landed).
            </p>
        {/if}
        {#if !data.umamiAvailable}
            <p class="mb-2 text-xs text-text-tertiary">Couldn't load visitors and clicks from Umami.</p>
        {/if}

        {#if table.top.length === 0 && !table.unknown}
            <p class="text-sm text-text-tertiary">No visitors or downloads in the last {days} days.</p>
        {:else}
            <div class="grid grid-cols-1 items-start gap-6 2xl:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]">
                <!-- Stacked, the map caps its width so it doesn't push the table a screen down. -->
                <div class="mx-auto w-full max-w-3xl 2xl:max-w-none">
                    <WorldMap
                        valueOf={visitorsIn}
                        hasData={(code: string) => rowsByCountry.has(code)}
                        showNoData={!data.umamiAvailable}
                        measure="visitors"
                        labelOf={(code: string) => `${formatCountry(code)}: ${cell(visitorsIn(code))} visitors`}
                    >
                        {#snippet tooltip(code: string)}
                            {@const row = rowsByCountry.get(code)}
                            <p class="mb-1.5 font-semibold text-text-primary">{formatCountry(code)}</p>
                            {#if row}
                                <dl class="grid grid-cols-[1fr_auto] gap-x-3 gap-y-0.5">
                                    {#each metricLines(row) as line (line.label)}
                                        <dt>{line.label}</dt>
                                        <dd class="text-right text-text-primary tabular-nums">{line.value}</dd>
                                    {/each}
                                </dl>
                            {:else}
                                <p>No visitors or downloads in the last {days} days.</p>
                            {/if}
                        {/snippet}
                    </WorldMap>
                    {#if notOnMap.length > 0}
                        <p class="mt-2 text-xs text-text-tertiary">
                            Too small for the map: {notOnMap
                                .map((r) => `${formatCountry(r.country)}: ${cell(r.visitors)}`)
                                .join(', ')}.
                        </p>
                    {/if}
                </div>

                <div class="overflow-x-auto">
                    <table class="w-full text-sm">
                        <thead>
                            <tr class="border-b border-border text-xs text-text-tertiary">
                                <th class="py-2 pr-3 text-left font-medium">Country</th>
                                <th class="py-2 pr-3 text-right font-medium">Visitors</th>
                                <th class="py-2 pr-3 text-right font-medium">Visitor → click</th>
                                <th class="py-2 pr-3 text-right font-medium">Clicks</th>
                                {@render headerWithInfo(
                                    'Server downloads',
                                    'server downloads',
                                    'Every DMG request the api-server logged, bots included. A country with many downloads and few installs is likely a scraper.',
                                )}
                                <th class="py-2 pr-3 text-right font-medium">Click → install</th>
                                <th class="py-2 pr-3 text-right font-medium">Installs</th>
                                {@render headerWithInfo(
                                    'D7 (pooled)',
                                    'pooled D7',
                                    'All D7-retained installs ÷ all installs in the cohorts old enough to have a D7, with the raw counts beside it.',
                                )}
                                {@render headerWithInfo(
                                    'D28 (pooled)',
                                    'pooled D28',
                                    'The same for day 28: retained ÷ installs in the cohorts old enough to have a D28. A dash until the api-server sends it.',
                                )}
                            </tr>
                        </thead>
                        <tbody>
                            {#each table.top as r (r.country)}
                                <tr class="border-b border-border-subtle {r.country === country ? 'bg-accent/10' : ''}">
                                    <th scope="row" class="py-1.5 pr-3 text-left font-medium text-text-primary">
                                        <!-- eslint-disable svelte/no-navigation-without-resolve -- `countryHref` builds it with resolve('/') -->
                                        <a
                                            class="inline-flex items-center gap-2 hover:text-accent"
                                            href={countryHref(r.country)}
                                            onclick={(e: MouseEvent) => { handleCountryClick(e, r.country); }}
                                        >
                                            <span
                                                class="inline-block size-2.5 shrink-0 rounded-full"
                                                style="background-color: {countryColor(r.country)}"
                                                aria-hidden="true"
                                            ></span>
                                            {formatCountry(r.country)}
                                        </a>
                                        <!-- eslint-enable svelte/no-navigation-without-resolve -->
                                    </th>
                                    {@render countCells(r)}
                                </tr>
                            {/each}
                            {#if table.other}
                                <tr class="border-b border-border-subtle">
                                    <th scope="row" class="py-1.5 pr-3 text-left font-medium text-text-secondary">
                                        Other ({table.other.countryCount}
                                        {table.other.countryCount === 1 ? 'country' : 'countries'})
                                    </th>
                                    {@render countCells(table.other)}
                                </tr>
                            {/if}
                            {#if table.unknown}
                                <tr class="border-b border-border-subtle">
                                    <th scope="row" class="py-1.5 pr-3 text-left font-medium text-text-secondary">
                                        {unknownCountry}
                                    </th>
                                    {@render countCells(table.unknown)}
                                </tr>
                            {/if}
                        </tbody>
                    </table>
                </div>
            </div>
        {/if}
    {:else}
        <ErrorState error={funnelCountries.error} {selection} />
    {/if}
</section>

<style>
    /* A tint of the count's color behind primary ink: the hue reads at a glance, the number keeps full contrast. */
    .count-chip {
        background-color: color-mix(in oklab, var(--color-count) 26%, transparent);
        box-shadow: inset 0 0 0 1px color-mix(in oklab, var(--color-count) 55%, transparent);
    }
</style>
