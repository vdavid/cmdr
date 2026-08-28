/**
 * Module-level slot for the report the post-send toast talks about. `ErrorReportDialog`
 * calls `setLastSentReport({ id, kind })` right before `addToast(ErrorReportToastContent, ...)`
 * so the toast can render both the ID and the right sentence without prop bridging (the
 * toast system mounts components with no props). Read them reactively via the getters.
 *
 * `kind` is what keeps the two outcomes honest: `sent` shipped a new report, `amended`
 * added a note to the one Flow B had already sent. One field, set with the id in one
 * call, so the pair can't drift.
 *
 * Lives in a `.svelte.ts` module (not the toast's `<script module>`) so its
 * types resolve across imports; a `.svelte` module export is seen as `any`.
 */

/** `sent`: a new report shipped. `amended`: a note joined the auto-sent report. */
export type SentReportKind = 'sent' | 'amended'

interface LastSentReport {
  id: string
  kind: SentReportKind
}

let lastSentReport = $state<LastSentReport>({ id: '', kind: 'sent' })

export function setLastSentReport(report: LastSentReport): void {
  lastSentReport = report
}

export function getLastSentReportId(): string {
  return lastSentReport.id
}

export function getLastSentReportKind(): SentReportKind {
  return lastSentReport.kind
}
