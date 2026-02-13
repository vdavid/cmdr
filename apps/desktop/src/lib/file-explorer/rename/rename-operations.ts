/**
 * Rename save flow: trim -> validate -> extension check -> conflict check -> backend rename.
 * Pure logic module — returns instructions instead of calling Tauri directly,
 * except for the actual backend calls which are awaited.
 */

import { getExtension } from '$lib/utils/filename-validation'
import { checkRenamePermission, checkRenameValidity, renameFile, type RenameValidityResult } from '$lib/tauri-commands'
import type { RenameTarget } from './rename-state.svelte'
import type { ExtensionChangePolicy } from '$lib/settings'

export type RenameResult =
    | { type: 'noop' }
    | { type: 'error'; message: string }
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
): Promise<RenameResult> {
    // No-op if name unchanged
    if (trimmedName === target.originalName) {
        return { type: 'noop' }
    }

    // Check extension change
    if (!skipExtensionCheck) {
        const oldExt = getExtension(target.originalName)
        const newExt = getExtension(trimmedName)
        if (oldExt !== newExt && extensionPolicy === 'ask') {
            return {
                type: 'extension-ask',
                oldExtension: oldExt.replace(/^\./, ''),
                newExtension: newExt.replace(/^\./, ''),
            }
        }
    }

    // Backend validity check (authoritative, checks conflicts via inode comparison)
    let validity: RenameValidityResult
    try {
        validity = await checkRenameValidity(target.parentPath, target.originalName, trimmedName)
    } catch (e) {
        return { type: 'error', message: e instanceof Error ? e.message : String(e) }
    }

    if (!validity.valid) {
        return { type: 'error', message: validity.error?.message ?? 'Invalid filename' }
    }

    // Conflict detected (and not a case-only rename of the same file)
    if (validity.hasConflict && !validity.isCaseOnlyRename) {
        return { type: 'conflict', validity }
    }

    // Perform the rename
    return performRename(target, trimmedName, false)
}

/**
 * Performs the actual rename call.
 * @param force - If true, overwrites the destination (used after conflict resolution).
 */
export async function performRename(target: RenameTarget, newName: string, force: boolean): Promise<RenameResult> {
    const fromPath = target.path
    const toPath = target.parentPath + '/' + newName

    try {
        await renameFile(fromPath, toPath, force)
        return { type: 'success', newName }
    } catch (e) {
        return { type: 'error', message: e instanceof Error ? e.message : String(e) }
    }
}

/** Checks rename permission and returns an error message, or null if permitted. */
export async function checkPermission(path: string): Promise<string | null> {
    try {
        await checkRenamePermission(path)
        return null
    } catch (e) {
        return e instanceof Error ? e.message : String(e)
    }
}
