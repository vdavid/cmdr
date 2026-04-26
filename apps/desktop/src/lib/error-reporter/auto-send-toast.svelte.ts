/**
 * Listens for the Tauri `error-report-auto-sent` event and renders a confirmation
 * toast. Mounted once at app boot from `(main)/+layout.svelte`, alongside the Flow A
 * dialog. Single responsibility: bridge the backend event into the toast system.
 *
 * The toast auto-dismisses after 10 seconds (longer than the default 4 s — auto-sent
 * reports are surprising; the user needs time to notice and act).
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { addToast } from '$lib/ui/toast'

import AutoSendToastContent, { setLastAutoSentReportId } from './AutoSendToastContent.svelte'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('errorReporter')

const TOAST_ID = 'error-report-auto-sent'
const TOAST_TIMEOUT_MS = 10_000

/** Backend payload: just the server-issued report ID as a JSON string. */
type AutoSentPayload = string

let unlisten: UnlistenFn | undefined

/**
 * Start listening for `error-report-auto-sent` events. Idempotent — repeated calls
 * are no-ops (the second listener would just dedup via the toast `id` slot, but we
 * skip the work entirely).
 */
export async function initAutoSendToastListener(): Promise<void> {
  if (unlisten) {
    log.debug('Auto-send toast listener already initialized')
    return
  }
  unlisten = await listen<AutoSentPayload>('error-report-auto-sent', (event) => {
    const reportId = event.payload
    log.info('Error report auto-sent: {id}', { id: reportId })
    // eslint-disable-next-line @typescript-eslint/no-unsafe-call -- Svelte module export type not resolved
    setLastAutoSentReportId(reportId)
    addToast(AutoSendToastContent, {
      id: TOAST_ID,
      level: 'info',
      dismissal: 'transient',
      timeoutMs: TOAST_TIMEOUT_MS,
    })
  })
}

/** Stop listening. Called from the layout's `onDestroy`. */
export function cleanupAutoSendToastListener(): void {
  if (unlisten) {
    unlisten()
    unlisten = undefined
  }
}
