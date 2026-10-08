// Rename-related Tauri command wrappers

import { commands, type Initiator, type RenameByMove, type ValidationError } from '$lib/ipc/bindings'
import { throwMutationError } from '$lib/file-operations/mutation-error'
import { awaitMutation, type MutationWaitOptions } from './mutation-reply'

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
  /** Set only when this rename copies (an S3 folder or big file): what it would
   *  move, and whether it's big enough to confirm in the Move dialog first. */
  byMove: RenameByMove | null
}

/**
 * Throws a `MutationFailure` carrying the backend's typed refusal.
 *
 * `volumeId` is what decides whether the check runs at all: it's an `lstat` +
 * `access` pair, which means nothing on a volume that serves its own I/O. Pass
 * the pane's volume, never omit it for a remote one.
 */
export async function checkRenamePermission(path: string, volumeId?: string): Promise<void> {
  const res = await commands.checkRenamePermission(path, volumeId ?? null)
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

/**
 * Renames, resolving once the rename landed however slow the volume is;
 * `wait.onStillRunning` says when it's being slow (`./mutation-reply.ts`). A
 * refusal throws typed, all the way to the surface that words it.
 */
export async function renameFile(
  from: string,
  to: string,
  force: boolean,
  volumeId?: string,
  initiator?: Initiator,
  wait?: MutationWaitOptions,
): Promise<void> {
  await awaitMutation(() => commands.renameFile(from, to, force, volumeId ?? null, initiator ?? null), wait)
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
