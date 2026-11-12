/**
 * The app-path grammar: what a path the app holds (a pane, a tab, a favorite, an IPC
 * answer) looks like, and the arithmetic every shape of it supports.
 *
 * An app path is **a scheme URL, or a `/`-separated absolute path whose first component
 * is the root**. The roots, by the local backend's flavor:
 *
 * - `posix` (macOS, Linux): `/`.
 * - `windows`: a drive, `C:/` (the letter always uppercase), or a UNC share,
 *   `//server/share/` (server and share are ONE root component, never two). Each is its
 *   own volume. A bare `/x` is no app path on Windows, and neither is anything holding a
 *   `\`: backslashes exist only in the shown form ({@link shownPath}) and inside the
 *   Windows local backend.
 *
 * A scheme URL (`mtp://…`, `sftp://…`, `search-results://…`) names its own volume, and
 * where its root sits is that volume's business (`cmdr-fs`'s `remote_paths.rs`), so the
 * grammar only does slash arithmetic on it, exactly as before.
 *
 * Rust holds the same grammar (`cmdr_fs::volume::path_grammar`), and both suites run one
 * case table (`crates/cmdr-fs/src/volume/path_grammar_cases.json`). Pure, no I/O. Every
 * function takes the flavor explicitly or follows the platform the backend injected
 * ({@link hostPathFlavor}); case sensitivity is the VOLUME's (`caseSensitive` on its
 * record), ❌ never the OS's.
 */
import { getPlatform } from '#lib/platform/index.js'

/** Which roots the local backend has: `/` alone, or drives and UNC shares. */
export type PathFlavor = 'posix' | 'windows'

/** The shape of an app path's root. */
export type PathKind = 'posix' | 'drive' | 'unc' | 'scheme'

/** An app path taken apart. `root` and `names` are `null` on a scheme URL, whose root is its volume's. */
export interface ParsedPath {
  kind: PathKind
  /** `/`, `C:/`, or `//server/share/`: always ends in `/`. */
  root: string | null
  /** The names under the root, `.` and `..` resolved lexically and clamped at the root. */
  names: string[] | null
}

/** Whether two spellings name one file follows the volume (`VolumeInfo.caseSensitive`). */
export interface CaseRule {
  caseSensitive: boolean
}

const SCHEME = /^[a-z][a-z0-9+.-]*:\/\//i
const DRIVE = /^([A-Za-z]):(?=\/|$)/

/** The local backend's flavor, from the platform the backend injected. */
export function hostPathFlavor(): PathFlavor {
  return getPlatform() === 'windows' ? 'windows' : 'posix'
}

/** Resolves `.`, `..`, and empty names lexically; `..` never climbs above the root. */
function resolveNames(rest: string): string[] {
  const names: string[] = []
  for (const name of rest.split('/')) {
    if (name === '' || name === '.') continue
    if (name === '..') names.pop()
    else names.push(name)
  }
  return names
}

/** `path`'s root and the rest after it, or `null` when `path` is no local app path in `flavor`. */
function splitRoot(path: string, flavor: PathFlavor): { kind: PathKind; root: string; rest: string } | null {
  if (flavor === 'posix') return path.startsWith('/') ? { kind: 'posix', root: '/', rest: path.slice(1) } : null
  if (path.includes('\\')) return null
  const drive = DRIVE.exec(path)
  if (drive) return { kind: 'drive', root: `${drive[1].toUpperCase()}:/`, rest: path.slice(2) }
  if (!path.startsWith('//')) return null
  const [server, share, ...rest] = path.slice(2).split('/')
  // `//?/` and `//./` are Win32's device namespaces, not shares.
  if (!server || !share || server === '?' || server === '.') return null
  return { kind: 'unc', root: `//${server}/${share}/`, rest: rest.join('/') }
}

/** `path` taken apart, or `null` when it's no app path: relative, `~`-rooted, empty, or malformed. */
export function parsePath(path: string, flavor: PathFlavor = hostPathFlavor()): ParsedPath | null {
  if (SCHEME.test(path)) return { kind: 'scheme', root: null, names: null }
  const split = splitRoot(path, flavor)
  if (split === null) return null
  return { kind: split.kind, root: split.root, names: resolveNames(split.rest) }
}

