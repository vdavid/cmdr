/**
 * The archive-password prompt, and the two flows that can raise it.
 *
 * - `transfer`: a copy/move whose source is inside an encrypted archive fails
 *   with `archive_needs_password`. The progress dialog unmounts but the birth
 *   slot stays alive, so a successful unlock re-dispatches the SAME operation
 *   and a cancel settles it through the normal refresh/selection paths.
 * - `browse`: listing a header-encrypted archive needs the password even to
 *   read its metadata. No operation is involved; an unlock re-lists via `retry`.
 *
 * The flow never sees birth context. It asks whether one exists and tells the
 * owner to re-dispatch or settle it, but it holds no reference to the props and
 * can supply none, so the re-dispatch cannot be aimed at anything but the
 * operation the user unlocked. `DETAILS.md` § "Birth context".
 */

import { setArchivePassword, clearArchivePassword } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'
import { archiveNameFromPath } from './volume-capabilities'
import { transferOpLabel } from './transfer-op-label'
import type { TransferOperationType } from '../types'
import type { ArchivePasswordPropsData } from './dialog-props'

const log = getAppLogger('fileExplorer')

export interface ArchivePasswordFlowDeps {
  /** Whether an operation this window started is waiting on the unlock. */
  hasBirthContext: () => boolean
  /** Re-runs the operation the birth slot holds, with a fresh scan. Takes no
   *  argument on purpose: the flow cannot aim it. */
  redispatchBirthOperation: () => void
  /** Settles the birth operation exactly as a dismissed transfer error would
   *  (refresh panes, drop the source-pane snapshot and selection). */
  settleBirthOperation: () => void
  /** Mounts or unmounts the shared progress dialog. */
  setProgressDialogShown: (shown: boolean) => void
  onRefocus: () => void
}

export function createArchivePasswordFlow(deps: ArchivePasswordFlowDeps) {
  let showDialog = $state(false)
  let promptProps = $state<ArchivePasswordPropsData | null>(null)

  /** Stores the password, logging (but not surfacing) a store failure: whatever
   *  retries next will simply re-prompt, since the password never landed. */
  async function storePassword(parentVolumeId: string, archivePath: string, password: string): Promise<void> {
    try {
      await setArchivePassword(parentVolumeId, archivePath, password)
    } catch (err) {
      log.warn('Failed to store archive password: {error}', { error: err })
    }
  }

  return {
    get showDialog(): boolean {
      return showDialog
    },
    get props(): ArchivePasswordPropsData | null {
      return promptProps
    },

    /** Raises the transfer-time prompt: an operation the birth slot holds needs
     *  a password to read its source. The parent volume the archive lives on is
     *  the source pane's volume id (an archive pane keeps its parent drive's
     *  id); the archive path is the errored source path. */
    promptForTransfer(info: {
      operationType: TransferOperationType
      parentVolumeId: string
      archivePath: string
      wrongAttempt: boolean
    }): void {
      log.info('{op} operation needs an archive password ({state}): {path}', {
        op: transferOpLabel(info.operationType),
        state: info.wrongAttempt ? 'rejected' : 'first prompt',
        path: info.archivePath,
      })
      promptProps = {
        archiveName: archiveNameFromPath(info.archivePath),
        wrongAttempt: info.wrongAttempt,
        parentVolumeId: info.parentVolumeId,
        archivePath: info.archivePath,
        mode: 'transfer',
      }
      deps.setProgressDialogShown(false)
      showDialog = true
    },

    /** Raises the browse-time prompt: a directory listing of a header-encrypted
     *  archive failed because its metadata is encrypted, so even listing needs
     *  the password. No operation is involved; on unlock `retry` re-lists the
     *  same directory. */
    promptForBrowse(info: { volumeId: string; archivePath: string; wrongAttempt: boolean; retry: () => void }): void {
      log.info('Directory listing needs an archive password ({state}): {path}', {
        state: info.wrongAttempt ? 'rejected' : 'first prompt',
        path: info.archivePath,
      })
      promptProps = {
        archiveName: archiveNameFromPath(info.archivePath),
        wrongAttempt: info.wrongAttempt,
        parentVolumeId: info.volumeId,
        archivePath: info.archivePath,
        mode: 'browse',
        retry: info.retry,
      }
      showDialog = true
    },

    /** Stores the entered password on the backend, then retries whatever raised
     *  the prompt: browse mode re-lists the directory, transfer mode re-runs the
     *  copy/move. Either way a wrong password re-raises the prompt (with
     *  `wrongAttempt: true`). */
    handleSubmit(password: string): void {
      const pw = promptProps
      if (!pw) return

      if (pw.mode === 'browse') {
        const retry = pw.retry
        showDialog = false
        promptProps = null
        void (async () => {
          await storePassword(pw.parentVolumeId, pw.archivePath, password)
          retry?.()
        })()
        return
      }

      // Transfer path: the re-dispatch is a NEW operation, so it re-scans (the
      // previous preview was consumed and the backend refuses a second claim on
      // one preview). Nothing to retry if the operation has gone away.
      if (!deps.hasBirthContext()) return

      showDialog = false
      promptProps = null

      void (async () => {
        await storePassword(pw.parentVolumeId, pw.archivePath, password)
        deps.redispatchBirthOperation()
      })()
    },

    /** The user dismissed the prompt: forget any stored password. Browse mode
     *  just closes it, leaving the "This archive needs a password" fallback pane
     *  in place (the loader already settled it), so the user simply doesn't get
     *  in. Transfer mode settles the operation exactly as a dismissed transfer
     *  error would, so nothing looks stuck. */
    handleCancel(): void {
      const pw = promptProps
      if (pw) {
        void clearArchivePassword(pw.parentVolumeId, pw.archivePath)
      }

      if (pw?.mode === 'browse') {
        log.info('Browse archive-password prompt cancelled')
        showDialog = false
        promptProps = null
        deps.onRefocus()
        return
      }

      showDialog = false
      promptProps = null
      deps.settleBirthOperation()
      deps.onRefocus()
    },

    /** Clears the prompt with no backend call, for the render-failure sweep. */
    forget(): void {
      showDialog = false
      promptProps = null
    },
  }
}

export type ArchivePasswordFlow = ReturnType<typeof createArchivePasswordFlow>
