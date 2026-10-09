/**
 * The copy/move dialog's remembered "files already exist" choice.
 *
 * The dialog opens its policy radios on `fileOperations.defaultConflictPolicy`
 * and writes a person's confirmed pick back, so a Total Commander habit ("always
 * overwrite older") sticks. Three rules keep that from costing anyone files:
 *
 * - A REMEMBERED policy only goes out while its radios are on screen. Before the
 *   conflict check answers, when it couldn't run, or when nothing clashes, the
 *   person never saw it, so the confirm asks per file (`stop`) instead.
 * - Only a person's pick is written. A policy an MCP caller named (`autoConfirm`,
 *   `dialog confirm`) is the agent's, not the person's habit.
 * - A remembered policy that overwrites wears a note until the person picks.
 *
 * Why each rule: `DETAILS.md` § "The remembered conflict policy".
 */

import { getSetting, setSetting, type DefaultConflictPolicy } from '$lib/settings'
import type { ConflictResolution, TransferOperationType } from '$lib/file-explorer/types'

export const REMEMBERED_CONFLICT_POLICY_SETTING = 'fileOperations.defaultConflictPolicy'

/**
 * Where the dialog's current policy came from: the saved setting, the person's
 * own pick in the radios, or a caller that named one (MCP).
 */
export type ConflictPolicySource = 'remembered' | 'picked' | 'explicit'

const REMEMBERABLE: readonly DefaultConflictPolicy[] = [
  'stop',
  'skip',
  'overwrite',
  'overwrite_smaller',
  'overwrite_older',
] satisfies readonly ConflictResolution[]

function isRememberable(value: unknown): value is DefaultConflictPolicy {
  return REMEMBERABLE.some((policy) => policy === value)
}

/** The saved policy, or `stop` (ask for each) when the store holds anything else. */
export function readRememberedConflictPolicy(): DefaultConflictPolicy {
  const stored: unknown = getSetting(REMEMBERED_CONFLICT_POLICY_SETTING)
  return isRememberable(stored) ? stored : 'stop'
}

/** What a confirm sends: a remembered policy the person can't see falls back to asking. */
export function dispatchedConflictPolicy(state: {
  policy: ConflictResolution
  source: ConflictPolicySource
  choiceVisible: boolean
}): ConflictResolution {
  return state.source === 'remembered' && !state.choiceVisible ? 'stop' : state.policy
}

/**
 * Saves a person's confirmed pick as the new default. Writes nothing for a
 * remembered or MCP-named policy, for compress (it has no conflict choice), or
 * when the pick already matches, so sparse persistence never pins the default.
 * A managed-policy lock refuses the write inside `setSetting`.
 */
export function rememberConflictPolicy(choice: {
  policy: ConflictResolution
  source: ConflictPolicySource
  operationType: TransferOperationType
}): void {
  if (choice.source !== 'picked' || choice.operationType === 'compress') return
  if (!isRememberable(choice.policy) || choice.policy === readRememberedConflictPolicy()) return
  setSetting(REMEMBERED_CONFLICT_POLICY_SETTING, choice.policy)
}

/** Which files a remembered overwriting policy replaces, the note's `reach`. */
export type OverwriteNoteReach = 'all' | 'smaller' | 'older'

/** The note's reach while a remembered policy overwrites, `null` once there's nothing to flag. */
export function overwriteNoteReach(state: {
  policy: ConflictResolution
  source: ConflictPolicySource
}): OverwriteNoteReach | null {
  if (state.source !== 'remembered') return null
  switch (state.policy) {
    case 'overwrite':
      return 'all'
    case 'overwrite_smaller':
      return 'smaller'
    case 'overwrite_older':
      return 'older'
    default:
      return null
  }
}
