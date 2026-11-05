/**
 * Asks the engine whether a delete, move, or rename would touch the person's own
 * home folder (or a folder holding it) BEFORE its confirm dialog or the rename
 * editor opens, so no confirm UI ever shows for a refused one and no Enter can
 * act on it. The rule is Rust's (`crates/cmdr-operations/src/home_folder.rs`);
 * each start refuses the same way, so a lost answer only costs the early word.
 */
import { checkHomeFolderGuard } from '#lib/tauri-commands/index.js'
import type { HomeFolderAction } from '#lib/ipc/bindings.js'
import type { TransferOperationType, WriteOperationError } from '#lib/file-explorer/types.js'
import { getUserFriendlyMessage } from '#lib/file-operations/transfer/transfer-error-messages.js'
import { refuseOpening } from './operation-start-gate'

/** The dialog that words the refusal, as `DialogState.showHomeFolderRefusal` opens it. */
export interface HomeFolderRefusalSurface {
  showHomeFolderRefusal: (operationType: TransferOperationType, error: WriteOperationError) => void
}

/** The engine's typed refusal, or `null` when the action may go ahead (or nobody could answer). */
export async function homeFolderRefusalFor(
  sources: string[],
  volumeId: string,
  action: HomeFolderAction,
): Promise<WriteOperationError | null> {
  try {
    if (sources.length === 0) return null
    return await checkHomeFolderGuard(sources, volumeId, action)
  } catch {
    return null
  }
}

/** The operation type the error dialog files a refusal under. A rename has none of
 *  its own there; its words come from the error's `action`, not from this. */
function operationTypeFor(action: HomeFolderAction): TransferOperationType {
  return action === 'delete' ? 'delete' : 'move'
}

/**
 * Asks, and when refused, says why in place of the dialog: the error dialog for a
 * person, a typed `homeFolder` refusal for an MCP request. Resolves `true` when
 * the action may go on to its dialog.
 */
export async function passesHomeFolderGuard(
  surface: HomeFolderRefusalSurface,
  request: { sources: string[]; volumeId: string; action: HomeFolderAction; mcpRequestId?: string },
): Promise<boolean> {
  const refusal = await homeFolderRefusalFor(request.sources, request.volumeId, request.action)
  if (refusal === null) return true
  const operationType = operationTypeFor(request.action)
  const words = getUserFriendlyMessage(refusal, operationType)
  refuseOpening(request.mcpRequestId, { kind: 'homeFolder', message: words.title }, () => {
    surface.showHomeFolderRefusal(operationType, refusal)
  })
  return false
}
