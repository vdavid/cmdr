/** Which log level a failed write operation's typed error gets. */
import type { WriteOperationError } from '#lib/ipc/bindings.js'
import { getErrorDisplayMeta } from './transfer-error-messages'

/**
 * Whether a failure is the world's doing, which the error dialog explains with a way
 * forward (`warn`), or a `serious` kind (a read, write, or I/O error, a symlink loop)
 * that may be our own defect (`error`). An error log counts toward an auto-sent error
 * report, so a permission refusal or a full disk never files one (ERR-4K2HT,
 * 2026-11-04: a refused delete in a system folder auto-sent a report).
 *
 * Rides the dialog's own category table (`getErrorDisplayMeta`), so a new variant is
 * classified once. A wrapper that carries the reason it stopped (`originals_kept_aside`,
 * `source_not_removed`, `new_files_kept`) logs at its cause's level: the kept files
 * are a fine outcome, the cause is what may be a defect. The viewer's twin is
 * `routes/viewer/viewer-open-failure.ts`.
 */
export function logLevelFor(error: WriteOperationError): 'warn' | 'error' {
  if (error.type === 'originals_kept_aside' || error.type === 'source_not_removed' || error.type === 'new_files_kept') {
    return logLevelFor(error.cause)
  }
  return getErrorDisplayMeta(error).category === 'serious' ? 'error' : 'warn'
}
