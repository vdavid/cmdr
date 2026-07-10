// Rename-related Tauri command wrappers

import { commands, type Initiator, type ValidationError } from '$lib/ipc/bindings'
import { throwIpcError } from './ipc-types'

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

export async function checkRenamePermission(path: string): Promise<void> {
  const res = await commands.checkRenamePermission(path)
  if (res.status === 'error') throwIpcError(res.error)
}

export async function checkRenameValidity(
  dir: string,
  oldName: string,
  newName: string,
  volumeId?: string,
): Promise<RenameValidityResult> {
  const res = await commands.checkRenameValidity(dir, oldName, newName, volumeId ?? null)
  if (res.status === 'error') throwIpcError(res.error)
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
  if (res.status === 'error') throwIpcError(res.error)
}

export async function moveToTrash(path: string): Promise<void> {
  const res = await commands.moveToTrash(path)
  if (res.status === 'error') throwIpcError(res.error)
}
