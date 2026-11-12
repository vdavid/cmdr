/**
 * The unlock prompt, and the two flows that can raise it.
 *
 * - `transfer`: a copy/move whose source is inside a locked container fails
 *   with `archive_needs_password`. The progress dialog unmounts but the birth
 *   slot stays alive, so a successful unlock re-dispatches the SAME operation
 *   and a cancel settles it through the normal refresh/selection paths.
 * - `browse`: a listing needs the password (a header-encrypted archive's
 *   metadata, a restic repo's snapshots). No operation is involved; an unlock
 *   re-lists via `retry`.
 *
 * Two kinds of container answer differently, and the backend's typed
 * `NeedsPassword { container }` says which one this is, ❌ never the path (a
 * header-encrypted 7z backed up in a restic snapshot is an archive):
 * - An archive STORES the password unchecked; the prompt closes at once and the
 *   retry proves it (a wrong one comes back as a fresh prompt).
 * - A restic repo CHECKS it, which takes a second or more (a key derivation and
 *   an open, maybe over a NAS). The prompt stays up with a spinner, a wrong
 *   password re-arms it in place, and Cancel mid-check closes at once and tells
 *   the backend to drop the check, so nothing is kept or saved to the Keychain.
 *
 * The flow never sees birth context. It asks whether one exists and tells the
 * owner to re-dispatch or settle it, but it holds no reference to the props and
 * can supply none, so the re-dispatch cannot be aimed at anything but the
 * operation the user unlocked. `DETAILS.md` § "Birth context".
 *
 * Two entry points answer the prompt, and only one of them re-dispatches:
 * `handleSubmit` is a person typing, `supplyStoredPassword` is the MCP
 * `unlock` tool. See that method for why an agent must not start the
 * write. Whatever raises or clears the prompt also mirrors it to the backend
 * (`showPrompt` / `hidePrompt`), which is what lets `cmdr://state` name the
 * container at all.
 */

import {
  setArchivePassword,
  unlockContainer,
  cancelContainerUnlock,
  forgetContainerPassword,
  UnlockFailure,
  notifyArchivePasswordPrompt,
  notifyArchivePasswordDismissed,
  type UnlockOutcome,
} from '#lib/tauri-commands/index.js'
import { failureOf } from '#lib/ipc/typed-failure.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { archiveNameFromPath } from './archive-paths'
import { resticRepoRoot } from '../restic/path-detection'
import { transferOpLabel } from './transfer-op-label'
import type { TransferOperationType } from '../types'
import type { BrowseUnlockRequest, UnlockContainer, UnlockPromptPropsData } from './dialog-props'

const log = getAppLogger('fileExplorer')

