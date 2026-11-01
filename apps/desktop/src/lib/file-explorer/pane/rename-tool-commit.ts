/**
 * The MCP `dialog_confirm` on the inline rename editor: Enter with nobody there to ask.
 *
 * So it differs from Enter in what it can't put to a person: the extension question is skipped, and
 * a taken name refuses instead of raising the conflict dialog. It answers with how the save ended,
 * and a refused name leaves the editor open, as a person's would.
 */

import { tString } from '#lib/intl/messages.svelte.js'
import type { ExtensionChangePolicy } from '#lib/settings/index.js'
import { executeRenameSave, type RenameResult } from '../rename/rename-operations'
import type { RenameSessionId, RenameTarget, createRenameState } from '../rename/rename-state.svelte'
import type { CreatePress } from './dialog-props'

export interface RenameToolCommitDeps {
  rename: ReturnType<typeof createRenameState>
  /** What typing `name` into the editor does: the field, and its validation. */
  applyName: (name: string) => void
  /** Ends the session with nothing saved, handing focus back. */
  closeAndRefocus: () => void
  extensionPolicy: () => ExtensionChangePolicy
  getVolumeId: () => string
  /** The save owns the session while it runs, as it does for a click-away. */
  setPendingCommit: (pending: Promise<void> | null) => void
  /** The flow's own landing, which ends the session and moves the cursor. */
  land: (result: RenameResult, target: RenameTarget, trimmedName: string, sessionId: RenameSessionId) => void
}

export function createRenameToolCommit(deps: RenameToolCommitDeps) {
  const { rename } = deps

  function endOf(
    result: RenameResult,
    target: RenameTarget,
    trimmedName: string,
    sessionId: RenameSessionId,
  ): CreatePress {
    switch (result.type) {
      case 'error':
        return { kind: 'refused', reason: result.message }
      case 'conflict':
        return { kind: 'refused', reason: tString('fileOperations.validation.conflict', { name: trimmedName }) }
      case 'extension-ask':
        // The save skips that question, so this can't come back.
        return { kind: 'refused', reason: 'The extension change needs a person.' }
      case 'still-renaming':
        deps.land(result, target, trimmedName, sessionId)
        return { kind: 'stillRunning' }
      case 'confirm-move':
        deps.land(result, target, trimmedName, sessionId)
        return { kind: 'needsMoveDialog' }
      case 'noop':
      case 'success':
        deps.land(result, target, trimmedName, sessionId)
        return { kind: 'done' }
    }
  }

  return async function commitFromTool(name?: string): Promise<CreatePress> {
    const target = rename.target
    if (!target) return { kind: 'refused', reason: 'The rename editor closed.' }
    if (name !== undefined) deps.applyName(name)
    if (rename.severity === 'error') return { kind: 'refused', reason: rename.validation.message }
    if (!rename.hasChanged()) {
      deps.closeAndRefocus()
      return { kind: 'done' }
    }
    const sessionId = rename.sessionId
    const trimmedName = rename.getTrimmedName()
    const saving = executeRenameSave(target, trimmedName, deps.extensionPolicy(), true, deps.getVolumeId()).finally(
      () => {
        deps.setPendingCommit(null)
      },
    )
    deps.setPendingCommit(saving.then(() => {}))
    return endOf(await saving, target, trimmedName, sessionId)
  }
}
