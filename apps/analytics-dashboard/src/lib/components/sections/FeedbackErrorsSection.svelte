<!--
  Product § Feedback & errors: in-app feedback messages and error-report bundle metadata. Shows error
  reports per day, by kind, and by version, plus recent feedback and a recent-error-reports table. Each
  feedback message with a reply-to email has a mark-answered toggle (the `setAnswered` action on `/product`).
  A message sent with logs names their `ERR-XXXXX`: a link to that row of the recent-error-reports table when it's
  listed there, else the id as selectable text (older, or outside the table's 15 rows).
-->
<script lang="ts">
    import type { SourceResult, DashboardSelection } from '#lib/server/types.js'
    import type { FeedbackAndErrorsData } from '#lib/server/sources/feedback-and-errors.js'
    import { enhance } from '$app/forms'
    import { countFeedbackAwaitingReply, tallyErrorReportsByField, errorReportsByDay } from '#lib/feedback-and-errors.js'
    import { formatNumber } from '#lib/format.js'
    import { COLOR_GOLD } from '#lib/colors.js'
    import Chart from '#lib/components/Chart.svelte'
    import ErrorState from '#lib/components/ErrorState.svelte'
    import MetricRow from '#lib/components/MetricRow.svelte'
    import MetricTable from '#lib/components/MetricTable.svelte'
    import EmptyState from '#lib/components/EmptyState.svelte'
    import SectionDescription from '#lib/components/SectionDescription.svelte'
    import ExternalLinks from '#lib/components/ExternalLinks.svelte'

    const {
        feedbackAndErrors,
        selection,
        answerError = null,
    }: {
        feedbackAndErrors: SourceResult<FeedbackAndErrorsData>
        selection: DashboardSelection
        /** The mark-answered action's failure message, if the last toggle didn't save. */
        answerError?: string | null
    } = $props()
</script>

<section class="rounded-xl border border-border bg-surface p-6">
    <div class="mb-1">
        <h2 class="text-lg font-semibold text-text-primary">Feedback &amp; errors</h2>
        <p class="text-sm text-text-tertiary">What are users telling us?</p>
    </div>
    <SectionDescription
        insight={'Use this to read what people sent through in-app feedback and to see error-report bundles roll in, so you ' +
            'catch pain points fast.'}
        caveat={"Feedback carries no install id (it's unjoinable to analytics), and error reports use a separate diagnostics " +
            'id. Both are low-volume, so a short window can look empty even when things are fine.'}
    />

    {#if feedbackAndErrors.ok}
        {@const fe = feedbackAndErrors.data}
        {@const awaitingReply = countFeedbackAwaitingReply(fe.feedback)}
        {@const errorsPerDay = errorReportsByDay(fe.errorReports)}
        {@const errorsByKind = tallyErrorReportsByField(fe.errorReports, 'kind')}
        {@const errorsByVersion = tallyErrorReportsByField(fe.errorReports, 'appVersion')}
        {@const recentReports = fe.errorReports.slice(0, 15)}

        <MetricRow
            metrics={[
                { label: 'Feedback messages', value: formatNumber(fe.feedback.length), color: COLOR_GOLD },
                { label: 'Awaiting reply', value: formatNumber(awaitingReply) },
                { label: 'Error reports', value: formatNumber(fe.errorReports.length) },
            ]}
        />

        {#if errorsPerDay.length > 1}
            <div class="mt-4">
                <h3 class="mb-2 text-sm font-medium text-text-secondary">Error reports per day</h3>
                <Chart
                    data={[errorsPerDay.map((d) => new Date(d.date).getTime() / 1000), errorsPerDay.map((d) => d.count)]}
                    labels={['Error reports']}
                    colors={[COLOR_GOLD]}
                    height={160}
                />
            </div>
        {/if}

        {#if fe.errorReports.length > 0}
            <div class="mt-4 grid gap-4 md:grid-cols-2">
                <div>
                    <h3 class="mb-2 text-sm font-medium text-text-secondary">By kind</h3>
                    <MetricTable
                        items={errorsByKind.map((e) => ({ x: e.key, y: e.count }))}
                        colLabel="Kind"
                        colValue="Reports"
                    />
                </div>
                <div>
                    <h3 class="mb-2 text-sm font-medium text-text-secondary">By version</h3>
                    <MetricTable
                        items={errorsByVersion.map((e) => ({ x: e.key, y: e.count }))}
                        colLabel="Version"
                        colValue="Reports"
                    />
                </div>
            </div>
        {/if}

        {#if fe.feedback.length > 0}
            <div class="mt-4">
                <h3 class="mb-2 text-sm font-medium text-text-secondary">Recent feedback</h3>
                {#if answerError}
                    <p class="mb-2 text-sm text-danger">{answerError}</p>
                {/if}
                <div class="space-y-2">
                    {#each fe.feedback.slice(0, 15) as msg (msg.id)}
                        <div class="rounded-lg border border-border-subtle bg-surface-elevated px-3 py-2">
                            <p class="text-sm whitespace-pre-wrap text-text-primary">{msg.feedback}</p>
                            <div class="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-text-tertiary">
                                <span class="tabular-nums">{msg.createdAt.split(' ')[0]}</span>
                                <span>v{msg.appVersion}</span>
                                <span>{msg.osVersion}</span>
                                {#if msg.errorReportId}
                                    {#if recentReports.some((report) => report.id === msg.errorReportId)}
                                        <a class="text-accent hover:text-accent-hover" href="#{msg.errorReportId}">
                                            Logs <span class="font-mono">{msg.errorReportId}</span>
                                        </a>
                                    {:else}
                                        <span>Logs <span class="font-mono select-all">{msg.errorReportId}</span></span>
                                    {/if}
                                {/if}
                                {#if msg.email}
                                    <a
                                        class="text-accent hover:text-accent-hover"
                                        href="mailto:{msg.email}?subject=Re%3A%20your%20Cmdr%20feedback"
                                    >
                                        Reply to {msg.email}
                                    </a>
                                    {#if msg.answeredAt}
                                        <span class="text-text-secondary">
                                            Answered <span class="tabular-nums">{msg.answeredAt.split(' ')[0]}</span>
                                        </span>
                                    {/if}
                                    <form
                                        method="POST"
                                        action="?/setAnswered"
                                        use:enhance={() =>
                                            ({ update }) =>
                                                update({ reset: false })}
                                    >
                                        <input type="hidden" name="id" value={msg.id} />
                                        <input type="hidden" name="answered" value={msg.answeredAt ? 'false' : 'true'} />
                                        <button
                                            type="submit"
                                            class="rounded-md border border-border px-2 py-0.5 text-xs text-text-secondary
                                                transition-colors hover:text-text-primary"
                                        >
                                            {msg.answeredAt ? 'Mark as awaiting reply' : 'Mark answered'}
                                        </button>
                                    </form>
                                {/if}
                            </div>
                        </div>
                    {/each}
                </div>
            </div>
        {:else}
            <EmptyState />
        {/if}

        {#if fe.errorReports.length > 0}
            <div class="mt-4">
                <h3 class="mb-2 text-sm font-medium text-text-secondary">Recent error reports</h3>
                <div class="overflow-x-auto">
                    <table class="w-full text-left text-sm">
                        <thead>
                            <tr class="border-b border-border-subtle text-text-tertiary">
                                <th class="pr-4 pb-2 font-medium">ID</th>
                                <th class="pr-4 pb-2 font-medium">Kind</th>
                                <th class="pr-4 pb-2 font-medium">Version</th>
                                <th class="pb-2 text-right font-medium">Date</th>
                            </tr>
                        </thead>
                        <tbody>
                            {#each recentReports as report (report.id)}
                                <tr id={report.id} class="scroll-mt-32 border-b border-border-subtle/50 target:bg-accent/10">
                                    <td class="py-1.5 pr-4 font-mono text-text-primary">{report.id}</td>
                                    <td class="py-1.5 pr-4 text-text-secondary">{report.kind}</td>
                                    <td class="py-1.5 pr-4 text-text-secondary tabular-nums">{report.appVersion}</td>
                                    <td class="py-1.5 text-right text-text-tertiary tabular-nums">{report.date}</td>
                                </tr>
                            {/each}
                        </tbody>
                    </table>
                </div>
            </div>
        {/if}

        <ExternalLinks links={[{ label: 'View bundles in Cloudflare R2', href: 'https://dash.cloudflare.com' }]} />
    {:else}
        <ErrorState error={feedbackAndErrors.error} {selection} />
    {/if}
</section>
