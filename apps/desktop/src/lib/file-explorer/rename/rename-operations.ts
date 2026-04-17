/**
 * Rename save flow: trim -> validate -> extension check -> conflict check -> backend rename.
 * Pure logic module — returns instructions instead of calling Tauri directly,
 * except for the actual backend calls which are awaited.
 */

import { extensionsDifferIgnoringCase, getExtension } from '$lib/utils/filename-validation'

export interface ConflictFileInfo {
  name: string
  size: number
  /** Unix timestamp in seconds, or undefined if unavailable */
  modifiedAt: number | undefined
}

export type RenameConflictResolution = 'overwrite-trash' | 'overwrite-delete' | 'cancel' | 'continue'
import {
  checkRenamePermission,
  checkRenameValidity,
  getIpcErrorMessage,
  isIpcError,
  renameFile,
  type RenameValidityResult,
} from '$lib/tauri-commands'
import type { RenameTarget } from './rename-state.svelte'
import type { ExtensionChangePolicy } from '$lib/settings'

export type RenameResult =
  | { type: 'noop' }
  | { type: 'error'; message: string }
  | { type: 'timeout'; message: string }
  | { type: 'extension-ask'; oldExtension: string; newExtension: string }
  | { type: 'conflict'; validity: RenameValidityResult }
  | { type: 'success'; newName: string }

/**
 * Runs the full rename save flow.
 * Stops at the first point requiring user interaction (extension dialog or conflict dialog).
 */
export async function executeRenameSave(
  target: RenameTarget,
  trimmedName: string,
  extensionPolicy: ExtensionChangePolicy,
  skipExtensionCheck?: boolean,
  volumeId?: string,
): Promise<RenameResult> {
  // No-op if name unchanged
  if (trimmedName === target.originalName) {
    return { type: 'noop' }
  }

  // Check extension change (case-only changes are silently allowed)
  if (
    !skipExtensionCheck &&
    extensionPolicy === 'ask' &&
    extensionsDifferIgnoringCase(target.originalName, trimmedName)
  ) {
    return {
      type: 'extension-ask',
      oldExtension: getExtension(target.originalName).replace(/^\./, ''),
      newExtension: getExtension(trimmedName).replace(/^\./, ''),
    }
  }

  // Backend validity check (authoritative, checks conflicts via inode comparison on local FS,
  // or Volume trait's get_metadata on MTP and other non-local volumes)
  let validity: RenameValidityResult
  try {
    validity = await checkRenameValidity(target.parentPath, target.originalName, trimmedName, volumeId)
  } catch (e) {
    return { type: 'error', message: getIpcErrorMessage(e) }
  }

  if (!validity.valid) {
    return { type: 'error', message: validity.error?.message ?? 'Invalid filename' }
  }

  // Conflict detected (and not a case-only rename of the same file)
  if (validity.hasConflict && !validity.isCaseOnlyRename) {
    return { type: 'conflict', validity }
  }

  // Perform the rename
  return performRename(target, trimmedName, false, volumeId)
}

/**
 * Performs the actual rename call.
 * @param force - If true, overwrites the destination (used after conflict resolution).
 */
export async function performRename(
  target: RenameTarget,
  newName: string,
  force: boolean,
  volumeId?: string,
): Promise<RenameResult> {
  const fromPath = target.path
  const toPath = target.parentPath + '/' + newName

  try {
    await renameFile(fromPath, toPath, force, volumeId)
    return { type: 'success', newName }
  } catch (e) {
    if (isIpcError(e) && e.timedOut) {
      return {
        type: 'timeout',
        message: "Couldn't confirm the rename completed. The volume may be slow — the file may have been renamed.",
      }
    }
    return { type: 'error', message: getIpcErrorMessage(e) }
  }
}

/** Checks rename permission and returns an error message, or null if permitted. */
export async function checkPermission(path: string): Promise<string | null> {
  try {
    await checkRenamePermission(path)
    return null
  } catch (e) {
    return getIpcErrorMessage(e)
  }
}
