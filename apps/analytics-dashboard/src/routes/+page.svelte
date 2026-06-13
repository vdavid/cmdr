<!--
  Acquisition page (`/`): the daily funnel plus the awareness, interest, and download sections. The
  shared range/day picker and nav live in the layout; this page renders the sections and handles the
  funnel's row-click "filter to this day". Each section is its own component under `$lib/components`.
-->
<script lang="ts">
    import FunnelTable from '$lib/components/FunnelTable.svelte'
    import AwarenessSection from '$lib/components/sections/AwarenessSection.svelte'
    import InterestSection from '$lib/components/sections/InterestSection.svelte'
    import DownloadSection from '$lib/components/sections/DownloadSection.svelte'

    let { data } = $props()

    /** True when a single specific UTC day is selected (vs a relative range). */
    const isDaySelected = $derived(data.selection.range === 'day')
    /** The selected specific day, or '' for the funnel's active-row highlight. */
    const selectedDay = $derived(data.selection.day ?? '')
    /** Today's UTC day as YYYY-MM-DD (the funnel marks today's row "partial"). */
    const todayIso = new Date().toISOString().slice(0, 10)

    /** Time range in seconds per relative range, the default zoom window for the star charts. */
    const rangeSeconds: Record<string, number> = { today: 86400, '24h': 86400, '7d': 7 * 86400, '30d': 30 * 86400 }
    const starChartXMin = $derived(Date.now() / 1000 - (rangeSeconds[data.selection.range] ?? 7 * 86400))

    /** Filter the dashboard to a specific UTC day (funnel row click), staying on this page. */
    function selectDay(day: string) {
        window.location.href = day ? `/?day=${day}` : '/?range=7d'
    }
</script>

<FunnelTable
    funnel={data.funnel}
    selection={data.selection}
    {selectedDay}
    {isDaySelected}
    {todayIso}
    onselectday={selectDay}
/>

<!-- Acquisition sections -->
<div class="grid grid-cols-1 gap-6 md:grid-cols-2 xl:grid-cols-3">
    <AwarenessSection umami={data.umami} githubStars={data.githubStars} selection={data.selection} {starChartXMin} />
    <InterestSection umami={data.umami} posthog={data.posthog} selection={data.selection} />
    <DownloadSection cloudflare={data.cloudflare} github={data.github} selection={data.selection} />
</div>