/** Whether `path` is a local app path (`/…`, `C:/…`, `//server/share/…`), ❌ not a scheme URL. */
export function isLocalPath(path: string, flavor: PathFlavor = hostPathFlavor()): boolean {
  const kind = parsePath(path, flavor)?.kind
  return kind !== undefined && kind !== 'scheme'
}

/** `path`'s root (`/`, `C:/`, `//server/share/`), or `null` for a scheme URL or no app path. */
export function rootOf(path: string, flavor: PathFlavor = hostPathFlavor()): string | null {
  return parsePath(path, flavor)?.root ?? null
}

/**
 * The one spelling of a local app path: drive letter uppercased, `.` and `..` resolved,
 * no doubled or trailing `/` except the root's own. A scheme URL comes back as is;
 * `null` for no app path.
 */
export function normalizePath(path: string, flavor: PathFlavor = hostPathFlavor()): string | null {
  const parsed = parsePath(path, flavor)
  if (parsed === null) return null
  if (parsed.root === null || parsed.names === null) return path
  return parsed.root + parsed.names.join('/')
}

/** Whether `path` is its volume's root. A scheme URL answers `false`: only its volume knows. */
export function isVolumeRoot(path: string, flavor: PathFlavor = hostPathFlavor()): boolean {
  const parsed = parsePath(path, flavor)
  return parsed?.names?.length === 0
}

/**
 * The folder holding `path`, normalized, or `null` at a root: there is no parent INSIDE
 * the volume, and what "up" from `C:/` shows is the pane's question, not the grammar's.
 * A scheme URL is plain slash arithmetic.
 */
export function parentInVolume(path: string, flavor: PathFlavor = hostPathFlavor()): string | null {
  const parsed = parsePath(path, flavor)
  if (parsed === null) return null
  if (parsed.root === null || parsed.names === null) return path.slice(0, Math.max(path.lastIndexOf('/'), 0))
  if (parsed.names.length === 0) return null
  return parsed.root + parsed.names.slice(0, -1).join('/')
}

/** The last name in `path` (`''` at a root), or `null` for no app path. */
export function basenameInVolume(path: string, flavor: PathFlavor = hostPathFlavor()): string | null {
  const parsed = parsePath(path, flavor)
  if (parsed === null) return null
  if (parsed.names === null) return path.slice(path.lastIndexOf('/') + 1)
  return parsed.names.at(-1) ?? ''
}

/**
 * A drive or share root as the prefix a volume-relative `/name` joins onto: `C:/` → `C:`,
 * `//server/share/` → `//server/share`. Every other path, `/` included, comes back as is,
 * so a caller's own `/` special case keeps working.
 */
export function volumePrefix(volumePath: string, flavor: PathFlavor = hostPathFlavor()): string {
  const kind = parsePath(volumePath, flavor)?.kind
  const isDriveOrShareRoot = (kind === 'drive' || kind === 'unc') && isVolumeRoot(volumePath, flavor)
  return isDriveOrShareRoot ? (normalizePath(volumePath, flavor) ?? volumePath).slice(0, -1) : volumePath
}

/**
 * The folder holding `path`, a root being its own (`/` → `/`, `C:/` → `C:/`): what a caller
 * holding a plain string asks where `parentOf` needs the `CanonicalPath` brand. A drive or
 * share path goes through the grammar; everything else is the slash arithmetic it always
 * was, byte for byte (`/x` → `/`, and a text with no `/` → `/`).
 */
export function parentOrRoot(path: string, flavor: PathFlavor = hostPathFlavor()): string {
  const parsed = parsePath(path, flavor)
  if ((parsed?.kind === 'drive' || parsed?.kind === 'unc') && parsed.root !== null) {
    return parentInVolume(path, flavor) ?? parsed.root
  }
  const i = path.lastIndexOf('/')
  return i <= 0 ? '/' : path.slice(0, i)
}

/**
 * The last name in `path` (`''` at a root), for a caller holding a plain string. A drive
 * or share path goes through the grammar; everything else is what follows its last `/`,
 * byte for byte, so a bare name (no `/`) comes back whole.
 */
