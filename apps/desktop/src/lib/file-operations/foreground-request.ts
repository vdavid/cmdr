/**
 * Turning "show operation X" into something the main window's progress dialog
 * can mount.
 *
 * The queue row sends an `operationId` and nothing else, because the registry
 * snapshot is the single source of truth about an operation and both windows
 * already receive it. This resolves that id against the MAIN window's own copy
 * of the snapshot, which is what makes the request survive the trip: whatever
 * the queue window was showing, the dialog opens on what this window knows.
 *
 * A miss is ordinary, not defensive: an operation that ended between the click
 * and the delivery has left the snapshot, and its row has left the queue window
 * at the same moment. There is nothing to show and nothing to say.
 */

import type { AdoptedOperationData } from '$lib/file-explorer/pane/dialog-props'
import type { OperationSnapshot } from '$lib/tauri-commands'
import type { TransferOperationType } from '$lib/file-explorer/types'
import type { OperationRow } from './queue/operations-store.svelte'

/**
 * Which operations the progress dialog can show, by wire type. The `null` arms
 * are the instant metadata ops: they emit no `write-progress` at all, so the
 * dialog would be a title over an empty frame, and they're over before anyone
 * could press anything. A `Record` rather than a set, so a new wire type is a
 * compile error here instead of a silent omission.
 */
const SHOWABLE_TYPES: Record<OperationSnapshot['operationType'], TransferOperationType | null> = {
  copy: 'copy',
  move: 'move',
  delete: 'delete',
  trash: 'trash',
  archive_edit: 'archive_edit',
  rename: null,
  create_folder: null,
  create_file: null,
}

/** The dialog chrome for `operationId`, or `null` when this window's snapshot
 *  has no such operation or it's one the dialog can't show. Pure: the caller
 *  owns the listening and the window. */
export function adoptedOperationFor(rows: OperationRow[], operationId: string): AdoptedOperationData | null {
  const row = rows.find((candidate) => candidate.snapshot.operationId === operationId)
  if (!row) return null
  const { snapshot } = row
  const operationType = SHOWABLE_TYPES[snapshot.operationType]
  if (operationType === null) return null
  return {
    operationId: snapshot.operationId,
    operationType,
    sourcePath: snapshot.source,
    destinationPath: snapshot.destination,
    reverses: snapshot.reverses,
  }
}
