/**
 * Domain types and pure aggregation helpers for in-app feedback and error reports. Lives outside
 * `$lib/server` so both the page (client bundle) and the report/server code can import the helpers;
 * the server-only fetching lives in `$lib/server/sources/feedback-and-errors.ts`.
 */

/** One in-app "Send feedback" submission. `email` is the optional reply-to the sender attached. */
export interface FeedbackRow {
  id: number
  createdAt: string
  feedback: string
  email: string | null
  appVersion: string
  osVersion: string
  buildMode: string | null
}

/** One error-report bundle's metadata (the zip itself stays in R2). `kind` is "auto" or "user". */
export interface ErrorReportRow {
  id: string
  kind: string
  appVersion: string
  osVersion: string
  arch: string
  date: string
  generatedAt: string
}

/** Number of feedback messages that carry a reply-to email (people awaiting a response). */
export function countFeedbackWithReplyTo(rows: FeedbackRow[]): number {
  return rows.filter((row) => row.email != null && row.email !== '').length
}

/** Tallies error reports by a field (kind/version/arch), highest count first. */
export function tallyErrorReportsByField(
  rows: ErrorReportRow[],
  field: 'kind' | 'appVersion' | 'arch',
): Array<{ key: string; count: number }> {
  const counts = new Map<string, number>()
  for (const row of rows) {
    const key = row[field] || '(unknown)'
    counts.set(key, (counts.get(key) ?? 0) + 1)
  }
  return [...counts.entries()].map(([key, count]) => ({ key, count })).sort((a, b) => b.count - a.count)
}

/** Error reports grouped by day, oldest first (for the timeline chart). */
export function errorReportsByDay(rows: ErrorReportRow[]): Array<{ date: string; count: number }> {
  const counts = new Map<string, number>()
  for (const row of rows) {
    counts.set(row.date, (counts.get(row.date) ?? 0) + 1)
  }
  return [...counts.entries()].map(([date, count]) => ({ date, count })).sort((a, b) => a.date.localeCompare(b.date))
}
