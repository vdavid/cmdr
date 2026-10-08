/**
 * The words under a suggestion whose approval refused to start.
 *
 * A refusal the write engine gave (a phone nobody connected, a drive that left, no space) is
 * the same `WriteOperationError` a clicked operation would have hit, so it reads through the
 * same `errors.write.*` pipeline (`../file-operations/transfer/transfer-error-messages.ts`):
 * the suggestion says exactly what the transfer dialog would. Only the two refusals that exist
 * for suggestions alone carry their own copy.
 */

import { tString } from '$lib/intl/messages.svelte'
import type { TransferOperationType } from '$lib/file-explorer/types'
import type { SuggestedGroupView } from '$lib/tauri-commands'
import { getUserFriendlyMessage } from '$lib/file-operations/transfer/transfer-error-messages'
import type { GroupRefusal } from './suggested-ops-trigger.svelte'

/** What sits under the group. `message` is HTML (the pipeline escapes every name and path), so
 *  it renders through `{@html}`, the boundary the queue's failed row uses too. */
export interface RefusalReason {
  message: string
  suggestion: string
}

/**
 * Which `errors.write.<field>.<op>` arm a group's verb reads from. A `Record` over every verb
 * makes a new verb a compile error here rather than a missing key at runtime. A rename runs as
 * a move where it can't rename in place, and extract is a copy out of an archive.
 */
const OPERATION_TYPE: Record<SuggestedGroupView['verb'], TransferOperationType> = {
  move: 'move',
  copy: 'copy',
  extract: 'copy',
  compress: 'compress',
  trash: 'trash',
  delete: 'delete',
  rename: 'move',
}

export function refusalReason(refusal: GroupRefusal, verb: SuggestedGroupView['verb']): RefusalReason {
  switch (refusal.kind) {
    case 'refused': {
      const friendly = getUserFriendlyMessage(refusal.error, OPERATION_TYPE[verb])
      return { message: friendly.message, suggestion: friendly.suggestion }
    }
    case 'nothingToRun':
      return {
        message: tString('suggestedOps.refusal.nothingToRun.message'),
        suggestion: tString('suggestedOps.refusal.nothingToRun.suggestion'),
      }
    case 'couldNotStart':
      return {
        message: tString('suggestedOps.refusal.couldNotStart.message'),
        suggestion: tString('suggestedOps.refusal.couldNotStart.suggestion'),
      }
  }
}
