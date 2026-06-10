/**
 * Single entry point for opening the "Send feedback" dialog.
 *
 * The Help menu item ("Send feedback…") and the `feedback.send` palette command both
 * call `openFeedbackDialog()`. The dialog component reads from the exported reactive
 * `feedbackFlow` state and renders itself only when `open` is true.
 *
 * The actual mounting happens in `(main)/+layout.svelte`, next to `ErrorReportDialog`
 * (same pattern), which keeps focus/Escape handling consistent.
 */

import { recordBreadcrumb } from '$lib/error-reporter/breadcrumbs'

interface FlowState {
  open: boolean
}

export const feedbackFlow = $state<FlowState>({
  open: false,
})

export function openFeedbackDialog(): void {
  feedbackFlow.open = true
  recordBreadcrumb('feedback', 'dialog-opened')
}

export function closeFeedbackDialog(): void {
  feedbackFlow.open = false
  recordBreadcrumb('feedback', 'dialog-closed')
}
