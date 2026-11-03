/**
 * The reason a retained failure shows, in the words the error dialog already
 * uses.
 *
 * A failed row and the main window's failure toast both render this, so the two
 * surfaces can't drift into describing the same failure differently. All the
 * wording comes from the existing `errors.write.*` pipeline
 * (`../transfer/transfer-error-messages.ts`): no prose crosses IPC, and no new
 * error copy is invented here.
 *
 * The one thing this module owns is the operation-type mapping. A snapshot
 * carries the WIRE type (`archive_edit`, `create_folder`, …) while the error
 * catalog only phrases the four transfer verbs, so the mapping has to be
 * explicit — a cast would resolve a missing catalog key at runtime.
 */

import type { OperationSnapshot } from '#lib/tauri-commands/index.js'
import { operationFailureKinds, type OperationFailedIncident } from '#lib/error-reporter/report-incident.js'
import type { TransferOperationType } from '#lib/file-explorer/types.js'
import { getUserFriendlyMessage, type FriendlyErrorMessage } from '../transfer/transfer-error-messages'

/**
 * Which set of `errors.write.<field>.<op>` arms an operation's reason reads
 * from. A `Record` keyed by every wire type makes a new operation type a
 * compile error here rather than a missing key at runtime.
 *
 * The four transfer verbs map to themselves. Everything else borrows the copy
 * wording: it's the pipeline's own default, and the copy arms read naturally
 * for "couldn't put the file where it was going", which is what an archive edit
 * or a folder create fails at too.
 */
const REASON_OPERATION_TYPE: Record<OperationSnapshot['operationType'], TransferOperationType> = {
  copy: 'copy',
  move: 'move',
  delete: 'delete',
  trash: 'trash',
  archive_edit: 'copy',
  compress: 'compress',
  rename: 'copy',
  create_folder: 'copy',
  create_file: 'copy',
  tag: 'copy',
}

/**
 * The title, explanation, and suggestion for a retained failure, or `null` for
 * a row that carries no error (every live row).
 *
 * The explanation is HTML: the pipeline escapes interpolated names and paths
 * and wraps sizes in tier classes, so a caller renders it with `{@html}` (the
 * same boundary `FallbackErrorContent` uses), never as raw text.
 */
export function failureReasonFor(snapshot: OperationSnapshot): FriendlyErrorMessage | null {
  if (snapshot.error === null) return null
  return getUserFriendlyMessage(snapshot.error, REASON_OPERATION_TYPE[snapshot.operationType])
}

/**
 * What "Send error report…" on a retained failure reports: the operation, the typed
 * error kind, and the pipeline's title for it. `null` for a row that carries no error.
 */
export function failureIncidentFor(
  snapshot: OperationSnapshot,
  from: OperationFailedIncident['from'],
): OperationFailedIncident | null {
  const reason = failureReasonFor(snapshot)
  if (snapshot.error === null || reason === null) return null
  return {
    kind: 'operationFailed',
    from,
    operation: snapshot.operationType,
    ...operationFailureKinds(snapshot.error),
    title: reason.title,
  }
}
