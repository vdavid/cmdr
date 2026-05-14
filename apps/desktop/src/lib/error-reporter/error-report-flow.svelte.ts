/**
 * Single entry point for opening the error-report dialog.
 *
 * Both the Help menu item ("Send error report…") and the inline button on error toasts
 * call `openErrorReportDialog(initialNote?)`. The dialog component reads from the
 * exported reactive `errorReportFlow` state and renders itself only when `open` is true.
 *
 * The actual mounting happens in `(main)/+layout.svelte`; keeping the dialog mounted
 * once at layout level matches how `CrashReportDialog` works and ensures consistent
 * focus/Escape handling.
 */

import { recordBreadcrumb } from './breadcrumbs'

interface FlowState {
  open: boolean
  initialNote: string
}

export const errorReportFlow = $state<FlowState>({
  open: false,
  initialNote: '',
})

/**
 * Open the error-report preview dialog. If `initialNote` is provided, it pre-fills the
 * note textarea, used by the toast button to ferry the toast message into the report.
 */
export function openErrorReportDialog(initialNote?: string): void {
  errorReportFlow.initialNote = initialNote ?? ''
  errorReportFlow.open = true
  recordBreadcrumb('error-report', 'dialog-opened', initialNote ? { hasInitialNote: true } : undefined)
}

export function closeErrorReportDialog(): void {
  errorReportFlow.open = false
  errorReportFlow.initialNote = ''
  recordBreadcrumb('error-report', 'dialog-closed')
}
