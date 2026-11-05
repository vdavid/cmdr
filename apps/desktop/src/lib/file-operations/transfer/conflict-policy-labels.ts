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
 * Each policy's NAME for the progress dialog's caption, a noun phrase or state
 * ("Overwrite if older"). ❗ Not the radios' labels: those are commands, which many
 * languages write as imperatives, and the caption then read as an order
 * ("…: Overwrite if older!"). A `Record`, so a new policy can't compile without one.
 */
const CONFLICT_POLICY_NAME_KEYS: Record<ConflictResolution, MessageKey> = {
  skip: 'fileOperations.transferProgress.policyName.skip',
  overwrite: 'fileOperations.transferProgress.policyName.overwrite',
  overwrite_smaller: 'fileOperations.transferProgress.policyName.overwriteSmaller',
  overwrite_older: 'fileOperations.transferProgress.policyName.overwriteOlder',
  stop: 'fileOperations.transferProgress.policyName.askEach',
  rename: 'fileOperations.transferProgress.policyName.rename',
}

/** The progress dialog's one-line caption: "If a file already exists: Skip". */
export function conflictPolicyCaption(policy: ConflictResolution): string {
  return tString('fileOperations.transferProgress.conflictPolicyCaption', {
    policy: tString(CONFLICT_POLICY_NAME_KEYS[policy]),
  })
}
