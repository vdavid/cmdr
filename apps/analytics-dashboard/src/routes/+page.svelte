<!--
  Acquisition page (`/`): the trend charts (one row per funnel metric, scoped by the country chips), the
  per-country table, and the awareness, interest, and download sections.
  The shared range picker and nav live in the layout. Each section is its own component.
-->
<script lang="ts">
    import type { PageProps } from './$types'
    import { rangeDays } from '#lib/time-range.js'
    import { page } from '$app/state'
    import { goto } from '$app/navigation'
    import { resolve } from '$app/paths'
    import AcquisitionTrendsSection from '#lib/components/sections/AcquisitionTrendsSection.svelte'
    import CountryFunnelSection from '#lib/components/sections/CountryFunnelSection.svelte'
    import AwarenessSection from '#lib/components/sections/AwarenessSection.svelte'
    import InterestSection from '#lib/components/sections/InterestSection.svelte'
    import DownloadSection from '#lib/components/sections/DownloadSection.svelte'

    const { data }: PageProps = $props()

    /** Today's UTC day as YYYY-MM-DD (the charts fade it and the stats leave it out: it's partial). */
    const todayIso = new Date().toISOString().slice(0, 10)

    /** The selected range's length in days: the trend charts' window and the star charts' default zoom. */
    const days = $derived(rangeDays[data.selection.range])
    const starChartXMin = $derived(Date.now() / 1000 - days * 86400)

    /**
     * The country filter. It starts from `?country=` and follows a navigation that changes it (a link from
     * the country table), while a chip click sets it here directly: the funnel already carries every chip's
     * slice, so filtering is instant and only the URL changes, shallowly, so links and reloads still land on it.
     */
    let country = $derived(data.country)

    function selectCountry(code: string | null) {
        country = code
        // eslint-disable-next-line svelte/no-navigation-without-resolve -- `countryHref` builds every href with resolve('/')
        void goto(countryHref(code), { shallow: true, replace: true })
    }

    /** This page's URL with `?country=` set or cleared, keeping the range. */
    function countryHref(code: string | null): string {
        // A throwaway builder for one href, so it needs no reactivity. It reads the live URL, because a chip
        // click updates it shallowly and `page.url` doesn't follow a shallow navigation.
        // eslint-disable-next-line svelte/prefer-svelte-reactivity
        const params = new URLSearchParams(typeof window === 'undefined' ? page.url.search : window.location.search)
        if (code) params.set('country', code)
        else params.delete('country')
        const query = params.toString()
        return query ? `${resolve('/')}?${query}` : resolve('/')
    }

</script>

<AcquisitionTrendsSection
    funnel={data.funnel}
    {country}
    onselectcountry={selectCountry}
    viewDays={days}
    selection={data.selection}
    {todayIso}
/>

<CountryFunnelSection
    funnelCountries={data.funnelCountries}
    {country}
    selection={data.selection}
    {countryHref}
    {days}
    onselectcountry={selectCountry}
/>

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
