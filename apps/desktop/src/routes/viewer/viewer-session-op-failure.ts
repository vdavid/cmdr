/**
 * What a session op that didn't take says and logs: reloading the file, switching tail mode,
 * or switching the encoding. Each reads the backend's typed `ViewerError` (the timeout is its
 * own kind), ❌ never its message, so "the disk is slow" and "the file is gone" word apart.
 */

import type { ViewerError } from '#lib/tauri-commands/index.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import type { Logger } from '#lib/logging/logger.js'
import { logLevelFor } from './viewer-open-failure'

export type SessionOp = 'reload' | 'tailOn' | 'tailOff' | 'encoding'

const genericKey: Record<SessionOp, MessageKey> = {
  reload: 'viewer.reloadToast.couldNotReload',
  tailOn: 'viewer.tail.couldNotTurnOn',
  tailOff: 'viewer.tail.couldNotTurnOff',
  encoding: 'viewer.encoding.couldNotSwitch',
}

/** The message for `op` failing with `error`; `null` is a failure that never reached the typed path. */
export function sessionOpFailureKey(op: SessionOp, error: ViewerError | null): MessageKey {
  if (error?.kind === 'timedOut') return 'viewer.sessionOp.slow'
  if (op === 'reload' && error?.kind === 'notFound') return 'viewer.reloadToast.gone'
  return genericKey[op]
}

/**
 * Logs `op` failing at the level its kind earns (`logLevelFor`): a slow mount or a deleted file
 * is warn, since an error log auto-sends a report; a dead session or an untyped failure is ours.
 */
export function logSessionOpFailure(log: Logger, op: SessionOp, error: ViewerError | null): void {
  const detail = { op, error: error ? JSON.stringify(error) : 'untyped' }
  if (error && logLevelFor(error) === 'warn') {
    log.warn('Viewer {op} failed: {error}', detail)
  } else {
    log.error('Viewer {op} failed: {error}', detail)
  }
}
