// Rename-related Tauri command wrappers

import { commands, type Initiator, type ValidationError } from '$lib/ipc/bindings'
import { throwMutationError } from '$lib/file-operations/mutation-error'

export interface RenameConflictFileInfo {
  name: string
  size: number
  /** Unix timestamp in seconds, or null if unavailable. */
  modified: number | null
  isDirectory: boolean
}

export interface RenameValidityResult {
  valid: boolean
  error: ValidationError | null
  hasConflict: boolean
  isCaseOnlyRename: boolean
  conflict: RenameConflictFileInfo | null
}

/** Throws a `MutationFailure` carrying the backend's typed refusal. */
export async function checkRenamePermission(path: string): Promise<void> {
  const res = await commands.checkRenamePermission(path)
  if (res.status === 'error') throwMutationError(res.error)
}

export async function checkRenameValidity(
  dir: string,
  oldName: string,
  newName: string,
  volumeId?: string,
): Promise<RenameValidityResult> {
  const res = await commands.checkRenameValidity(dir, oldName, newName, volumeId ?? null)
  if (res.status === 'error') throwMutationError(res.error)
  return res.data
}

export async function renameFile(
  from: string,
  to: string,
  force: boolean,
  volumeId?: string,
  initiator?: Initiator,
): Promise<void> {
  const res = await commands.renameFile(from, to, force, volumeId ?? null, initiator ?? null)
  // Typed all the way to the surface that words it: `throwIpcError` would flatten
  // a `MutationError` into a JSON string, which is what this path exists to end.
  if (res.status === 'error') throwMutationError(res.error)
}

export async function moveToTrash(path: string): Promise<void> {
  const res = await commands.moveToTrash(path)
  if (res.status === 'error') throwMutationError(res.error)
}
