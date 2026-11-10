<!--
  Shared shell for the whole dashboard: a sticky top bar with the page nav (Acquisition, Product,
  Licenses, Link codes) and the range picker (7d / 30d / 90d) beside the dates in view. The picker drives the
  shared time selection via `?range=` and stays on the current page when you switch, so Acquisition and Product
  share selection state. On Acquisition, ‹ and › step the window by its length (`?end=`, the window math in
  `#lib/time-range.ts`); everywhere else the window ends today. It's hidden on the two ledger pages (Licenses, Link codes), where a time range is
  irrelevant. The selection comes from the layout load; `updatedAt` comes from whichever data page is
  active.
-->
<script lang="ts">
    import '../app.css'
    import type { LayoutProps } from './$types'
    import { page } from '$app/state'
    import { resolve } from '$app/paths'
    import { formatTime } from '#lib/format.js'
    import {
        formatWindow,
        selectionQuery,
        stepEnd,
        timeRanges,
        viewWindow,
        type TimeRange,
    } from '#lib/time-range.js'

    const { children, data }: LayoutProps = $props()

    const navItems = [
        { href: resolve('/'), label: 'Acquisition' },
        { href: resolve('/product'), label: 'Product' },
        { href: resolve('/licenses'), label: 'Licenses' },
        { href: resolve('/links'), label: 'Link codes' },
    ]


    /** The current pathname, used to keep the nav active state and to keep range switches on the same page. */
    const pathname = $derived(page.url.pathname)

    /** Pages that are ledgers rather than time series, so a range/day picker means nothing on them. */
    const pathsWithoutPicker = ['/licenses', '/links']

    /** Hide the range/day picker where a time window has no meaning. */
    const showPicker = $derived(!pathsWithoutPicker.includes(pathname))

    /** The freshness stamp, set by the active data page's load (absent on Link codes). */
    const updatedAt = $derived(page.data.updatedAt as string | undefined)

    /** Pages whose sections step back through earlier windows (`?end=`); elsewhere every window ends today. */
    const pathsWithSteps = ['/']
    const canStep = $derived(pathsWithSteps.includes(pathname))

    /** Today's UTC day. A load re-renders the layout, so a page open across midnight catches up on the next step. */
    const today = new Date().toISOString().slice(0, 10)

    /** The past window's last day in view, or null for the window ending today. */
    const end = $derived(canStep ? data.selection.end : null)
    const shown = $derived(viewWindow(data.selection.range, end, today))
    const backEnd = $derived(stepEnd(data.selection.range, end, today, 'back'))
    const forwardEnd = $derived(stepEnd(data.selection.range, end, today, 'forward'))

    /** Navigate to a range and window on the current page, keeping the Acquisition page's `?country=` filter. */
    function go(range: TimeRange, nextEnd: string | null) {
        if (range === data.selection.range && nextEnd === end) return
        // Read the live URL: the country chips update it shallowly, which `page.url` doesn't follow.
        const country = new URL(window.location.href).searchParams.get('country')
        // Switching range keeps a past window's last day; `normalizeEnd` on the server pulls it up if a
        // longer range would start before the history.
        window.location.href = `${pathname}${selectionQuery({ range, end: canStep ? nextEnd : null }, country)}`
    }
</script>

<!-- Header (sticky, outside the width-capped container so it spans the viewport): brand, page nav, and the
     range/day picker. It wraps on narrow screens, and sticky (rather than fixed) lets the content flow below
     whatever height that gives, so nothing gets cut off or covered. Wrapped on a phone it's ~190px tall, so it
     only pins from `sm` up and scrolls away on a phone. -->
<header
    class="relative top-0 z-40 flex flex-wrap items-center justify-between gap-x-4 gap-y-2 border-b border-border bg-surface/90 px-6 py-2 backdrop-blur-sm sm:sticky"
>
        <div class="flex min-w-0 flex-wrap items-center gap-x-6 gap-y-1">
            <h1 class="text-lg font-bold text-text-primary">Cmdr analytics</h1>
            <nav class="flex flex-wrap items-center gap-1">
                {#each navItems as item (item.href)}
                    {@const active = pathname === item.href}
                    <!-- eslint-disable svelte/no-navigation-without-resolve -- every `navItems` href is built with resolve() above; the rule can't follow it through the array -->
                    <a
                        href={item.href}
                        aria-current={active ? 'page' : undefined}
                        class="rounded-md px-3 py-1 text-sm font-medium transition-colors
                            {active ? 'bg-accent text-accent-contrast' : 'text-text-secondary hover:text-text-primary'}"
                    >
                        {item.label}
                    </a>
                    <!-- eslint-enable svelte/no-navigation-without-resolve -->
                {/each}
            </nav>
        </div>

        {#if showPicker}
            <div class="flex flex-wrap items-center gap-x-3 gap-y-2">
                <div class="flex items-center gap-1">
                    {#if canStep}
                        <button
                            onclick={() => { if (backEnd !== undefined) go(data.selection.range, backEnd); }}
                            disabled={backEnd === undefined}
                            aria-label="Previous {data.selection.range}"
                            title="Previous {data.selection.range}"
                            class="rounded-md px-2.5 py-0.5 text-lg leading-none font-medium text-text-secondary transition-colors
                                hover:text-text-primary disabled:cursor-not-allowed disabled:opacity-40
                                disabled:hover:text-text-secondary"
                        >
                            ‹
                        </button>
                    {/if}
                    <span class="text-sm whitespace-nowrap text-text-secondary tabular-nums" aria-live="polite">
                        {formatWindow(shown)}
                    </span>
                    {#if canStep}
                        <button
                            onclick={() => { if (forwardEnd !== undefined) go(data.selection.range, forwardEnd); }}
                            disabled={forwardEnd === undefined}
                            aria-label="Next {data.selection.range}"
                            title="Next {data.selection.range}"
                            class="rounded-md px-2.5 py-0.5 text-lg leading-none font-medium text-text-secondary transition-colors
                                hover:text-text-primary disabled:cursor-not-allowed disabled:opacity-40
                                disabled:hover:text-text-secondary"
                        >
                            ›
                        </button>
                    {/if}
                </div>
                <div class="flex rounded-lg border border-border bg-surface p-0.5">
                    {#each timeRanges as r (r)}
                        <button
                            onclick={() => { go(r, end); }}
                            aria-pressed={data.selection.range === r}
                            class="rounded-md px-3 py-1 text-sm font-medium transition-colors
                                {data.selection.range === r
                                ? 'bg-accent text-accent-contrast'
                                : 'text-text-secondary hover:text-text-primary'}"
                        >
                            {r}
                        </button>
                    {/each}
                </div>
                {#if updatedAt}
                    <span class="text-xs text-text-tertiary"> Updated {formatTime(updatedAt)} </span>
                {/if}
            </div>
        {/if}
</header>

<div class="mx-auto max-w-[1800px] px-6 pt-4 pb-8">
    {@render children()}
</div>
