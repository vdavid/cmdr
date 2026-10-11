/**
 * The MCP `dialog confirm`: confirming an open dialog from outside it.
 *
 * Each dialog kind is confirmed the way that dialog confirms itself, which for
 * the transfer and delete dialogs means pressing their OWN confirm. The mounted
 * `TransferDialog` / `DeleteDialog` registers the function its button runs, and
 * this presses it (the transfer one under the conflict policy the agent named),
 * ❌ never a payload built from the props the dialog opened with. The props hold
 * what the dialog OPENED with; the dialog holds what it will actually send (the
 * edited path, the picked volume, the scan preview, the trash switch). For a
 * compress the two differ from the first frame: the box names
 * `<folder>/<name>.zip` and the props only the folder, which the backend refuses
 * as a read-only destination.
 *
 * The factory owns the registered confirms and nothing else. What's open, and how
 * a password prompt is answered, stays with the dialog state that builds it
 * (`dialog-state.svelte.ts`), handed in as `deps`.
 */

import { conflictPolicyFromMcpName } from '$lib/file-operations/transfer/conflict-policy'
import { getAppLogger } from '$lib/logging/logger'
import type { ConfirmOptions, DeleteConfirmer, TransferConfirmer } from './dialog-props'

const log = getAppLogger('fileExplorer')

/**
 * What a programmatic confirm did, for the MCP reply. Mirrored by
 * `BackgroundConfirmAck` in `src-tauri/src/mcp/executor/dialogs.rs`: the refusal
 * crosses as this typed field, ❌ never as message text.
 */
export type ProgrammaticConfirmVerdict =
  | { pressed: true }
  | {
      pressed: false
      /** `permanentDelete`: a background press on a dialog that would delete
       *  permanently. `notReady`: no such dialog is open and mounted. */
      refusal: 'permanentDelete' | 'notReady'
    }

const PRESSED: ProgrammaticConfirmVerdict = { pressed: true }
const NOT_READY: ProgrammaticConfirmVerdict = { pressed: false, refusal: 'notReady' }

export interface ProgrammaticConfirmDeps {
  /** Whether the transfer confirmation is on screen. */
  isTransferDialogOpen: () => boolean
  /** Whether the delete confirmation is on screen. */
  isDeleteDialogOpen: () => boolean
  /** Whether the archive-password prompt is on screen. */
  isArchivePasswordOpen: () => boolean
  supplyStoredPassword: () => void
}

/**
 * One mounted dialog's own confirm. `register` is called by the dialog as it
 * mounts, with the function its confirm button runs, and returns the unregister
 * for its teardown, which leaves a newer dialog's registration alone.
 */
function confirmerSlot<Confirm>() {
  let current: Confirm | null = null
  return {
    register: (confirm: Confirm): (() => void) => {
      current = confirm
      return () => {
        if (current === confirm) current = null
      }
    },
    get current(): Confirm | null {
      return current
    },
  }
}

export function createProgrammaticConfirm(deps: ProgrammaticConfirmDeps) {
  const transfer = confirmerSlot<TransferConfirmer>()
  const deletion = confirmerSlot<DeleteConfirmer>()

  return {
    registerTransferConfirmer: transfer.register,
    registerDeleteConfirmer: deletion.register,

    /** Programmatically confirm an open dialog (for MCP confirm action). With
     *  `startInBackground` it presses the dialog's Background button (F2)
     *  instead, under the same rules, and the verdict says whether it did: the
     *  MCP tool answers the agent from it (`mcp/executor/dialogs.rs`). */
    confirmOpenDialog(
      dialogType: string,
      onConflict?: string,
      options: ConfirmOptions = {},
    ): ProgrammaticConfirmVerdict {
      if (dialogType === 'transfer-confirmation' && deps.isTransferDialogOpen()) {
        // A policy the backend accepted but the map doesn't know would quietly
        // become `skip`, so an agent that asked to be asked per file would
        // instead watch every clash get skipped. Say so; the backend validates
        // the name, so this can only fire when the two lists have drifted.
        const mapped = conflictPolicyFromMcpName(onConflict)
        if (onConflict !== undefined && mapped === undefined) {
          log.warn('Unknown conflict policy {onConflict} on a programmatic confirm; falling back to skip', {
            onConflict,
          })
        }
        const press = transfer.current
        if (!press) {
          log.warn('A programmatic confirm found the transfer dialog open but not mounted yet; nothing confirmed')
          return NOT_READY
        }
        press(mapped ?? 'skip', options)
        return PRESSED
      } else if (dialogType === 'delete-confirmation' && deps.isDeleteDialogOpen()) {
        // Same press as the button: the scan preview the dialog started and the
        // mode it shows now, including a trash the walk turned into a delete (then
        // the press is handed back, as it is for a person).
        const press = deletion.current
        if (!press) {
          log.warn('A programmatic confirm found the delete dialog open but not mounted yet; nothing confirmed')
          return NOT_READY
        }
        // A background press on a dialog that would delete permanently is refused by
        // the dialog itself (it's the one operation nothing undoes), typed.
        return press(options) === 'refusedPermanentDelete' ? { pressed: false, refusal: 'permanentDelete' } : PRESSED
      } else if (dialogType === 'archive-password' && deps.isArchivePasswordOpen()) {
        // The `unlock_archive` tool already stored the password on the backend;
        // this is the follow-up. ⚠️ It settles a transfer rather than
        // re-dispatching it — see `supplyStoredPassword`.
        deps.supplyStoredPassword()
        return PRESSED
      }
      return NOT_READY
    },
  }
}
