// Rename-related Tauri command wrappers

import { invoke } from '@tauri-apps/api/core'

export interface RenameConflictFileInfo {
    name: string
    size: number
    /** Unix timestamp in seconds, or null if unavailable. */
    modified: number | null
    isDirectory: boolean
}

export interface RenameValidityResult {
    valid: boolean
    error: { type: string; message: string } | null
    hasConflict: boolean
    isCaseOnlyRename: boolean
    conflict: RenameConflictFileInfo | null
}

export function checkRenamePermission(path: string): Promise<void> {
    return invoke('check_rename_permission', { path })
}

export function checkRenameValidity(dir: string, oldName: string, newName: string): Promise<RenameValidityResult> {
    return invoke<RenameValidityResult>('check_rename_validity', { dir, oldName, newName })
}

export function renameFile(from: string, to: string, force: boolean): Promise<void> {
    return invoke('rename_file', { from, to, force })
}

export function moveToTrash(path: string): Promise<void> {
    return invoke('move_to_trash', { path })
}
