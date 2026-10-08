/**
 * What an open that didn't succeed shows and logs. The viewer's three open sites (mount,
 * Retry, and the view-as-text / view-as-media reopen) all go through `handleOpenFailure`,
 * so they render one consistent, per-variant message.
 */

import { asViewerError, type ViewerError } from '$lib/tauri-commands'
import { tString } from '$lib/intl/messages.svelte'
import type { Logger } from '$lib/logging/logger'

export interface OpenFailure {
  message: string
  canRetry: boolean
}

/**
 * Maps a caught viewer-open failure to the display copy + whether Retry applies. Every
 * branch reads the typed `ViewerError`; anything that never reached the typed path at all
 * reads as the generic copy rather than as the backend's own English.
 */
function openFailureCopy(e: unknown): OpenFailure {
  const ve = asViewerError(e)
  if (ve) {
    if (ve.kind === 'timedOut') return { message: tString('viewer.error.timeout'), canRetry: true }
    if (ve.kind === 'stoppedResponding') {
      return { message: tString('viewer.error.stoppedResponding'), canRetry: true }
    }
    if (ve.kind === 'notFound') return { message: tString('viewer.error.notFound'), canRetry: false }
    if (ve.kind === 'isDirectory') return { message: tString('viewer.error.isDirectory'), canRetry: false }
    if (ve.kind === 'tooLargeToPreview') return { message: tString('viewer.error.tooLargeToPreview'), canRetry: false }
    if (ve.kind === 'archive') return { message: tString('viewer.error.archiveUnreadable'), canRetry: false }
    if (ve.kind === 'coldStorage') return { message: tString('viewer.error.coldStorage'), canRetry: false }
  }
  return { message: tString('viewer.error.readFailed'), canRetry: true }
}

/**
 * Whether a typed failure is the world's doing, which the window renders with a way forward
 * (`warn`), or can't reach an open unless our own code is wrong (`error`). An error log counts
 * toward an auto-sent error report, so only the second kind files one. Exhaustive on purpose:
 * a new `ViewerError` variant doesn't compile here until someone decides which it is.
 *
 * `cancelled` means the window closed mid-pull or the read was stopped on purpose; `io` is
 * what the OS or the source refused (permission denied, a disk read error, a phone or a
 * repository that dropped mid-read). The error side can't reach an open at all: no live
 * session, a line past the end, a save-only refusal.
 */
function logLevelFor(ve: ViewerError): 'warn' | 'error' {
  switch (ve.kind) {
    case 'timedOut':
    case 'stoppedResponding':
    case 'notFound':
    case 'isDirectory':
    case 'tooLargeToPreview':
    case 'archive':
    case 'coldStorage':
    case 'cancelled':
    case 'io':
      return 'warn'
    case 'sessionNotFound':
    case 'outOfRange':
    case 'destinationIsReadOnly':
      return 'error'
    default: {
      const unhandled: never = ve
      return unhandled
    }
  }
}

/**
 * Logs a failed open (`action` names which site) and returns what the window shows for it.
 * A failure that never reached the typed path at all logs at error too: that one is a defect.
 */
export function handleOpenFailure(log: Logger, action: string, e: unknown): OpenFailure {
  const ve = asViewerError(e)
  if (ve && logLevelFor(ve) === 'warn') {
    log.warn('{action} failed: {error}', { action, error: String(e) })
  } else {
    log.error('{action} failed: {error}', { action, error: String(e) })
  }
  return openFailureCopy(e)
}
