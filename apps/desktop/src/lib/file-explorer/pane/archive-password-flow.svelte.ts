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
 *
 * Two entry points answer the prompt, and only one of them re-dispatches:
 * `handleSubmit` is a person typing, `supplyStoredPassword` is the MCP
 * `unlock_archive` tool. See that method for why an agent must not start the
 * write. Whatever raises or clears the prompt also mirrors it to the backend
 * (`showPrompt` / `hidePrompt`), which is what lets `cmdr://state` name the
 * archive at all.
 */

import {
  setArchivePassword,
  clearArchivePassword,
  notifyArchivePasswordPrompt,
  notifyArchivePasswordDismissed,
} from '$lib/tauri-commands'
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

  /** Raises the prompt and mirrors WHAT IT ASKS to the backend, so `cmdr://state`
   *  names the archive and `unlock_archive` has something to answer. The two go
   *  together: a prompt on screen that nothing outside can see is exactly the
   *  blind spot this flow used to be. ❌ The password is not part of the mirror. */
  function showPrompt(props: ArchivePasswordPropsData, operationId: string | null): void {
    promptProps = props
    showDialog = true
    void notifyArchivePasswordPrompt({
      archiveName: props.archiveName,
      archivePath: props.archivePath,
      parentVolumeId: props.parentVolumeId,
      mode: props.mode,
      wrongAttempt: props.wrongAttempt,
      operationId,
    }).catch((err: unknown) => {
      // Only MCP loses out, and only until the next prompt. Never worth
      // surfacing to someone who is looking at the dialog.
      log.warn('Failed to mirror the archive-password prompt: {error}', { error: err })
    })
  }

  /** Takes the prompt down and clears the mirror with it, so nothing outside is
   *  told a question is still open. */
  function hidePrompt(): void {
    showDialog = false
    promptProps = null
    void notifyArchivePasswordDismissed().catch((err: unknown) => {
      log.warn('Failed to clear the archive-password mirror: {error}', { error: err })
    })
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
      /** The operation that hit the prompt. Already settled by the backend (a
       *  password failure settles it), so it's a correlation handle for whoever
       *  started the copy, never something to resume. */
      operationId: string | null
    }): void {
      log.info('{op} operation needs an archive password ({state}): {path}', {
        op: transferOpLabel(info.operationType),
        state: info.wrongAttempt ? 'rejected' : 'first prompt',
        path: info.archivePath,
      })
      deps.setProgressDialogShown(false)
      showPrompt(
        {
          archiveName: archiveNameFromPath(info.archivePath),
          wrongAttempt: info.wrongAttempt,
          parentVolumeId: info.parentVolumeId,
          archivePath: info.archivePath,
          mode: 'transfer',
        },
        info.operationId,
      )
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
      showPrompt(
        {
          archiveName: archiveNameFromPath(info.archivePath),
          wrongAttempt: info.wrongAttempt,
          parentVolumeId: info.volumeId,
          archivePath: info.archivePath,
          mode: 'browse',
          retry: info.retry,
        },
        null,
      )
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
        hidePrompt()
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

      hidePrompt()

      void (async () => {
        await storePassword(pw.parentVolumeId, pw.archivePath, password)
        deps.redispatchBirthOperation()
      })()
    },

    /**
     * The MCP `unlock_archive` tool supplied the password. The backend already
     * stored it, so all that is left is the mode's follow-up.
     *
     * **The one difference from `handleSubmit`, and the reason this is a separate
     * entry point: transfer mode does NOT re-dispatch.** A person typing the
     * password is standing in front of the operation they started; an agent
     * supplying one would otherwise be starting a brand-new write with no
     * confirmation dialog and no token in front of it, which is the one thing
     * extraction must not get that a copy doesn't. ❌ Never call
     * `redispatchBirthOperation` from here. The agent runs `copy` / `move`
     * again, through the same gate as any other write, and the stored password
     * makes that one succeed.
     *
     * Browse mode has no such boundary: re-listing is a read, so it retries
     * exactly as a person's submit does.
     */
    supplyStoredPassword(): void {
      const pw = promptProps
      if (!pw) return

      if (pw.mode === 'browse') {
        const retry = pw.retry
        hidePrompt()
        retry?.()
        return
      }

      log.info('Archive password supplied over MCP: stored, and the transfer is settled rather than re-dispatched')
      hidePrompt()
      if (deps.hasBirthContext()) deps.settleBirthOperation()
      deps.onRefocus()
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
        hidePrompt()
        deps.onRefocus()
        return
      }

      hidePrompt()
      deps.settleBirthOperation()
      deps.onRefocus()
    },

    /** Clears the prompt with no password call, for the render-failure sweep.
     *  The mirror still goes: a prompt nothing will ever answer must not be left
     *  advertised in `cmdr://state`. */
    forget(): void {
      hidePrompt()
    },
  }
}