export interface UnlockFlowDeps {
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

/** The name a person knows the locked `container` at `path` by: the repo
 *  folder ("test-restic") or the archive file ("photos.zip"). Lexical, no I/O. */
export function containerName(container: UnlockContainer, path: string): string {
  const repoRoot = container === 'resticRepo' ? resticRepoRoot(path) : null
  if (repoRoot !== null) return repoRoot.split('/').filter(Boolean).at(-1) ?? repoRoot
  return archiveNameFromPath(path)
}

/** A fresh prompt's props for a locked `path`. */
function promptFor(info: {
  path: string
  parentVolumeId: string
  container: UnlockContainer
  wrongAttempt: boolean
  mode: UnlockPromptPropsData['mode']
  retry?: () => void
}): UnlockPromptPropsData {
  const name = containerName(info.container, info.path)
  return { ...info, name, checking: false, unreachable: false, rejections: 0 }
}

export function createUnlockFlow(deps: UnlockFlowDeps) {
  let showDialog = $state(false)
  let promptProps = $state<UnlockPromptPropsData | null>(null)
  /** The operation the prompt belongs to, for the mirror's re-sends. */
  let promptOperationId: string | null = null
  /** The repo check in flight, by the id the backend can cancel it by. A
   *  check whose id is no longer here was cancelled or outlived its prompt,
   *  and whatever it answers is ignored. */
  let pendingUnlockId: string | null = null

  /** Stores the password, logging (but not surfacing) a store failure: whatever
   *  retries next will simply re-prompt, since the password never landed. */
  async function storePassword(parentVolumeId: string, archivePath: string, password: string): Promise<void> {
    try {
      await setArchivePassword(parentVolumeId, archivePath, password)
    } catch (err) {
      log.warn('Failed to store archive password: {error}', { error: err })
    }
  }

  /** Mirrors WHAT THE PROMPT ASKS to the backend, so `cmdr://state` names the
   *  container and `unlock` has something to answer. A prompt on screen that
   *  nothing outside can see is exactly the blind spot this flow used to be.
   *  ❌ The password is not part of the mirror. */
  function mirror(props: UnlockPromptPropsData): void {
    void notifyArchivePasswordPrompt({
      archiveName: props.name,
      archivePath: props.path,
      parentVolumeId: props.parentVolumeId,
      container: props.container,
      mode: props.mode,
      wrongAttempt: props.wrongAttempt,
      operationId: promptOperationId,
    }).catch((err: unknown) => {
      // Only MCP loses out, and only until the next prompt. Never worth
      // surfacing to someone who is looking at the dialog.
      log.warn('Failed to mirror the password prompt: {error}', { error: err })
    })
  }

  /** Raises the prompt and mirrors it. */
  function showPrompt(props: UnlockPromptPropsData, operationId: string | null): void {
    promptProps = props
    promptOperationId = operationId
    showDialog = true
    mirror(props)
  }

  /** Takes the prompt down and clears the mirror with it, so nothing outside is
   *  told a question is still open. A check still in flight is orphaned: its
   *  answer finds no prompt and changes nothing here. */
  function hidePrompt(): void {
    showDialog = false
    promptProps = null
    promptOperationId = null
    pendingUnlockId = null
    void notifyArchivePasswordDismissed().catch((err: unknown) => {
      log.warn('Failed to clear the password-prompt mirror: {error}', { error: err })
    })
  }

  /** What an unlock leads to: browse re-lists, transfer re-dispatches. */
  function proceed(pw: UnlockPromptPropsData): void {
    hidePrompt()
    if (pw.mode === 'browse') pw.retry?.()
    else deps.redispatchBirthOperation()
  }

  /** Asks the repo whether `password` opens it, with the prompt still up. */
  async function checkRepoPassword(pw: UnlockPromptPropsData, password: string, remember: boolean): Promise<void> {
    const unlockId = crypto.randomUUID()
    pendingUnlockId = unlockId
    promptProps = { ...pw, checking: true, unreachable: false }

    let outcome: UnlockOutcome | null = null
    try {
      outcome = await unlockContainer({
        parentVolumeId: pw.parentVolumeId,
        path: pw.path,
        password,
        remember,
        unlockId,
      })
    } catch (err) {
      const refusal = failureOf(UnlockFailure, err)
      log.warn('Backup password check didn’t reach the repo ({reason}): {path}', {
        reason: refusal?.type ?? 'unexpected',
        path: pw.path,
      })
    }

    // Cancelled, answered over MCP, or swept while it ran: not ours to act on.
    if (pendingUnlockId !== unlockId) return
    pendingUnlockId = null
    // TypeScript narrows `promptProps` from the assignment before the `await`, but a
    // sweep or an MCP answer can clear it while the check runs, so keep the fallback.
    const current = (promptProps as UnlockPromptPropsData | null) ?? pw

    if (outcome === 'unlocked' || outcome === 'stored') {
      log.info('Backup unlocked: {path}', { path: pw.path })
      proceed(current)
      return
    }
    if (outcome === 'wrongPassword') {
      promptProps = {
        ...current,
        checking: false,
        wrongAttempt: true,
        rejections: current.rejections + 1,
      }
      mirror(promptProps)
      return
    }
    // It never reached the repo (gone, too slow, or refused to be asked), so it
    // said nothing about the password: keep it in the field and say so.
    promptProps = { ...current, checking: false, unreachable: true }
  }

  return {
    get showDialog(): boolean {
      return showDialog
    },
    get props(): UnlockPromptPropsData | null {
      return promptProps
    },

    /** Raises the transfer-time prompt: an operation the birth slot holds needs
     *  a password to read its source. The parent volume the container lives on
     *  is the source pane's volume id (a routed pane keeps its parent drive's
     *  id); the path is the errored source path. */
    promptForTransfer(info: {
      operationType: TransferOperationType
      parentVolumeId: string
      path: string
      /** What's locked, as the operation's `archive_needs_password` said. */
      container: UnlockContainer
      wrongAttempt: boolean
      /** The operation that hit the prompt. Already settled by the backend (a
       *  password failure settles it), so it's a correlation handle for whoever
       *  started the copy, never something to resume. */
      operationId: string | null
    }): void {
      log.info('{op} operation needs a password ({state}): {path}', {
        op: transferOpLabel(info.operationType),
        state: info.wrongAttempt ? 'rejected' : 'first prompt',
        path: info.path,
      })
      deps.setProgressDialogShown(false)
      showPrompt(
        promptFor({
          path: info.path,
          parentVolumeId: info.parentVolumeId,
          container: info.container,
          wrongAttempt: info.wrongAttempt,
          mode: 'transfer',
        }),
        info.operationId,
      )
    },

    /** Raises the browse-time prompt: a listing failed because it needs the
     *  password (a header-encrypted archive, a repo's snapshots). No operation
     *  is involved; on unlock `retry` re-lists the same directory. */
    promptForBrowse(info: BrowseUnlockRequest): void {
      log.info('Directory listing needs a password ({state}): {path}', {
        state: info.wrongAttempt ? 'rejected' : 'first prompt',
        path: info.path,
      })
      showPrompt(
        promptFor({
          path: info.path,
          parentVolumeId: info.volumeId,
          container: info.container,
          wrongAttempt: info.wrongAttempt,
          mode: 'browse',
          retry: info.retry,
        }),
        null,
      )
    },

    /** A person entered a password. An archive stores it and retries at once
     *  (a wrong one re-raises the prompt); a repo checks it first, with the
     *  prompt still up. `remember` is the Keychain checkbox, a repo's only. */
    handleSubmit(answer: { password: string; remember: boolean }): void {
      const pw = promptProps
      if (!pw || pw.checking) return

      // Transfer path: the re-dispatch is a NEW operation, so it re-scans (the
      // previous preview was consumed and the backend refuses a second claim on
      // one preview). Nothing to retry if the operation has gone away.
      if (pw.mode === 'transfer' && !deps.hasBirthContext()) return

      if (pw.container === 'resticRepo') {
        void checkRepoPassword(pw, answer.password, answer.remember)
        return
      }

      hidePrompt()
      void (async () => {
        await storePassword(pw.parentVolumeId, pw.path, answer.password)
        if (pw.mode === 'browse') pw.retry?.()
        else deps.redispatchBirthOperation()
      })()
    },

    /**
     * The MCP `unlock` tool supplied the password. The backend already
     * stored it (and for a repo, checked it), so all that is left is the
     * mode's follow-up.
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

      log.info('Password supplied over MCP: stored, and the transfer is settled rather than re-dispatched')
      hidePrompt()
      if (deps.hasBirthContext()) deps.settleBirthOperation()
      deps.onRefocus()
    },

    /** The user dismissed the prompt, possibly mid-check: drop the check, then
     *  forget any password the container holds. Browse mode just closes it,
     *  leaving the "needs a password" fallback pane in place (the loader already
     *  settled it), so the user simply doesn't get in. Transfer mode settles the
     *  operation exactly as a dismissed transfer error would, so nothing looks
     *  stuck. */
    handleCancel(): void {
      const pw = promptProps
      const unlockId = pendingUnlockId
      if (pw) {
        void (async () => {
          // The cancel first: a check finishing between a forget and a later
          // cancel would keep the password behind the forget's back.
          if (unlockId) await cancelContainerUnlock(unlockId)
          await forgetContainerPassword(pw.parentVolumeId, pw.path)
        })().catch((err: unknown) => {
          log.warn('Failed to drop the password after a cancelled prompt: {error}', { error: err })
        })
      }

      if (pw?.mode === 'browse') {
        log.info('Browse password prompt cancelled')
        hidePrompt()
        deps.onRefocus()
        return
      }

      hidePrompt()
      deps.settleBirthOperation()
      deps.onRefocus()
    },

    /** Clears the prompt with no password call, for the render-failure sweep.
     *  A check in flight is dropped, and the mirror still goes: a prompt
     *  nothing will ever answer must not be left advertised in `cmdr://state`. */
    forget(): void {
      if (pendingUnlockId) {
        void cancelContainerUnlock(pendingUnlockId).catch((err: unknown) => {
          log.warn('Failed to drop a password check: {error}', { error: err })
        })
      }
      hidePrompt()
    },
  }
}
