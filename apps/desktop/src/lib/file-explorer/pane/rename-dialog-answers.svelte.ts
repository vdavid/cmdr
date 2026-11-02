/**
 * The inline rename editor's two questions: the extension change and the name conflict.
 *
 * This owns what each dialog shows and what its answers do. The flow (`rename-flow.svelte.ts`)
 * decides when to ask, and the session it owns reaches here through narrow getters and callbacks:
 * an answer that saves lands through the flow's own `land`, so it's judged against the session
 * like any other save.
 */

import { refreshListing, moveToTrash, type RenameValidityResult } from '#lib/tauri-commands/index.js'
import { asMutationError } from '#lib/file-operations/mutation-error.js'
import { renderMutationError } from '#lib/file-operations/mutation-error-messages.js'
import { fileExtension, kindOfRenamedName, withExtension } from '#lib/utils/file-extension.js'
import { addToastForPane, type ToastOriginPane } from '#lib/ui/toast/index.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { getVolumes } from '#lib/stores/volume-store.svelte.js'
import { performRename, type RenameConflictResolution, type RenameResult } from '../rename/rename-operations'
import type { RenameSessionId, RenameTarget, createRenameState } from '../rename/rename-state.svelte'
import { paneOffersTrash } from './trash-availability'

export interface ExtensionDialogState {
  oldExtension: string
  newExtension: string
  isDirectory: boolean
}

export interface ConflictDialogState {
  validity: RenameValidityResult
  trimmedName: string
  /** Whether the file being clobbered has a Trash to go to, read when the dialog opened. */
  supportsTrash: boolean
}

export interface RenameDialogAnswersDeps {
  rename: ReturnType<typeof createRenameState>
  /** Owning pane, so the answers' toasts stay pane-scoped. */
  paneId: ToastOriginPane
  getListingId: () => string
  getCurrentPath: () => string
  getVolumeId: () => string
  /** Saves the editor's name again, past the extension question the person just answered. */
  saveSkippingExtensionCheck: () => void
  /** The flow's own landing for a save, judged against the session that sent it. */
  land: (result: RenameResult, target: RenameTarget, trimmedName: string, sessionId: RenameSessionId) => void
  /** Ends the session with nothing (more) saved, handing focus back. */
  closeAndRefocus: () => void
}

export function createRenameDialogAnswers(deps: RenameDialogAnswersDeps) {
  const { rename } = deps

  let extensionDialogState = $state<ExtensionDialogState | null>(null)
  let conflictDialogState = $state<ConflictDialogState | null>(null)

  /** The overwrite-to-Trash answer: the clobbered file goes to the Trash, then the rename runs. */
  function trashThenRename(target: RenameTarget, trimmedName: string, sessionId: RenameSessionId, volumeId: string) {
    const conflictPath = target.parentPath + '/' + trimmedName
    void moveToTrash(conflictPath, target.initiator)
      .then(() => performRename(target, trimmedName, true, volumeId))
      .then((result) => {
        deps.land(result, target, trimmedName, sessionId)
      })
      .catch((e: unknown) => {
        const failure = asMutationError(e)
        if (failure?.type === 'timedOut') {
          addToastForPane(deps.paneId, tString('fileExplorer.pane.trashUnconfirmedToast'), {
            level: 'warn',
            dismissal: 'persistent',
          })
          // Unforced: a top-up after an unconfirmed rename.
          void refreshListing(deps.getListingId(), false)
        } else {
          // Worded in the user's language either way: a value that never
          // reached the typed path reads as the backend's own honest
          // fallback rather than as raw English.
          addToastForPane(
            deps.paneId,
            renderMutationError(failure ?? { type: 'unexpected', detail: '' }, target.isDirectory ? 'folder' : 'file'),
            { level: 'error' },
          )
        }
        if (rename.isSuperseded(sessionId)) return
        deps.closeAndRefocus()
      })
  }

  return {
    get extensionDialogState() {
      return extensionDialogState
    },
    get conflictDialogState() {
      return conflictDialogState
    },
    /** Either question is on screen, so the dialog, not a click on it, owns the decision. */
    get isAsking() {
      return extensionDialogState !== null || conflictDialogState !== null
    },

    askExtension(result: Extract<RenameResult, { type: 'extension-ask' }>, target: RenameTarget) {
      extensionDialogState = {
        oldExtension: result.oldExtension,
        newExtension: result.newExtension,
        isDirectory: target.isDirectory,
      }
    },

    askConflict(result: Extract<RenameResult, { type: 'conflict' }>, trimmedName: string) {
      // Snapshot at open: the dialog lives for a few seconds, and the answer
      // decides which buttons it draws, so re-deriving it mid-dialog could move
      // the primary action under the user's hand.
      conflictDialogState = {
        validity: result.validity,
        trimmedName,
        supportsTrash: paneOffersTrash(deps.getVolumeId(), deps.getCurrentPath(), getVolumes()),
      }
    },

    /** The session ended: neither question is still waiting for an answer. */
    dismiss() {
      extensionDialogState = null
      conflictDialogState = null
    },

    // The answers are arrows: the flow hands them to `FilePane` unbound.
    keepOldExtension: () => {
      extensionDialogState = null
      if (rename.target) {
        // Put the old extension back, also when the edit removed it or added one
        // where there was none, so "Keep" always means the original extension.
        const { originalName, nameKind } = rename.target
        const oldExt = fileExtension(originalName, nameKind)
        const typed = rename.getTrimmedName()
        rename.setCurrentName(withExtension(typed, oldExt, kindOfRenamedName(nameKind, typed)))
      }
      rename.requestRefocus()
    },

    useNewExtension: () => {
      extensionDialogState = null
      deps.saveSkippingExtensionCheck()
    },

    resolveConflict: (resolution: RenameConflictResolution) => {
      const target = rename.target
      const sessionId = rename.sessionId
      const trimmedName = conflictDialogState?.trimmedName
      conflictDialogState = null

      if (!target || !trimmedName) {
        deps.closeAndRefocus()
        return
      }

      // Read as the person answers: the pane may move on while the Trash works.
      const volumeId = deps.getVolumeId()
      switch (resolution) {
        case 'overwrite-trash':
          trashThenRename(target, trimmedName, sessionId, volumeId)
          break
        case 'overwrite-delete':
          void performRename(target, trimmedName, true, volumeId).then((result) => {
            deps.land(result, target, trimmedName, sessionId)
          })
          break
        case 'cancel':
          deps.closeAndRefocus()
          break
        case 'continue':
          rename.requestRefocus()
          break
      }
    },
  }
}
