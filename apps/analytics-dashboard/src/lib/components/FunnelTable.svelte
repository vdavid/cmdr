<!--
  The "Daily funnel" section: a full-width table, one row per UTC day for the last 30 days (newest
  first), plus the "Channels" breakdown rolled up over the window. Always the last 30 days, independent
  of the range picker. Clicking a row filters the rest of the dashboard to that day via `onselectday`.
-->
<script lang="ts">
    import type { SourceResult } from '$lib/server/types.js'
    import type { FunnelData } from '$lib/server/sources/funnel.js'
    import { aggregateChannels, aggregateReferers, aggregateUaFamilies } from '$lib/funnel.js'
    import { formatNumber } from '$lib/format.js'
    import ErrorState from './ErrorState.svelte'
    import MetricTable from './MetricTable.svelte'
    import Methodology from './Methodology.svelte'
    import SectionDescription from './SectionDescription.svelte'
    import type { DashboardSelection } from '$lib/server/types.js'

    let {
        funnel,
        selection,
        selectedDay,
        isDaySelected,
        todayIso,
        onselectday,
    }: {
        funnel: SourceResult<FunnelData>
        selection: DashboardSelection
        selectedDay: string
        isDaySelected: boolean
        todayIso: string
        onselectday: (day: string) => void
    } = $props()

    /** Format a funnel cell: a real number, or an en dash when the value is unknown (null). */
    function funnelCell(value: number | null): string {
        return value === null ? '–' : formatNumber(value)
    }

    /** Format a D7 retention fraction as a percent, or an en dash when unknown (null / young cohort). */
    function funnelPercent(fraction: number | null): string {
        return fraction === null ? '–' : `${Math.round(fraction * 100)}%`
    }
</script>

