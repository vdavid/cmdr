<!--
  Shared shell for the whole dashboard: a sticky top bar with the page nav (Acquisition, Product, Link
  codes) and the range/day picker. The picker drives the shared time selection via `?range=` / `?day=`
  and stays on the current page when you switch, so Acquisition and Product share selection state. It's
  hidden on the Link codes page, where a time range is irrelevant. The selection comes from the layout
  load; `updatedAt` comes from whichever data page is active.
-->
<script lang="ts">
    import '../app.css'
    import { page } from '$app/state'
    import { formatTime } from '$lib/format.js'

    let { children, data } = $props()

    const navItems = [
        { href: '/', label: 'Acquisition' },
        { href: '/product', label: 'Product' },
        { href: '/links', label: 'Link codes' },
    ]

    /** The relative ranges shown as picker buttons, in display order. `day` is set via the date input. */
    const rangeButtons = ['today', '24h', '7d', '30d'] as const

    /** The current pathname, used to keep the nav active state and to keep range switches on the same page. */
    const pathname = $derived(page.url.pathname)

    /** Hide the range/day picker where a time window has no meaning (the Link codes page). */
    const showPicker = $derived(pathname !== '/links')

    /** The freshness stamp, set by the active data page's load (absent on Link codes). */
    const updatedAt = $derived(page.data.updatedAt as string | undefined)

    /** True when a single specific UTC day is selected (vs a relative range). */
    const isDaySelected = $derived(data.selection.range === 'day')
    /** The selected specific day, or '' for the date input when a relative range is active. */
    const selectedDay = $derived(data.selection.day ?? '')

    /** Today's UTC day as YYYY-MM-DD, the max selectable day (no future days). */
    const todayIso = new Date().toISOString().slice(0, 10)

    /** Navigate to a relative range on the current page, clearing any specific-day selection. */
    function selectRange(range: string) {
        if (!(data.selection.range === range && !data.selection.day)) {
            window.location.href = `${pathname}?range=${range}`
        }
    }

    /** Navigate to a single specific UTC day on the current page (or back to the default range when cleared). */
    function selectDay(day: string) {
        window.location.href = day ? `${pathname}?day=${day}` : `${pathname}?range=7d`
    }
</script>

<div class="mx-auto max-w-[1800px] px-6 pb-8 pt-14">
    <!-- Header (sticky): brand, page nav, and the range/day picker -->
    <header
        class="fixed inset-x-0 top-0 z-40 flex items-center justify-between gap-4 border-b border-border bg-surface/90 px-6 py-2 backdrop-blur-sm"
    >
        <div class="flex items-center gap-6">
            <h1 class="text-lg font-bold text-text-primary">Cmdr analytics</h1>
            <nav class="flex items-center gap-1">
                {#each navItems as item}
                    {@const active = pathname === item.href}
                    <a
                        href={item.href}
                        aria-current={active ? 'page' : undefined}
                        class="rounded-md px-3 py-1 text-sm font-medium transition-colors
                            {active ? 'bg-accent text-accent-contrast' : 'text-text-secondary hover:text-text-primary'}"
                    >
                        {item.label}
                    </a>
                {/each}
            </nav>
        </div>

        {#if showPicker}
            <div class="flex items-center gap-3">
                <div class="flex rounded-lg border border-border bg-surface p-0.5">
                    {#each rangeButtons as r}
                        <button
                            onclick={() => selectRange(r)}
                            class="rounded-md px-3 py-1 text-sm font-medium transition-colors
                                {!isDaySelected && data.selection.range === r
                                ? 'bg-accent text-accent-contrast'
                                : 'text-text-secondary hover:text-text-primary'}"
                        >
                            {r}
                        </button>
                    {/each}
                </div>
                <input
                    type="date"
                    max={todayIso}
                    value={selectedDay}
                    onchange={(e) => selectDay((e.currentTarget as HTMLInputElement).value)}
                    aria-label="View a specific UTC day"
                    class="rounded-lg border px-2 py-1 text-sm transition-colors
                        {isDaySelected
                        ? 'border-accent bg-accent/10 text-text-primary'
                        : 'border-border bg-surface text-text-secondary hover:text-text-primary'}"
                />
                {#if updatedAt}
                    <span class="text-xs text-text-tertiary"> Updated {formatTime(updatedAt)} </span>
                {/if}
            </div>
        {/if}
    </header>

    {@render children()}
</div>
