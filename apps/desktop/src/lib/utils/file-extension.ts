/**
 * What counts as a name's extension: the frontend mirror of the product's ONE
 * rule, `crates/cmdr-fs/src/file_extension.rs`. The rule, why it's mirrored
 * rather than asked over IPC, and who uses it: `crates/cmdr-fs/DETAILS.md` §
 * "What counts as an extension". Everything on the frontend that splits a name
 * at its extension goes through here. `file-extension.test.ts` runs the
 * backend's own cases (`file_extension_cases.json`) against it.
 *
 * In short: a plain folder has none; a macOS package's (`Cmdr.app`) is what
 * follows its last dot; a file's is 1–10 ASCII letters and digits after the
 * last dot, or a known compound (`tar.gz`); a leading or trailing dot isn't
 * one. Which of the three a name is comes from its entry (`nameKindOf`): the
 * backend decides packages (`FileEntry.isPackage`, the list plus macOS). The
 * package list here serves only names with no entry yet, like a folder's new
 * name typed into the rename field (`kindOfRenamedName`).
 */

import { hasCapability } from '#lib/platform/index.js'

/** What a name belongs to, which decides what its extension can be. Mirrors Rust's `NameKind`. */
export type NameKind = 'file' | 'folder' | 'package'

/**
 * Rust's `PACKAGE_EXTENSIONS`, for names no entry describes yet. Pinned to it by
 * `file_extension_cases.json`, which both languages' tests compare their list
 * with. Lowercase, no dot.
 */
export const PACKAGE_EXTENSIONS: readonly string[] = [
  'app',
  'appex',
  'bundle',
  'component',
  'framework',
  'kext',
  'mdimporter',
  'photoslibrary',
  'pkg',
  'plugin',
  'prefpane',
  'qlgenerator',
  'rtfd',
  'saver',
  'wdgt',
  'xcodeproj',
  'xcworkspace',
]

/**
 * The kind of an entry, from what the backend reported about it. A listed row
 * always says (`isPackage`, macOS included); one built from a path alone (an
 * MCP rename's commit target) falls back to the package list.
 */
export function nameKindOf(entry: { name: string; isDirectory: boolean; isPackage?: boolean }): NameKind {
  if (entry.isPackage === undefined) return nameKindByName(entry.name, entry.isDirectory)
  if (!entry.isDirectory) return 'file'
  return entry.isPackage ? 'package' : 'folder'
}

/**
 * The kind of `newName`, the name an entry of `kind` is being renamed to. A
 * file stays a file and a package stays a package (its new extension is still
 * what follows the last dot), but a plain folder renamed to a listed package
 * suffix (`Plugins` → `Plugins.bundle`) becomes one, which is worth asking
 * about: Finder will show it as one opaque item.
 */
export function kindOfRenamedName(kind: NameKind, newName: string): NameKind {
  return kind === 'folder' ? nameKindByName(newName, true) : kind
}

/**
 * The kind read off the name alone, through `PACKAGE_EXTENSIONS`, for a name no
 * entry describes. Mirrors Rust's `NameKind::by_name`. Prefer `nameKindOf`
 * whenever there's an entry: only the backend knows the packages macOS reports.
 * Off macOS (no `appBundles`) no name makes a package.
 */
export function nameKindByName(name: string, isDirectory: boolean): NameKind {
  if (!isDirectory) return 'file'
  // Only macOS reads a suffixed folder name as a package (Rust's `folder_names_make_packages`).
  if (!hasCapability('appBundles')) return 'folder'
  return PACKAGE_EXTENSIONS.includes(lastSegment(name).toLowerCase()) ? 'package' : 'folder'
}

/** Two-part extensions that count as one: the compressed tars. Lowercase, no leading dot. */
export const COMPOUND_EXTENSIONS: readonly string[] = ['tar.gz', 'tar.bz2', 'tar.xz', 'tar.zst']

const PLAIN_EXTENSION = /^[A-Za-z0-9]{1,10}$/

/** The part after the last dot, when there's a stem before it. */
function lastSegment(name: string): string {
  const dot = name.lastIndexOf('.')
  return dot > 0 ? name.slice(dot + 1) : ''
}

function compoundExtension(name: string): string {
  const lower = name.toLowerCase()
  for (const compound of COMPOUND_EXTENSIONS) {
    // A real stem before the dot, so `.tar.gz` alone is a dotfile's `gz`.
    if (lower.length > compound.length + 1 && lower.endsWith(`.${compound}`)) {
      return name.slice(name.length - compound.length)
    }
  }
  return ''
}

/**
 * The extension of `name`, without its dot and in the name's own case, or `''`
 * when it has none under the product's rule.
 */
export function fileExtension(name: string, kind: NameKind): string {
  if (kind === 'folder') return ''
  const last = lastSegment(name)
  if (kind === 'package') return last
  const compound = compoundExtension(name)
  if (compound) return compound
  return PLAIN_EXTENSION.test(last) ? last : ''
}

/** `name` with its extension and the dot before it stripped, or the whole name when it has none. */
export function nameWithoutExtension(name: string, kind: NameKind): string {
  const ext = fileExtension(name, kind)
  return ext ? name.slice(0, name.length - ext.length - 1) : name
}

/** `name` with its extension replaced by `extension` (no dot; `''` removes it), or added when it had none. */
export function withExtension(name: string, extension: string, kind: NameKind): string {
  const stem = nameWithoutExtension(name, kind)
  return extension ? `${stem}.${extension}` : stem
}
