/**
 * The MCP `dialog_confirm`: confirming an open dialog from outside it.
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

import { conflictPolicyFromMcpName } from '#lib/file-operations/transfer/conflict-policy.js'
import { getAppLogger } from '#lib/logging/logger.js'
import type {
  ConfirmOptions,
  ConfirmPress,
  CreateConfirmer,
  CreatePress,
  DeleteConfirmer,
  TransferConfirmer,
} from './dialog-props'

const log = getAppLogger('fileExplorer')

/**
 * What a programmatic confirm did, for the MCP reply. Mirrored by `ConfirmAck`
 * in `src-tauri/src/tools/actions/dialogs.rs`: the refusal crosses as this typed
 * field, ❌ never as message text. `pressed` reached the start, and the start
 * answers the round-trip with its operation id (`options.mcpRequestId`).
 */
export type ProgrammaticConfirmVerdict =
  | { pressed: true }
  /** A rename or a create has no start to answer, so its own end does. */
  | { pressed: true; committed: true; stillRunning: boolean }
  | {
      pressed: false
      /** `permanentDelete`: a background press on a dialog that would delete
       *  permanently. `notStarted`: the dialog took the press and a guard of its
       *  own started nothing. `notReady`: no such dialog is open and mounted.
       *  `needsMoveDialog`: the rename moved on to the Move dialog. */
      refusal: 'permanentDelete' | 'notStarted' | 'notReady' | 'needsMoveDialog'
    }
  /** The name didn't stick, and the field stays open. `reason` is what it would show a person. */
  | { pressed: false; refusal: 'nameRefused'; reason: string }

function verdictFor(press: ConfirmPress): ProgrammaticConfirmVerdict {
  switch (press) {
    case 'pressed':
      return PRESSED
    case 'notStarted':
      return { pressed: false, refusal: 'notStarted' }
    case 'refusedPermanentDelete':
      return { pressed: false, refusal: 'permanentDelete' }
  }
}

/** A rename's or a create's end, as the MCP reply reads it. */
export function verdictForNamePress(press: CreatePress): ProgrammaticConfirmVerdict {
  switch (press.kind) {
    case 'done':
      return { pressed: true, committed: true, stillRunning: false }
    case 'stillRunning':
      return { pressed: true, committed: true, stillRunning: true }
    case 'refused':
      return { pressed: false, refusal: 'nameRefused', reason: press.reason }
    case 'needsMoveDialog':
      return { pressed: false, refusal: 'needsMoveDialog' }
  }
}

const PRESSED: ProgrammaticConfirmVerdict = { pressed: true }
const NOT_READY: ProgrammaticConfirmVerdict = { pressed: false, refusal: 'notReady' }

export interface ProgrammaticConfirmDeps {
  /** Whether the transfer confirmation is on screen. */
  isTransferDialogOpen: () => boolean
  /** Whether the delete confirmation is on screen. */
  isDeleteDialogOpen: () => boolean
  /** Whether the new folder dialog is on screen. */
  isNewFolderDialogOpen: () => boolean
  /** Whether the new file dialog is on screen. */
  isNewFileDialogOpen: () => boolean
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
  const newFolder = confirmerSlot<CreateConfirmer>()
  const newFile = confirmerSlot<CreateConfirmer>()

  async function confirmTransfer(onConflict: string | undefined, options: ConfirmOptions) {
    // A policy the backend accepted but the map doesn't know mustn't become some
    // other policy over somebody's files, so it starts nothing. The backend
    // validates the name, so this can only fire when the two lists have drifted.
    const mapped = conflictPolicyFromMcpName(onConflict)
    if (onConflict !== undefined && mapped === undefined) {
      log.warn('Unknown conflict policy {onConflict} on a programmatic confirm; nothing confirmed', { onConflict })
      return verdictFor('notStarted')
    }
    const press = transfer.current
    if (!press) {
      log.warn('A programmatic confirm found the transfer dialog open but not mounted yet; nothing confirmed')
      return NOT_READY
    }
    // No policy named: the dialog keeps the one it shows, which may be a person's remembered choice.
    return verdictFor(await press(mapped, options))
  }

  async function pressNamingDialog(press: CreateConfirmer | null, name: string | undefined) {
    return press ? verdictForNamePress(await press(name)) : NOT_READY
  }

  return {
    registerTransferConfirmer: transfer.register,
    registerDeleteConfirmer: deletion.register,
    registerNewFolderConfirmer: newFolder.register,
    registerNewFileConfirmer: newFile.register,

    /** Programmatically confirm an open dialog (for MCP confirm action). With
     *  `startInBackground` it presses the dialog's Background button (F2)
     *  instead, under the same rules. Resolves once the press reached the start
     *  or a guard, and the verdict says which: the MCP tool answers the agent
     *  from it (`tools/actions/dialog_confirm.rs`). The rename editor belongs to a
     *  pane, so `pane-commands.ts` answers for it. */
    async confirmOpenDialog(
      dialogType: string,
      onConflict?: string,
      options: ConfirmOptions = {},
    ): Promise<ProgrammaticConfirmVerdict> {
      if (dialogType === 'transfer-confirmation' && deps.isTransferDialogOpen()) {
        return confirmTransfer(onConflict, options)
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
        return verdictFor(await press(options))
      } else if (dialogType === 'mkdir-confirmation' && deps.isNewFolderDialogOpen()) {
        return pressNamingDialog(newFolder.current, options.name)
      } else if (dialogType === 'new-file-confirmation' && deps.isNewFileDialogOpen()) {
        return pressNamingDialog(newFile.current, options.name)
      } else if (dialogType === 'archive-password' && deps.isArchivePasswordOpen()) {
        // The `unlock` tool already stored the password on the backend;
        // this is the follow-up. ⚠️ It settles a transfer rather than
        // re-dispatching it — see `supplyStoredPassword`.
        deps.supplyStoredPassword()
        return PRESSED
      }
      return NOT_READY
    },
  }
}
