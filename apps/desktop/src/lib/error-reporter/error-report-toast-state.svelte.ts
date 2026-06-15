/**
 * Module-level slot for the most recently sent report ID. `ErrorReportDialog`
 * calls `setLastSentReportId(id)` right before `addToast(ErrorReportToastContent, ...)`
 * so the toast can render the ID without prop bridging (the toast system mounts
 * components with no props). Read it reactively via `getLastSentReportId()`.
 *
 * Lives in a `.svelte.ts` module (not the toast's `<script module>`) so its
 * types resolve across imports; a `.svelte` module export is seen as `any`.
 */
let lastSentReportId = $state('')

export function setLastSentReportId(id: string): void {
  lastSentReportId = id
}

export function getLastSentReportId(): string {
  return lastSentReportId
}
