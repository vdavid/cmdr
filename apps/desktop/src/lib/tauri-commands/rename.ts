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

/**
 * Which trash directory holds items trashed from `path`'s volume (macOS keeps one
 * per volume). `null` means that volume has no trash to go to, which is an answer,
 * not a refusal: a volume nobody has trashed to yet, or one with no trash at all.
 *
 * `path` need not still exist — the resolver climbs to a live ancestor on the same
 * volume, which is what makes it usable on an item that was just trashed away.
 */
export async function getTrashDir(path: string): Promise<string | null> {
  const res = await commands.getTrashDir(path)
  if (res.status === 'error') throwMutationError(res.error)
  return res.data
}
