/**
 * Enter-behavior policy for archives and macOS bundles — the single, pure source
 * of truth for "what happens when the user presses Enter on a `.zip` / a `.app` /
 * an OOXML document".
 *
 * Three outcomes: `browse` (step inside like a folder), `open` (hand the file to
 * its external app via LaunchServices), or `ask` (show the Browse | Open popup so
 * the user picks per-Enter). A format's default is overridable per-format in
 * Settings › Behavior › Archives; the stored overrides are a pinned-shape JSON
 * object (`{ zip: 'ask', bundle: 'open', … }`) parsed by `parseEnterBehaviorOverrides`.
 *
 * This module is a pure leaf (no I/O, no store, no Svelte): FilePane reads the
 * live setting and passes the parsed overrides in, so the decision is unit-testable
 * in isolation. Classification is extension-only, mirroring the backend's
 * `has_supported_archive_extension` (the cheap check the listing already ran);
 * the backend magic-byte confirms the real archive at navigation time.
 */

/** What pressing Enter does on an archive/bundle/document entry. */
export type EnterAction = 'browse' | 'open' | 'ask'

/** The stable format keys, used both as settings keys and as resolver categories. */
export type ArchiveFormatKey = 'zip' | 'ooxml' | 'bundle'

/** The stored per-format override map (a subset — unset formats use their default). */
export type EnterBehaviorOverrides = Partial<Record<ArchiveFormatKey, EnterAction>>

/** The entry fields the resolver reads (a subset of `FileEntry`). */
export interface EnterCandidate {
  name: string
  isDirectory: boolean
  /**
   * Backend-computed, extension-only, never true for a directory. Optional to
   * match `FileEntry` (synthetic rows omit it); absent reads as not-an-archive.
   */
  isArchive?: boolean
}

interface FormatDescriptor {
  key: ArchiveFormatKey
  /** True when an entry belongs to this format. */
  matches: (entry: EnterCandidate) => boolean
  /** The action when the user hasn't overridden this format. */
  defaultAction: EnterAction
  /** Whether this format is exposed as a Settings row (with a Browse option). */
  configurable: boolean
}

/** Zip-based document and app packages users mean as documents, not folders. */
const OOXML_EXTENSIONS: readonly string[] = ['docx', 'xlsx', 'pptx', 'jar', 'apk']
/** macOS bundle directory extensions (a folder macOS presents as one item). */
const BUNDLE_EXTENSIONS: readonly string[] = ['app', 'bundle', 'framework']

/**
 * The format registry. Order is the Settings-list order.
 *
 * - `zip`: true archives Cmdr can browse into, keyed off the backend's `isArchive`
 *   flag (its single source of truth — extension-only, never a directory) so the
 *   two stay in lockstep and future formats (tar/7z) join automatically. Default
 *   Ask (browse or open is a genuine per-file choice).
 * - `ooxml`: Office and Java/Android packages — zip under the hood, but opened, not
 *   browsed. Default Open, and NOT configurable yet — browsing into them isn't
 *   supported this phase, so a Browse option would be dead. Encoded so the default
 *   is explicit and testable (it resolves to the same Open as any other document).
 * - `bundle`: macOS application/framework bundles. Directories, so browsing already
 *   works; Open launches them via LaunchServices. Default Ask.
 */
export const ARCHIVE_ENTER_FORMATS: readonly FormatDescriptor[] = [
  {
    key: 'zip',
    matches: (entry) => entry.isArchive === true,
    defaultAction: 'ask',
    configurable: true,
  },
  {
    key: 'ooxml',
    matches: (entry) => !entry.isDirectory && hasExtensionIn(entry.name, OOXML_EXTENSIONS),
    defaultAction: 'open',
    configurable: false,
  },
  {
    key: 'bundle',
    matches: (entry) => entry.isDirectory && hasExtensionIn(entry.name, BUNDLE_EXTENSIONS),
    defaultAction: 'ask',
    configurable: true,
  },
]

const ENTER_ACTIONS: readonly EnterAction[] = ['browse', 'open', 'ask']

/** The lowercased final extension of `name`, or `undefined` when it has no stem. */
function extensionOf(name: string): string | undefined {
  const dot = name.lastIndexOf('.')
  // `dot <= 0` covers "no dot" and a leading-dot dotfile (`.zip`) with no stem,
  // matching the backend's `Path::extension()` returning `None`.
  if (dot <= 0) return undefined
  return name.slice(dot + 1).toLowerCase()
}

function hasExtensionIn(name: string, extensions: readonly string[]): boolean {
  const ext = extensionOf(name)
  return ext !== undefined && extensions.includes(ext)
}

/** The format an entry belongs to, or `null` when it's an ordinary file/folder. */
function classify(entry: EnterCandidate): FormatDescriptor | null {
  return ARCHIVE_ENTER_FORMATS.find((format) => format.matches(entry)) ?? null
}

/**
 * The Enter action for an entry given the user's per-format overrides, or `null`
 * when the entry is neither an archive, a document package, nor a bundle (the
 * caller then does its ordinary open/browse).
 */
export function resolveEnterPolicy(entry: EnterCandidate, overrides: EnterBehaviorOverrides): EnterAction | null {
  const format = classify(entry)
  if (!format) return null
  return overrides[format.key] ?? format.defaultAction
}

function isEnterAction(value: unknown): value is EnterAction {
  return typeof value === 'string' && (ENTER_ACTIONS as readonly string[]).includes(value)
}

function isFormatKey(value: string): value is ArchiveFormatKey {
  return ARCHIVE_ENTER_FORMATS.some((f) => f.key === value)
}

/**
 * Parse the stored JSON overrides into a clean map, keeping only known format
 * keys with valid actions. Malformed, empty, or non-object input yields `{}` (all
 * formats fall to their defaults) — the setting can never wedge the Enter key.
 */
export function parseEnterBehaviorOverrides(stored: string): EnterBehaviorOverrides {
  let raw: unknown
  try {
    raw = JSON.parse(stored)
  } catch {
    return {}
  }
  if (raw === null || typeof raw !== 'object' || Array.isArray(raw)) return {}
  const result: EnterBehaviorOverrides = {}
  for (const [key, value] of Object.entries(raw)) {
    if (isFormatKey(key) && isEnterAction(value)) result[key] = value
  }
  return result
}
