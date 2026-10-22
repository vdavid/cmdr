/**
 * The words for a conflict policy, shared by the two dialogs that name one: the
 * setup dialog's radios (`TransferConflictsSection.svelte`) and the progress
 * dialog's caption under its from → to line. One list, so the policy a person
 * picked reads the same once the transfer runs.
 */

import type { ConflictResolution } from '#lib/file-explorer/types.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import { tString } from '#lib/intl/messages.svelte.js'

/** The setup dialog's policy radios, in their order. Each label is a count
 *  plural: "Skip" for one clash, "Skip all" for several. */
export const CONFLICT_POLICY_OPTIONS = [
  { value: 'skip', labelKey: 'fileOperations.transferDialog.policySkip' },
  { value: 'overwrite', labelKey: 'fileOperations.transferDialog.policyOverwrite' },
  { value: 'overwrite_smaller', labelKey: 'fileOperations.transferDialog.policyOverwriteSmaller' },
  { value: 'overwrite_older', labelKey: 'fileOperations.transferDialog.policyOverwriteOlder' },
  { value: 'stop', labelKey: 'fileOperations.transferDialog.policyStop' },
] as const satisfies readonly { value: ConflictResolution; labelKey: MessageKey }[]

/**
 * A policy in the radios' count-free wording, the one they show when nobody
 * knows how many files clash: "Overwrite if older", "Ask for each". Rename has
 * no radio (only a clash's "Rename all" reaches it), so it takes that button's
 * word.
 */
function conflictPolicyLabel(policy: ConflictResolution): string {
  if (policy === 'stop') return tString('fileOperations.transferDialog.policyAskEach')
  if (policy === 'rename') return tString('fileOperations.transferProgress.conflictRename')
  const option = CONFLICT_POLICY_OPTIONS.find((candidate) => candidate.value === policy)
  return option === undefined ? '' : tString(option.labelKey, { count: 1 })
}

/** The progress dialog's one-line caption: "If a file already exists: Skip". */
export function conflictPolicyCaption(policy: ConflictResolution): string {
  return tString('fileOperations.transferProgress.conflictPolicyCaption', { policy: conflictPolicyLabel(policy) })
}
