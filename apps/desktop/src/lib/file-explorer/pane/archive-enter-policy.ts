/**
 * Enter-behavior policy for archives and macOS bundles — the single, pure source
 * of truth for "what happens when the user presses Enter on a `.zip` / a `.app` /
 * an OOXML document".
 *
 * Three outcomes: `browse` (step inside like a folder), `open` (hand the file to
 * its external app via LaunchServices), or `ask` (show the Browse | Open popup so
 * the user picks per-Enter). Each format carries its own registry setting
 * (`behavior.archiveEnter.<format>`), rendered in Settings › Behavior › Archives.
 *
 * This module is a pure leaf (no I/O, no store, no Svelte): the caller reads the live
 * settings and passes the actions in, so the decision is unit-testable in isolation.
 * Classification is extension-only, mirroring the backend's
 * `has_supported_archive_extension` (the cheap check the listing already ran);
 * the backend magic-byte confirms the real archive at navigation time.
 */

/** What pressing Enter does on an archive/bundle/document entry. */
export type EnterAction = 'browse' | 'open' | 'ask'

/** The stable format keys, used both as settings keys and as resolver categories. */
export type ArchiveFormatKey = 'zip' | 'ooxml' | 'bundle'

/**
 * The registry setting that holds one format's action. Spelled as a template type so
 * a descriptor can't name an id that doesn't follow the scheme; that the id also
 * EXISTS in the settings registry (with a matching default) is what
 * `archive-enter-policy.test.ts`'s two-way parity test proves.
 */
export type ArchiveEnterSettingId = `behavior.archiveEnter.${ArchiveFormatKey}`

/**
 * What Enter does, per format. Partial: a format the caller didn't supply falls back
 * to its `defaultAction`, which is also the registry default for its setting, so both
 * paths land on the same answer.
 */
export type EnterBehaviorByFormat = Partial<Record<ArchiveFormatKey, EnterAction>>

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
  /**
   * The registry setting holding this format's action. Every format has one, so
   * "is this format configurable?" is answered by the format list itself rather
   * than by a flag that could disagree with it.
   */
  settingId: ArchiveEnterSettingId
  /** True when an entry belongs to this format. */
  matches: (entry: EnterCandidate) => boolean
  /** The action when the user hasn't chosen one. Must equal the setting's registry default. */
  defaultAction: EnterAction
}

/** Zip-based document and app packages users mean as documents, not folders. */
const OOXML_EXTENSIONS: readonly string[] = ['docx', 'xlsx', 'pptx', 'jar', 'apk']
/** macOS bundle directory extensions (a folder macOS presents as one item). */
const BUNDLE_EXTENSIONS: readonly string[] = ['app', 'bundle', 'framework']

/**
 * The format registry, ordered by MATCH SPECIFICITY: `classify` is first-match-wins,
 * so a narrow format must sit above every broader one it would otherwise fall into.
 * (Display order is the section's own business; it hand-renders each row.)
 *
 * - `ooxml`: Office and Java/Android packages — zip under the hood, but a document
 *   or an app, so Open by default. FIRST because it's a strict subset of `zip`:
 *   these files are real zips, and the moment the backend flags one `isArchive`
 *   the broader zip matcher would swallow it. `archive-enter-policy.test.ts` pins it.
 * - `zip`: true archives Cmdr can browse into, keyed off the backend's `isArchive`
 *   flag (its single source of truth — extension-only, never a directory) so the
 *   two stay in lockstep and future formats (tar/7z) join automatically. Default
 *   Ask (browse or open is a genuine per-file choice).
 * - `bundle`: macOS application/framework bundles. Directories, so browsing already
 *   works; Open launches them via LaunchServices. Default Ask.
 */
export const ARCHIVE_ENTER_FORMATS: readonly FormatDescriptor[] = [
  {
    key: 'ooxml',
    settingId: 'behavior.archiveEnter.ooxml',
    matches: (entry) => !entry.isDirectory && hasExtensionIn(entry.name, OOXML_EXTENSIONS),
    defaultAction: 'open',
  },
  {
    key: 'zip',
    settingId: 'behavior.archiveEnter.zip',
    matches: (entry) => entry.isArchive === true,
    defaultAction: 'ask',
  },
  {
    key: 'bundle',
    settingId: 'behavior.archiveEnter.bundle',
    matches: (entry) => entry.isDirectory && hasExtensionIn(entry.name, BUNDLE_EXTENSIONS),
    defaultAction: 'ask',
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
 * The Enter action for an entry given the per-format actions, or `null` when the entry
 * is neither an archive, a document package, nor a bundle (the caller then does its
 * ordinary open/browse).
 */
export function resolveEnterPolicy(entry: EnterCandidate, behavior: EnterBehaviorByFormat): EnterAction | null {
  const format = classify(entry)
  if (!format) return null
  return behavior[format.key] ?? format.defaultAction
}

/** Whether `value` is one of the three actions the resolver can return. */
export function isEnterAction(value: unknown): value is EnterAction {
  return typeof value === 'string' && (ENTER_ACTIONS as readonly string[]).includes(value)
}

/**
 * The per-format actions, read through `read` (in practice `getSetting`, which is
 * why this takes the reader rather than importing it: the module stays a pure leaf
 * and the map is trivial to fake in a test).
 *
 * Reads every format, so a newly added one is picked up here for free.
 */
export function enterBehaviorFromSettings(read: (id: ArchiveEnterSettingId) => EnterAction): EnterBehaviorByFormat {
  const behavior: EnterBehaviorByFormat = {}
  for (const format of ARCHIVE_ENTER_FORMATS) {
    behavior[format.key] = read(format.settingId)
  }
  return behavior
}
