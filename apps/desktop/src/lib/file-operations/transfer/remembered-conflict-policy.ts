/**
 * The copy/move dialog's remembered "files already exist" choice.
 *
 * The dialog opens its policy radios on `fileOperations.defaultConflictPolicy`,
 * and a "Remember this choice" box under them saves the confirmed choice there,
 * so a Total Commander habit ("always overwrite older") sticks once a person asks
 * for it. One setting holds it all: the box starts ticked exactly when a choice
 * other than `stop` (ask later) is saved, and an unticked confirm saves `stop`.
 * Four rules keep that from costing anyone files:
 *
 * - Nothing becomes a default without the tick: a plain pick only applies to
 *   this transfer.
 * - A REMEMBERED policy only goes out while its radios are on screen. Before the
 *   conflict check answers, when it couldn't run, or when nothing clashes, the
 *   person never saw it, so the confirm asks per file (`stop`) instead, and the
 *   box, off screen too, saves nothing.
 * - Only a person's pick writes. A policy an MCP caller named (`onConflict` on
 *   `copy` / `move`, or on `dialog_confirm`) is the agent's, not the person's habit.
 * - A remembered or preset policy that overwrites wears a note until the person picks.
 *
 * A preset is the policy an MCP caller's `copy` / `move` opens the dialog on. It
 * takes the remembered policy's place for that one dialog, under the same rule: it
 * only goes out while its radios are on screen, so an Enter while the check is
 * pending can't send a model-chosen overwrite with nothing showing.
 *
 * Why each rule: `DETAILS.md` § "The remembered conflict policy".
 */

import { getSetting, setSetting, type DefaultConflictPolicy } from '#lib/settings/index.js'
import type { ConflictResolution, TransferOperationType } from '#lib/file-explorer/types.js'

export const REMEMBERED_CONFLICT_POLICY_SETTING = 'fileOperations.defaultConflictPolicy'

/**
 * Where the dialog's current policy came from: the saved setting, the person's
 * own pick in the radios, what an MCP `copy` / `move` opened it on, or a
 * `dialog_confirm` that named one.
 */
export type ConflictPolicySource = 'remembered' | 'picked' | 'preset' | 'explicit'

/** Nobody chose these on this dialog, so they wait for their radios and wear the note. */
function isUnpicked(source: ConflictPolicySource): boolean {
  return source === 'remembered' || source === 'preset'
}

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

/** What a confirm sends: a remembered or preset policy the person can't see falls back to asking. */
export function dispatchedConflictPolicy(state: {
  policy: ConflictResolution
  source: ConflictPolicySource
  choiceVisible: boolean
}): ConflictResolution {
  return isUnpicked(state.source) && !state.choiceVisible ? 'stop' : state.policy
}

/** Whether "Remember this choice" starts ticked: a saved choice other than asking later. */
export function rememberStartsTicked(saved: DefaultConflictPolicy): boolean {
  return saved !== 'stop'
}

/**
 * Saves what a person's confirm leaves as the default: the selected policy under
 * a ticked "Remember this choice", else `stop`. Writes nothing for an MCP-named
 * policy (preset or explicit), for compress (it has no conflict choice), while the radios and the box
 * are off screen, or when the outcome already matches, so sparse persistence
 * never pins the default. A managed-policy lock refuses the write inside
 * `setSetting`.
 */
export function rememberConflictPolicy(choice: {
  policy: ConflictResolution
  source: ConflictPolicySource
  /** The "Remember this choice" box. Hidden on `stop`, where either answer saves `stop`. */
  remember: boolean
  choiceVisible: boolean
  operationType: TransferOperationType
}): void {
  if (choice.source === 'explicit' || choice.source === 'preset') return
  if (choice.operationType === 'compress' || !choice.choiceVisible) return
  const saved = choice.remember ? choice.policy : 'stop'
  if (!isRememberable(saved) || saved === readRememberedConflictPolicy()) return
  setSetting(REMEMBERED_CONFLICT_POLICY_SETTING, saved)
}

/** Which files a remembered or preset overwriting policy replaces, the note's `reach`. */
export type OverwriteNoteReach = 'all' | 'smaller' | 'older'

/** The note's reach while a remembered or preset policy overwrites, `null` once there's nothing to flag. */
export function overwriteNoteReach(state: {
  policy: ConflictResolution
  source: ConflictPolicySource
}): OverwriteNoteReach | null {
  if (!isUnpicked(state.source)) return null
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
