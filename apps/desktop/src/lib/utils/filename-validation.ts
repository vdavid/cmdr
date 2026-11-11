/** Client-side filename validation for instant keystroke feedback. */

import { tString } from '#lib/intl/messages.svelte.js'
import { fromUserInput, isLocalPath } from '#lib/path/grammar.js'
import { fileExtension, kindOfRenamedName, type NameKind } from './file-extension'
import { joinPath } from '#lib/path/grammar.js'

/** Defaults match macOS. Call initPathLimits() at startup to get platform-specific values from the backend. */
let maxNameBytes = 255
let maxPathBytes = 1024

/** Fetches platform-specific limits from the backend. Call once at app startup. */
export async function initPathLimits(): Promise<void> {
  const { getPathLimits } = await import('#lib/tauri-commands/index.js')
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

/** ICU `select` discriminator for the file-vs-folder message variants. */
function nameKind(isDir: boolean): 'file' | 'folder' {
  return isDir ? 'folder' : 'file'
}

/** Validates a file or dir name for disallowed characters. Operates on trimmed value. */
export function validateDisallowedChars(name: string, isDir = false): ValidationResult {
  if (DISALLOWED_CHARS_REGEX.test(name)) {
    return {
      severity: 'error',
      message: tString('fileOperations.validation.disallowedChars', { kind: nameKind(isDir) }),
    }
  }
  return OK_RESULT
}

/** Validates that a file or dir name is not empty after trimming. */
export function validateNotEmpty(name: string, isDir = false): ValidationResult {
  if (name.trim().length === 0) {
    return { severity: 'error', message: tString('fileOperations.validation.empty', { kind: nameKind(isDir) }) }
  }
  return OK_RESULT
}

/** Validates name byte length (max 255 bytes). */
export function validateNameLength(name: string, isDir = false): ValidationResult {
  const byteLength = new TextEncoder().encode(name.trim()).length
  if (byteLength >= maxNameBytes) {
    return {
      severity: 'error',
      message: tString('fileOperations.validation.nameTooLong', {
        kind: nameKind(isDir),
        byteCount: String(byteLength),
        maxBytes: String(maxNameBytes),
        max: maxNameBytes,
      }),
    }
  }
  return OK_RESULT
}

/** Validates full path byte length (max 1024 bytes). */
export function validatePathLength(parentPath: string, name: string): ValidationResult {
  const fullPath = joinPath(parentPath, name.trim())
  const byteLength = new TextEncoder().encode(fullPath).length
  if (byteLength >= maxPathBytes) {
    return {
      severity: 'error',
      message: tString('fileOperations.validation.pathTooLong', {
        byteCount: String(byteLength),
        maxBytes: String(maxPathBytes),
        max: maxPathBytes,
      }),
    }
  }
  return OK_RESULT
}

/**
 * Extensions treated as equivalent: switching between any two in a group skips the rename warning.
 * Each group lists spellings of the same underlying format, lowercase, no leading dot.
 *
 * The `md`/`markdown`/`txt` group is included by deliberate choice: Markdown is plain text and
 * the routing tradeoff (different "Open with" defaults) is accepted as a feature, not a bug.
 */
const EQUIVALENT_EXTENSION_GROUPS: readonly (readonly string[])[] = [
  ['jpg', 'jpeg', 'jpe', 'jfif'],
  ['tif', 'tiff'],
  ['htm', 'html'],
  ['yml', 'yaml'],
  ['mpg', 'mpeg'],
  ['mid', 'midi'],
  ['aif', 'aiff'],
  ['qt', 'mov'],
  ['md', 'markdown', 'txt'],
]

/** Precomputed `ext -> groupIndex` map for O(1) equivalence lookup. */
const EXTENSION_TO_GROUP: ReadonlyMap<string, number> = new Map(
  EQUIVALENT_EXTENSION_GROUPS.flatMap((group, i) => group.map((ext) => [ext, i] as const)),
)

function normalizedExt(filename: string, kind: NameKind): string {
  return fileExtension(filename, kind).toLowerCase()
}

/**
 * True if the extension change is meaningful enough to warrant a confirmation.
 * Returns false for case-only changes (`photo.JPG` → `photo.jpg`) and for changes
 * between known equivalents (`photo.jpeg` → `photo.jpg`, `notes.md` → `notes.txt`),
 * so users aren't pestered to confirm what's effectively a metadata tweak.
 */
export function extensionsDifferMeaningfully(oldName: string, newName: string, kind: NameKind = 'file'): boolean {
  const trimmed = newName.trim()
  const oldExt = normalizedExt(oldName, kind)
  const newExt = normalizedExt(trimmed, kindOfRenamedName(kind, trimmed))
  if (oldExt === newExt) return false
  const oldGroup = EXTENSION_TO_GROUP.get(oldExt)
  return oldGroup === undefined || oldGroup !== EXTENSION_TO_GROUP.get(newExt)
}

/**
 * True if renaming `oldName` to `newName` changes the extension enough to confirm.
 * What an extension is comes from `file-extension.ts`, and `kind` is the
 * entry's (`nameKindOf`): a folder has one only when the backend says it's a
 * macOS package, so `release.v1.2` → `release.v1.3` asks nothing while
 * `Cmdr.app` → `Cmdr.old` does, and so does a package the backend learned of
 * from macOS (`Sketch.myBundle`). The new name's kind is `kindOfRenamedName`'s,
 * so a folder turning into a listed package (`Plugins` → `Plugins.bundle`)
 * asks too.
 */
export function extensionChangeNeedsConfirmation(oldName: string, newName: string, kind: NameKind): boolean {
  return extensionsDifferMeaningfully(oldName, newName, kind)
}

/** Validates extension change against the user's preference. */
export function validateExtensionChange(
  oldName: string,
  newName: string,
  allowExtensionChanges: 'yes' | 'no' | 'ask',
  kind: NameKind = 'file',
): ValidationResult {
  if (allowExtensionChanges === 'yes') return OK_RESULT

  if (!extensionChangeNeedsConfirmation(oldName, newName, kind)) return OK_RESULT

  if (allowExtensionChanges === 'no') {
    const ext = fileExtension(oldName, kind)
    const oldExt = ext ? `.${ext}` : ''
    return {
      severity: 'error',
      message: tString('fileOperations.validation.extensionChangeBlocked', { oldExt }),
    }
  }
  // 'ask': no error, the dialog will handle it on save
  return OK_RESULT
}

/**
 * Checks if the new name matches an existing sibling, case-insensitively on every
 * volume: on a case-insensitive one (APFS, NTFS, SMB) that's a real clash, and on a
 * case-sensitive one a case-only twin is still worth a warning (it's not a refusal).
 * So this deliberately ignores the volume's `caseSensitive`.
 * Exception: case-only rename of the same file produces no warning.
 */
export function validateConflict(newName: string, siblingNames: string[], originalName: string): ValidationResult {
  const trimmed = newName.trim()
  const trimmedLower = trimmed.toLowerCase()
  const originalLower = originalName.toLowerCase()

  for (const sibling of siblingNames) {
    if (sibling.toLowerCase() === trimmedLower) {
      // Case-only rename of the same file: no warning
      if (sibling.toLowerCase() === originalLower) continue
      return { severity: 'warning', message: tString('fileOperations.validation.conflict', { name: trimmed }) }
    }
  }
  return OK_RESULT
}

/** Validates a full directory path (not a filename). Checks structure only, not existence. */
export function validateDirectoryPath(path: string): ValidationResult {
  const trimmed = path.trim()

  if (trimmed.length === 0) {
    return { severity: 'error', message: tString('fileOperations.validation.pathEmpty') }
  }

  // Accept absolute paths (`/…`, which is also how a volume-relative destination
  // reads on every volume; on Windows also `C:/…` or `C:\…` and a UNC share) and
  // the home shortcut (`~` or `~/…`). `~` is the app's internal stand-in for the
  // user's home dir, expanded to an absolute path by the backend. A bare `~foo` (no
  // slash) isn't a home shortcut, so it stays rejected as relative.
  const isAbsolute = trimmed.startsWith('/') || isLocalPath(fromUserInput(trimmed))
  if (!isAbsolute && trimmed !== '~' && !trimmed.startsWith('~/')) {
    return { severity: 'error', message: tString('fileOperations.validation.pathNotAbsolute') }
  }

  if (trimmed.includes('\0')) {
    return { severity: 'error', message: tString('fileOperations.validation.pathNullChar') }
  }

  const encoder = new TextEncoder()
  const totalBytes = encoder.encode(trimmed).length
  if (totalBytes >= maxPathBytes) {
    return {
      severity: 'error',
      message: tString('fileOperations.validation.pathTotalTooLong', {
        byteCount: String(totalBytes),
        maxBytes: String(maxPathBytes),
        max: maxPathBytes,
      }),
    }
  }

  const segments = trimmed.split('/').filter((s) => s.length > 0)
  for (const segment of segments) {
    const segmentBytes = encoder.encode(segment).length
    if (segmentBytes >= maxNameBytes) {
      return {
        severity: 'error',
        message: tString('fileOperations.validation.pathSegmentTooLong', {
          byteCount: String(segmentBytes),
          maxBytes: String(maxNameBytes),
          max: maxNameBytes,
        }),
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
  kind: NameKind = 'file',
): ValidationResult {
  const trimmed = newName.trim()
  const isDir = kind !== 'file'

  // Error checks
  const emptyCheck = validateNotEmpty(newName, isDir)
  if (emptyCheck.severity === 'error') return emptyCheck

  const charCheck = validateDisallowedChars(trimmed, isDir)
  if (charCheck.severity === 'error') return charCheck

  const nameLen = validateNameLength(newName, isDir)
  if (nameLen.severity === 'error') return nameLen

  const pathLen = validatePathLength(parentPath, newName)
  if (pathLen.severity === 'error') return pathLen

  const extCheck = validateExtensionChange(originalName, newName, allowExtensionChanges, kind)
  if (extCheck.severity === 'error') return extCheck

  // Warning checks
  const conflict = validateConflict(newName, siblingNames, originalName)
  if (conflict.severity === 'warning') return conflict

  return OK_RESULT
}
