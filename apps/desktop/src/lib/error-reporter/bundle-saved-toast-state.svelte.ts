/**
 * Module-level slot for the path of the most recently saved debug bundle.
 * `ErrorReportDialog` sets this right before `addToast(BundleSavedToastContent, ...)`.
 * Same prop-bridging pattern as `error-report-toast-state`. Read it reactively
 * via `getLastSavedBundlePath()`.
 *
 * Lives in a `.svelte.ts` module so its types resolve across imports; a
 * `.svelte` module export is seen as `any`.
 */
let lastSavedBundlePath = $state('')

export function setLastSavedBundlePath(path: string): void {
  lastSavedBundlePath = path
}

export function getLastSavedBundlePath(): string {
  return lastSavedBundlePath
}
