<!--
  Acquisition page (`/`): the trend charts (one row per funnel metric, scoped by the country chips), the
  per-country table, the awareness, interest, and download sections, and the old daily table, collapsed.
  The shared range/day picker and nav live in the layout. Each section is its own component.
-->
<script lang="ts">
    import type { PageProps } from './$types'
    import type { TimeRange } from '#lib/server/types.js'
    import { page } from '$app/state'
    import { resolve } from '$app/paths'
    import FunnelTable from '#lib/components/FunnelTable.svelte'
    import AcquisitionTrendsSection from '#lib/components/sections/AcquisitionTrendsSection.svelte'
    import CountryFunnelSection from '#lib/components/sections/CountryFunnelSection.svelte'
    import AwarenessSection from '#lib/components/sections/AwarenessSection.svelte'
    import InterestSection from '#lib/components/sections/InterestSection.svelte'
    import DownloadSection from '#lib/components/sections/DownloadSection.svelte'

    const { data }: PageProps = $props()

    /** True when a single specific UTC day is selected (vs a relative range). */
    const isDaySelected = $derived(data.selection.range === 'day')
    /** The selected specific day, or '' for the funnel's active-row highlight. */
    const selectedDay = $derived(data.selection.day ?? '')
    /** Today's UTC day as YYYY-MM-DD (the charts fade it and the stats leave it out: it's partial). */
    const todayIso = new Date().toISOString().slice(0, 10)

    /** Time range in seconds per relative range, the default zoom window for the star charts. */
    const rangeSeconds: Partial<Record<TimeRange, number>> = {
        today: 86400,
        '24h': 86400,
        '7d': 7 * 86400,
        '30d': 30 * 86400,
    }
    const starChartXMin = $derived(Date.now() / 1000 - (rangeSeconds[data.selection.range] ?? 7 * 86400))

    /** This page's URL with `?country=` set or cleared, keeping the range/day selection. */
    function countryHref(country: string | null): string {
        // A throwaway builder for one href, so it needs no reactivity.
        // eslint-disable-next-line svelte/prefer-svelte-reactivity
        const params = new URLSearchParams(page.url.search)
        if (country) params.set('country', country)
        else params.delete('country')
        const query = params.toString()
        return query ? `${resolve('/')}?${query}` : resolve('/')
    }

    /** Filter the dashboard to a specific UTC day (old table row click), staying on this page. */
    function selectDay(day: string) {
        // A throwaway builder for one navigation, so it needs no reactivity.
        // eslint-disable-next-line svelte/prefer-svelte-reactivity
        const params = new URLSearchParams(page.url.search)
        params.delete('range')
        params.delete('day')
        if (day) params.set('day', day)
        else params.set('range', '7d')
        window.location.href = `${resolve('/')}?${params.toString()}`
    }
</script>

<AcquisitionTrendsSection
    funnel={data.funnel}
    funnelCountries={data.funnelCountries}
    country={data.country}
    selection={data.selection}
    {todayIso}
    {countryHref}
/>

<CountryFunnelSection funnelCountries={data.funnelCountries} country={data.country} selection={data.selection} {countryHref} />

<!-- Acquisition sections -->
<div class="mb-6 grid grid-cols-1 gap-6 md:grid-cols-2 xl:grid-cols-3">
    <AwarenessSection
        umami={data.umami}
        umamiSiteUrls={data.umamiSiteUrls}
        githubStars={data.githubStars}
        selection={data.selection}
        {starChartXMin}
    />
    <InterestSection umami={data.umami} posthog={data.posthog} selection={data.selection} />
    <DownloadSection cloudflare={data.cloudflare} github={data.github} selection={data.selection} />
</div>

<FunnelTable
    funnel={data.funnel}
    selection={data.selection}
    {selectedDay}
    {isDaySelected}
    {todayIso}
    onselectday={selectDay}
/>
