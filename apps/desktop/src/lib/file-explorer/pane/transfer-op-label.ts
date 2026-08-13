import type { TransferOperationType } from '../types'

/** Human-readable label for a transfer op, used in log lines. Log-only, so it
 *  deliberately skips the i18n catalog: logs are read by us, in English. */
export function transferOpLabel(op: TransferOperationType): string {
  return op === 'copy'
    ? 'Copy'
    : op === 'move'
      ? 'Move'
      : op === 'compress'
        ? 'Compress'
        : op === 'trash'
          ? 'Trash'
          : 'Delete'
}
