/**
 * The `OpKind` behind the two other names an operation travels under.
 *
 * `OpKind` is the operation log's vocabulary — the one every decision about what
 * an operation DID or what undoing it will do is taken in
 * (`reversal-wording.ts`). Two other spellings reach the UI: the registry
 * snapshot's `WriteOperationType` (snake_case, straight off the wire) and the
 * progress dialog's `TransferOperationType` (the dialog's own set, which splits
 * a compress out of the archive edits it runs as).
 *
 * Both maps are `Record`s over the full source type, so a new operation type is
 * a compile error here rather than a `default` arm quietly wording it as a copy.
 */

import type { OpKind, WriteOperationType } from '$lib/ipc/bindings'
import type { TransferOperationType } from '$lib/file-explorer/types'

/** Wire type → `OpKind`. A pure spelling change: the two enums have the same arms. */
const WIRE_TO_OP_KIND: Record<WriteOperationType, OpKind> = {
  copy: 'copy',
  move: 'move',
  delete: 'delete',
  trash: 'trash',
  rename: 'rename',
  create_folder: 'createFolder',
  create_file: 'createFile',
  archive_edit: 'archiveEdit',
}

/**
 * Dialog type → `OpKind`. `compress` folds into `archiveEdit`: a compress runs
 * as an archive edit and is journaled as one, and the split exists only so the
 * dialog can say "Compressing" instead of "Copying".
 */
const TRANSFER_TO_OP_KIND: Record<TransferOperationType, OpKind> = {
  copy: 'copy',
  move: 'move',
  delete: 'delete',
  trash: 'trash',
  archive_edit: 'archiveEdit',
  compress: 'archiveEdit',
}

/** The `OpKind` a registry snapshot's `operationType` names. */
export function opKindForWireType(type: WriteOperationType): OpKind {
  return WIRE_TO_OP_KIND[type]
}

/** The `OpKind` a progress dialog's `operationType` names. */
export function opKindForTransferType(type: TransferOperationType): OpKind {
  return TRANSFER_TO_OP_KIND[type]
}
