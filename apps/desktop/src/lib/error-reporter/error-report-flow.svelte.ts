/**
 * Single entry point for opening the error-report dialog, in either of its two modes.
 *
 * `openErrorReportDialog(initialNote?)` is the compose path: the Help menu item ("Send
 * error report…") and the inline button on error toasts both call it.
 * `openErrorReportDialogForAutoSentReport()` is the amend path, reached only from the
 * Flow B auto-sent toast: same dialog, but it shows the report that already shipped and
 * adds the user's note to THAT report instead of uploading a second one.
 *
 * The mode lives here rather than in `openErrorReportDialog`'s signature so the ten
 * compose call sites stay untouched. The dialog component reads from the exported
 * reactive `errorReportFlow` state and renders itself only when `open` is true.
 *
 * The actual mounting happens in `(main)/+layout.svelte`; keeping the dialog mounted
 * once at layout level matches how `CrashReportDialog` works and ensures consistent
 * focus/Escape handling.
 */

import { recordBreadcrumb } from './breadcrumbs'

/** `compose` builds and uploads a new report; `amend` adds a note to the auto-sent one. */
export type ErrorReportMode = 'compose' | 'amend'

interface FlowState {
  open: boolean
  initialNote: string
  mode: ErrorReportMode
}

export const errorReportFlow = $state<FlowState>({
  open: false,
  initialNote: '',
  mode: 'compose',
})

/**
 * Open the error-report preview dialog in compose mode. If `initialNote` is provided, it
 * pre-fills the note textarea, used by the toast button to ferry the toast message into
 * the report.
 */
export function openErrorReportDialog(initialNote?: string): void {
  errorReportFlow.initialNote = initialNote ?? ''
  errorReportFlow.mode = 'compose'
  errorReportFlow.open = true
  recordBreadcrumb('error-report', 'dialog-opened', initialNote ? { hasInitialNote: true } : undefined)
}

/**
 * Open the dialog on the report Flow B already auto-sent, so a note joins THAT report.
 * Nothing here can upload a second bundle: the dialog's amend mode calls
 * `amendErrorReport`, never `sendErrorReport`.
 */
export function openErrorReportDialogForAutoSentReport(): void {
  errorReportFlow.initialNote = ''
  errorReportFlow.mode = 'amend'
  errorReportFlow.open = true
  recordBreadcrumb('error-report', 'amend-dialog-opened')
}

export function closeErrorReportDialog(): void {
  errorReportFlow.open = false
  errorReportFlow.initialNote = ''
  errorReportFlow.mode = 'compose'
  recordBreadcrumb('error-report', 'dialog-closed')
}
