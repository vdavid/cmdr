/**
 * Module-level slot for the most recently auto-sent report ID (Flow B). The
 * auto-send listener sets the ID right before `addToast(...)` so the rendered
 * toast can read it without prop bridging. Read it reactively via
 * `getLastAutoSentReportId()`.
 *
 * Lives in a `.svelte.ts` module so its types resolve across imports; a
 * `.svelte` module export is seen as `any`.
 */
let lastAutoSentReportId = $state('')

export function setLastAutoSentReportId(id: string): void {
  lastAutoSentReportId = id
}

export function getLastAutoSentReportId(): string {
  return lastAutoSentReportId
}
