/**
 * Rename save flow: trim -> validate -> extension check -> conflict check -> backend rename.
 * Pure logic module. Returns instructions instead of calling Tauri directly,
 * except for the actual backend calls which are awaited.
 */

import { extensionsDifferMeaningfully, getExtension } from '$lib/utils/filename-validation'
import { tString } from '$lib/intl/messages.svelte'

export interface ConflictFileInfo {
  name: string
  size: number
  /** Unix timestamp in seconds, or null/undefined if unavailable. Group A wire-format: IPC sends `null`. */
  modifiedAt: number | null | undefined
}

export type RenameConflictResolution = 'overwrite-trash' | 'overwrite-delete' | 'cancel' | 'continue'
import { checkRenamePermission, checkRenameValidity, renameFile, type RenameValidityResult } from '$lib/tauri-commands'
import { asMutationError, isMutationTimeout } from '$lib/file-operations/mutation-error'
import { renderMutationError } from '$lib/file-operations/mutation-error-messages'
import { getAppLogger } from '$lib/logging/logger'
import type { RenameTarget } from './rename-state.svelte'
import type { ExtensionChangePolicy } from '$lib/settings'

const log = getAppLogger('rename')

export type RenameResult =
  | { type: 'noop' }
  | { type: 'error'; message: string }
  /** The volume never confirmed the rename. It may still have landed on disk. */
  | { type: 'timeout' }
  | { type: 'extension-ask'; oldExtension: string; newExtension: string }
  | { type: 'conflict'; validity: RenameValidityResult }
  | { type: 'success'; newName: string }

/**
 * Words for the backend's typed verdict on a name it won't take.
 *
 * The same catalog messages the editor's live validation uses, so a name the
 * backend turns down reads the way the red border already read. The backend
 * names the offending character; the message doesn't, because it lists the
 * whole forbidden set instead, which is what the user needs in order to fix it.
 */
function validityMessage(error: RenameValidityResult['error'], isDirectory: boolean): string {
  const kind = isDirectory ? 'folder' : 'file'
  switch (error?.kind) {
    case 'empty':
      return tString('fileOperations.validation.empty', { kind })
    case 'disallowedCharacter':
      return tString('fileOperations.validation.disallowedChars', { kind })
    case 'nameTooLong':
      return tString('fileOperations.validation.nameTooLong', {
        kind,
        byteCount: String(error.bytes),
        maxBytes: String(error.max),
      })
    case 'pathTooLong':
      return tString('fileOperations.validation.pathTooLong', {
        byteCount: String(error.bytes),
        maxBytes: String(error.max),
      })
    default:
      return tString('fileOperations.validation.nameNotUsable', { kind })
  }
}

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

  // Check extension change (case-only and known-equivalent changes are silently allowed)
  if (
    !skipExtensionCheck &&
    extensionPolicy === 'ask' &&
    extensionsDifferMeaningfully(target.originalName, trimmedName)
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
    // Same typed vocabulary as the rename itself, so a validity check that can't
    // run reads in the user's own language rather than in `diskutil` English.
    return { type: 'error', message: renameFailureMessage(e, target.isDirectory) }
  }

  if (!validity.valid) {
    return { type: 'error', message: validityMessage(validity.error, target.isDirectory) }
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
    // The caller words this one: it aggregates a run of them into a single
    // toast, so the sentence depends on how many are waiting to be reported.
    if (isMutationTimeout(e)) return { type: 'timeout' }
    return { type: 'error', message: renameFailureMessage(e, target.isDirectory) }
  }
}

/**
 * The one sentence a rename refusal says.
 *
 * The backend's answer is typed, so the words come from the catalog in the
 * user's own language. A value that never reached the typed path at all (a
 * thrown `Error` from the IPC layer itself) reads as the same honest fallback
 * the backend would have sent, with the raw value logged instead of shown.
 */
function renameFailureMessage(e: unknown, isDirectory: boolean): string {
  const failure = asMutationError(e)
  if (failure) return renderMutationError(failure, isDirectory ? 'folder' : 'file')
  log.warn('A rename call threw an untyped value: {error}', { error: String(e) })
  return renderMutationError({ type: 'unexpected', detail: '' }, isDirectory ? 'folder' : 'file')
}

/** Checks rename permission and returns a message to show, or null if permitted. */
export async function checkPermission(path: string, isDirectory = false): Promise<string | null> {
  try {
    await checkRenamePermission(path)
    return null
  } catch (e) {
    return renameFailureMessage(e, isDirectory)
  }
}