export function lastName(path: string, flavor: PathFlavor = hostPathFlavor()): string {
  const kind = parsePath(path, flavor)?.kind
  if (kind === 'drive' || kind === 'unc') return basenameInVolume(path, flavor) ?? ''
  return path.slice(path.lastIndexOf('/') + 1)
}

/** `name` (one name, or several joined by `/`) inside the folder `base`. A root already ends in `/`. */
export function joinPath(base: string, name: string): string {
  return base.endsWith('/') ? base + name : `${base}/${name}`
}

/** Case and Unicode form folded, for a volume that doesn't tell `Ada` from `ada`. Compare only, ❌ never show. */
export function foldedForCompare(text: string): string {
  return text.normalize('NFC').toLowerCase()
}

/**
 * The part of `path` below `ancestor`, without leading or trailing `/` (`''` when they're
 * one), or `null` when `path` isn't `ancestor` or inside it. Whole components, ❌ never a
 * raw prefix: `/Users/ada` doesn't hold `/Users/ada-1`. Case folds only on a volume that
 * says so. The Rust twin is `cmdr_fs::volume::app_paths::path_under`.
 */
export function pathRelativeTo(
  path: string,
  ancestor: string,
  rule: CaseRule = { caseSensitive: true },
): string | null {
  // Folding compares the head at the ancestor's length, so both sides share one Unicode form first.
  const [p, a] = rule.caseSensitive ? [path, ancestor] : [path.normalize('NFC'), ancestor.normalize('NFC')]
  const base = a.endsWith('/') ? a.slice(0, -1) : a
  const head = p.slice(0, base.length)
  if (!(rule.caseSensitive ? head === base : foldedForCompare(head) === foldedForCompare(base))) return null
  const rest = p.slice(base.length)
  if (rest === '' || rest === '/') return ''
  if (!rest.startsWith('/')) return null
  return rest.replace(/^\/+|\/+$/g, '')
}

/** Whether two app paths name one file on a volume with this case rule. Spelling, never a `stat`. */
export function samePath(a: string, b: string, rule: CaseRule): boolean {
  return rule.caseSensitive ? a === b : foldedForCompare(a) === foldedForCompare(b)
}

/** The separator a person reads in `path`: `\` for a Windows local path, `/` for everything else. */
export function shownSeparator(path: string, flavor: PathFlavor = hostPathFlavor()): '/' | '\\' {
  // No parse off Windows: `displayPath` runs this over every formatted message.
  if (flavor === 'posix') return '/'
  const kind = parsePath(path, flavor)?.kind
  return kind === 'drive' || kind === 'unc' ? '\\' : '/'
}

/**
 * The ONE display formatter for a single path: a Windows local path reads with `\`
 * (`C:\Users\ada`, `\\server\share\x`, a share's root without the trailing `\`); every
 * other text, POSIX paths and scheme URLs included, comes back as is. Text that merely
 * HOLDS a path goes through `#lib/utils/display-path`'s `displayPath`, which calls this.
 */
export function shownPath(path: string, flavor: PathFlavor = hostPathFlavor()): string {
  if (shownSeparator(path, flavor) === '/') return path
  const parsed = parsePath(path, flavor)
  if (parsed?.root == null || parsed.names === null) return path
  const root = parsed.kind === 'unc' && parsed.names.length === 0 ? parsed.root.slice(0, -1) : parsed.root
  return (root + parsed.names.join('/')).replaceAll('/', '\\')
}

/**
 * What typed or pasted text becomes on the wire. On Windows both `\` and `/` separate,
 * and a local path comes out normalized (`c:\Users\` → `C:/Users`); text that isn't one
 * (relative, `~`) only has its separators turned, and the backend resolves it. On macOS
 * and Linux the text is untouched: a `\` is a legal filename character there.
 */
export function fromUserInput(text: string, flavor: PathFlavor = hostPathFlavor()): string {
  if (flavor === 'posix' || SCHEME.test(text)) return text
  const slashed = text.replaceAll('\\', '/')
  return normalizePath(slashed, flavor) ?? slashed
}
