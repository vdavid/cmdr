/** Client-side filename validation for instant keystroke feedback. */

/** Defaults match macOS. Call initPathLimits() at startup to get platform-specific values from the backend. */
let maxNameBytes = 255
let maxPathBytes = 1024

/** Fetches platform-specific limits from the backend. Call once at app startup. */
export async function initPathLimits(): Promise<void> {
    const { getPathLimits } = await import('$lib/tauri-commands')
    const limits = await getPathLimits()
    maxNameBytes = limits.maxNameBytes
    maxPathBytes = limits.maxPathBytes
}

/** Characters disallowed on macOS (APFS/HFS+). TODO: Per-OS logic for future platforms. */
const DISALLOWED_CHARS_REGEX = /[/\0]/

export type ValidationSeverity = 'error' | 'warning' | 'ok'

export interface ValidationResult {
    severity: ValidationSeverity
    message: string
}

const OK_RESULT: ValidationResult = { severity: 'ok', message: '' }

function nameLabel(isDir: boolean): string {
    return isDir ? 'Folder name' : 'Filename'
}

/** Validates a file or dir name for disallowed characters. Operates on trimmed value. */
export function validateDisallowedChars(name: string, isDir = false): ValidationResult {
    if (DISALLOWED_CHARS_REGEX.test(name)) {
        return { severity: 'error', message: `${nameLabel(isDir)} can't contain "/" or null characters` }
    }
    return OK_RESULT
}

/** Validates that a file or dir name is not empty after trimming. */
export function validateNotEmpty(name: string, isDir = false): ValidationResult {
    if (name.trim().length === 0) {
        return { severity: 'error', message: `${nameLabel(isDir)} can't be empty` }
    }
    return OK_RESULT
}

/** Validates name byte length (max 255 bytes). */
export function validateNameLength(name: string, isDir = false): ValidationResult {
    const byteLength = new TextEncoder().encode(name.trim()).length
    if (byteLength >= maxNameBytes) {
        return {
            severity: 'error',
            message: `${nameLabel(isDir)} is too long (${String(byteLength)}/${String(maxNameBytes)} bytes)`,
        }
    }
    return OK_RESULT
}

/** Validates full path byte length (max 1024 bytes). */
export function validatePathLength(parentPath: string, name: string): ValidationResult {
    const fullPath = parentPath.endsWith('/') ? parentPath + name.trim() : parentPath + '/' + name.trim()
    const byteLength = new TextEncoder().encode(fullPath).length
    if (byteLength >= maxPathBytes) {
        return {
            severity: 'error',
            message: `Full path is too long (${String(byteLength)}/${String(maxPathBytes)} bytes)`,
        }
    }
    return OK_RESULT
}

/** Extracts the extension from a filename (empty string if none). */
export function getExtension(filename: string): string {
    const lastDot = filename.lastIndexOf('.')
    if (lastDot <= 0) return ''
    return filename.substring(lastDot)
}

/** Validates extension change against the user's preference. */
export function validateExtensionChange(
    oldName: string,
    newName: string,
    allowExtensionChanges: 'yes' | 'no' | 'ask',
): ValidationResult {
    if (allowExtensionChanges === 'yes') return OK_RESULT

    const oldExt = getExtension(oldName)
    const newExt = getExtension(newName.trim())

    if (oldExt === newExt) return OK_RESULT

    if (allowExtensionChanges === 'no') {
        return { severity: 'error', message: `Changing the file extension isn't allowed (was "${oldExt}")` }
    }
    // 'ask' — no error, the dialog will handle it on save
    return OK_RESULT
}

/**
 * Checks if the new name matches an existing sibling (case-insensitive on APFS).
 * Exception: case-only rename of the same file produces no warning.
 */
export function validateConflict(newName: string, siblingNames: string[], originalName: string): ValidationResult {
    const trimmed = newName.trim()
    const trimmedLower = trimmed.toLowerCase()
    const originalLower = originalName.toLowerCase()

    for (const sibling of siblingNames) {
        if (sibling.toLowerCase() === trimmedLower) {
            // Case-only rename of the same file — no warning
            if (sibling.toLowerCase() === originalLower) continue
            return { severity: 'warning', message: `"${trimmed}" already exists in this folder` }
        }
    }
    return OK_RESULT
}

/** Validates a full directory path (not a filename). Checks structure only, not existence. */
export function validateDirectoryPath(path: string): ValidationResult {
    const trimmed = path.trim()

    if (trimmed.length === 0) {
        return { severity: 'error', message: "Path can't be empty" }
    }

    if (!trimmed.startsWith('/')) {
        return { severity: 'error', message: 'Path must be absolute (start with /)' }
    }

    if (trimmed.includes('\0')) {
        return { severity: 'error', message: 'Path contains a null character' }
    }

    const encoder = new TextEncoder()
    const totalBytes = encoder.encode(trimmed).length
    if (totalBytes >= maxPathBytes) {
        return {
            severity: 'error',
            message: `Path is too long (${String(totalBytes)}/${String(maxPathBytes)} bytes)`,
        }
    }

    const segments = trimmed.split('/').filter((s) => s.length > 0)
    for (const segment of segments) {
        const segmentBytes = encoder.encode(segment).length
        if (segmentBytes >= maxNameBytes) {
            return {
                severity: 'error',
                message: `A folder name in the path is too long (${String(segmentBytes)}/${String(maxNameBytes)} bytes)`,
            }
        }
    }

    return OK_RESULT
}

/** Runs all validation checks and returns the first error, then the first warning, or ok. */
export function validateFilename(
    newName: string,
    originalName: string,
    parentPath: string,
    siblingNames: string[],
    allowExtensionChanges: 'yes' | 'no' | 'ask',
): ValidationResult {
    const trimmed = newName.trim()

    // Error checks
    const emptyCheck = validateNotEmpty(newName)
    if (emptyCheck.severity === 'error') return emptyCheck

    const charCheck = validateDisallowedChars(trimmed)
    if (charCheck.severity === 'error') return charCheck

    const nameLen = validateNameLength(newName)
    if (nameLen.severity === 'error') return nameLen

    const pathLen = validatePathLength(parentPath, newName)
    if (pathLen.severity === 'error') return pathLen

    const extCheck = validateExtensionChange(originalName, newName, allowExtensionChanges)
    if (extCheck.severity === 'error') return extCheck

    // Warning checks
    const conflict = validateConflict(newName, siblingNames, originalName)
    if (conflict.severity === 'warning') return conflict

    return OK_RESULT
}