<section class="mb-6 rounded-xl border border-border bg-surface p-6">
    <div class="mb-1">
        <h2 class="text-lg font-semibold text-text-primary">Daily funnel</h2>
        <p class="text-sm text-text-tertiary">The last 30 days, one row per UTC day, newest first.</p>
    </div>
    <SectionDescription
        insight={'Use this to watch the whole acquisition path day by day: site visitors, download clicks, real server ' +
            'downloads, new installs, week-one retention, signups, and purchases, all lined up so you can spot where ' +
            'a day fell off.'}
        caveat={"All days are UTC, and today's row is partial (it's still going). A dash means we couldn't get that cell " +
            '(not a zero). Click a day to filter the sections below to it.'}
    />
    {#if !funnel.ok}
        <ErrorState error={funnel.error} {selection} />
    {:else}
        {@const rows = funnel.data.rows}
        <div class="overflow-x-auto">
            <table class="w-full text-sm">
                <thead>
                    <tr class="border-b border-border text-left text-xs text-text-tertiary">
                        <th class="py-2 pr-4 font-medium">Day</th>
                        <th class="py-2 pr-4 text-right font-medium">Visitors</th>
                        <th class="py-2 pr-4 text-right font-medium">Download clicks</th>
                        <th class="py-2 pr-4 text-right font-medium">Server downloads</th>
                        <th class="py-2 pr-4 text-right font-medium">New installs</th>
                        <th class="py-2 pr-4 text-right font-medium">D7 retained</th>
                        <th class="py-2 pr-4 text-right font-medium">Signups</th>
                        <th class="py-2 text-right font-medium">Purchases</th>
                    </tr>
                </thead>
                <tbody>
                    {#each [...rows].reverse() as row (row.date)}
                        {@const isToday = row.date === todayIso}
                        {@const isActiveDay = isDaySelected && row.date === selectedDay}
                        <tr
                            class="cursor-pointer border-b border-border-subtle transition-colors hover:bg-surface-elevated
                                {isActiveDay ? 'bg-accent/10' : ''}"
                            onclick={() => onselectday(row.date)}
                        >
                            <td class="py-1.5 pr-4 font-medium text-text-primary">
                                {row.date}{#if isToday}<span class="ml-1 text-xs font-normal text-text-tertiary"
                                        >(today, partial)</span
                                    >{/if}
                            </td>
                            <td class="py-1.5 pr-4 text-right tabular-nums text-text-secondary">{funnelCell(row.visitors)}</td>
                            <td class="py-1.5 pr-4 text-right tabular-nums text-text-secondary">{funnelCell(row.downloadClicks)}</td>
                            <td class="py-1.5 pr-4 text-right tabular-nums text-text-secondary">{funnelCell(row.serverDownloads)}</td>
                            <td class="py-1.5 pr-4 text-right tabular-nums text-text-secondary">{funnelCell(row.newInstalls)}</td>
                            <td class="py-1.5 pr-4 text-right tabular-nums text-text-secondary">
                                {funnelPercent(row.d7Retention)}{#if row.d7Retained !== null}<span
                                        class="ml-1 text-xs text-text-tertiary">({row.d7Retained})</span
                                    >{/if}
                            </td>
                            <td class="py-1.5 pr-4 text-right tabular-nums text-text-secondary">{funnelCell(row.newsletterSignups)}</td>
                            <td class="py-1.5 text-right tabular-nums text-text-secondary">{funnelCell(row.purchases)}</td>
                        </tr>
                    {/each}
                </tbody>
            </table>
        </div>
        <Methodology
            text={'Visitors and download clicks come from Umami (cookieless, in-browser). Server downloads, new installs, ' +
                "DAU, and D7 come from the app's own telemetry (D1); signups from Listmonk; purchases from Paddle. " +
                "Clicks and server downloads won't match: server downloads also include Homebrew, direct links, and " +
                'GitHub-page traffic, and bot user agents are filtered but imperfectly. D7 needs a cohort at least 8 ' +
                'days old, so recent rows show a dash there.'}
        />

        <!-- Channels: server downloads rolled up by first-touch ref over the whole 30-day window. -->
        {@const channels = aggregateChannels(rows)}
        <div class="mt-6 border-t border-border-subtle pt-4">
            <h3 class="mb-1 text-sm font-medium text-text-secondary">Channels (last 30 days)</h3>
            <SectionDescription
                insight={"Use this to see which channels drove downloads: a download's ref is the channel the visitor first " +
                    'arrived from (a UTM source or campaign, else the referring site).'}
                caveat={'Ref is first-touch per browser visit and comes from the URL only, so return visits and cross-device ' +
                    'journeys (read on the phone, download on the Mac) carry no ref and land in "(none)". Homebrew and ' +
                    'direct links have none too, and rows before 2026-06-12 predate the column. So treat "(none)" as ' +
                    '"channel unknown", not "direct". All days UTC.'}
            />
            {#if channels.length === 0}
                <p class="text-sm text-text-tertiary">No downloads with a channel yet.</p>
            {:else}
                <MetricTable
                    items={channels.map((c) => ({ x: c.ref === '(none)' ? '(none / unknown)' : c.ref, y: c.count }))}
                    colLabel="Channel"
                    colValue="Downloads"
                />
            {/if}
        </div>

        <!-- Download referrers: the raw Referer host of each /download hit, captured on every hit (not
             just the website button), so it reveals where the direct, no-ref downloads came from. -->
        {@const referers = aggregateReferers(rows)}
        <div class="mt-6 border-t border-border-subtle pt-4">
            <h3 class="mb-1 text-sm font-medium text-text-secondary">Download referrers (last 30 days)</h3>
            <SectionDescription
                insight={'The Referer host of each /download request. Unlike the first-touch channel above (set only ' +
                    'by the website button), this is captured on every hit, so it shows where the direct, no-ref ' +
                    'downloads came from: a link on AlternativeTo, a directory, GitHub, Reddit, or a forum.'}
                caveat={'"(none)" means no usable referer: a typed URL, a privacy browser, a referrer-policy strip, ' +
                    'Homebrew/curl, or rows before 2026-06-25 that predate the column. A download can appear both ' +
                    'here (by referer) and above (by ref); the two breakdowns count the same downloads differently. ' +
                    'All days UTC.'}
            />
            {#if referers.length === 0}
                <p class="text-sm text-text-tertiary">No downloads with a referer yet.</p>
            {:else}
                <MetricTable
                    items={referers.map((r) => ({ x: r.ref === '(none)' ? '(none / unknown)' : r.ref, y: r.count }))}
                    colLabel="Referrer"
                    colValue="Downloads"
                />
            {/if}
        </div>

        <!-- Downloads by client: the /download User-Agent family. Cmdr is macOS-only, so a non-macOS UA
             can't be a real install; "Human installs" excludes only those, keeping the ambiguous ones. -->
        {@const ua = aggregateUaFamilies(rows)}
        <div class="mt-6 border-t border-border-subtle pt-4">
            <h3 class="mb-1 text-sm font-medium text-text-secondary">Downloads by client (last 30 days)</h3>
            <SectionDescription
                insight={'Cmdr is macOS-only, so a non-macOS client downloading the .dmg literally cannot install it. ' +
                    '"Human installs" is the raw server downloads minus those provably-impossible bot hits, so the ' +
                    'headline stops reading as half noise. It keeps every ambiguous download (we only drop the ' +
                    'clearly-fake ones).'}
                caveat={'The scraper spoofs Mac browser UAs too (many from China), so "human" means "could be a real ' +
                    'install", not proof of one, and is NOT a clean count. "unknown" (no or unrecognized UA, including ' +
                    'rows before UA capture) stays counted because we can\'t tell. Only the bot row is the ' +
                    'high-confidence exclusion. All days UTC.'}
            />
            {#if ua.total === 0}
                <p class="text-sm text-text-tertiary">No downloads to classify yet.</p>
            {:else}
                <p class="mb-2 text-sm text-text-secondary">
                    Human installs:
                    <span class="font-semibold tabular-nums text-text-primary">{formatNumber(ua.humanInstalls)}</span>
                    <span class="text-text-tertiary">of {formatNumber(ua.total)} raw server downloads</span>
                </p>
                <MetricTable
                    items={[
                        { x: 'human (Mac, Homebrew, curl/wget)', y: ua.human },
                        { x: 'bot / impossible (Windows, Android, Linux, X11)', y: ua.bot },
                        { x: 'unknown (no / unrecognized UA)', y: ua.unknown },
                    ]}
                    colLabel="Client"
                    colValue="Downloads"
                />
            {/if}
        </div>
    {/if}
</section>
