import { foldedForCompare, isLocalPath, lastName, parentOrRoot, type CaseRule } from './grammar'

declare const __canonicalBrand: unique symbol

/**
 * An absolute path safe for `dirname` / `basename` arithmetic.
 *
 * Includes local app paths (`/…`; on Windows `C:/…` and `//server/share/…`, the grammar
 * in `./grammar.ts`) and virtual-volume URLs (`mtp://...`, `adb://...`, `smb://...`,
 * `search-results://...`). Excludes `~`-rooted paths, relative paths, and anything that
 * would silently break `lastIndexOf('/')` style derivations.
 *
 * Construct only via `toCanonical`. Never cast.
 */
export type CanonicalPath = string & { readonly [__canonicalBrand]: true }

const VIRTUAL_SCHEME = /^[a-z][a-z0-9+.-]*:\/\//i

/**
 * Convert a UI-level path (possibly `~`-rooted) into a `CanonicalPath`.
 *
 * `homeDir` must be a non-empty absolute path with no trailing slash (matches
 * the contract in `FilePane.svelte`'s `userHomePath`). Throws if the input
 * can't be canonicalized.
 */
export function toCanonical(raw: string, homeDir: string): CanonicalPath {
  if (raw === '~') {
    if (!homeDir) throw new Error('toCanonical: homeDir is empty, cannot expand ~')
    return homeDir as CanonicalPath
  }
  if (raw.startsWith('~/')) {
    if (!homeDir) throw new Error('toCanonical: homeDir is empty, cannot expand ~/')
    return (homeDir + raw.slice(1)) as CanonicalPath
  }
  if (VIRTUAL_SCHEME.test(raw) || isLocalPath(raw)) return raw as CanonicalPath
  throw new Error(`toCanonical: not absolute or ~-rooted: ${JSON.stringify(raw)}`)
}

/**
 * True when `raw` names a file an OS API can open by path on its own: a local
 * app path (`/…`, or on Windows `C:/…` and `//server/share/…`), with no
 * virtual-volume scheme in front of it.
 *
 * The question `CanonicalPath` deliberately does NOT answer. The brand admits
 * virtual-volume URLs because slash arithmetic is safe on them, but `mtp://…`,
 * `adb://…`, `smb://…`, and `search-results://…` resolve only inside Cmdr.
 * Ask this wherever a path LEAVES the app for something that expects a real
 * file: `NSURL::fileURLWithPath` reads an unknown scheme as a RELATIVE path and
 * hands back a file URL under the process working directory, silently, so a
 * caller that skips the check ships a mangled path instead of a refusal.
 *
 * `~`-rooted and relative paths answer false too: neither resolves without a
 * base the OS API doesn't have. Pure, no I/O, and takes a plain `string` on
 * purpose, since the callers guarding an outbound path hold raw paths.
 */
export function isPlainFilesystemPath(raw: string): boolean {
  return isLocalPath(raw)
}

/**
 * True when `path` equals `volumePath` or sits under it.
 *
 * Matches whole path COMPONENTS, never a raw string prefix: `/Volumes/naspi`
 * must not claim `/Volumes/naspi-backup`, and a remote volume rooted at
 * `/srv/data` must not claim a sibling `/srv/data-1`. The Rust side draws the
 * same line for the same reason (`cmdr-fs`'s `RemoteRoot::to_remote_path`).
 *
 * Tolerates a trailing slash on `volumePath`, which a remote volume rooted at
 * `/` really does carry: its root is `<prefix><server-side root>`, so an SFTP
 * volume at `/` spells itself `sftp://ada@nas.local:22/`.
 *
 * Case folds only when the volume says it doesn't tell `Ada` from `ada`
 * (`VolumeInfo.caseSensitive`); without a rule the match is exact.
 *
 * Takes plain `string`s on purpose: callers hold raw paths off IPC, persisted
 * tab state, and volume records, none of them branded.
 */
export function isPathOnVolume(path: string, volumePath: string, rule: CaseRule = { caseSensitive: true }): boolean {
  const [p, v] = rule.caseSensitive ? [path, volumePath] : [foldedForCompare(path), foldedForCompare(volumePath)]
  if (p === v) return true
  const prefix = v.endsWith('/') ? v : v + '/'
  return p.startsWith(prefix)
}

/**
 * Returns the parent of a canonical path. A root is its own parent: `parentOf('/') === '/'`,
 * and on Windows `parentOf('C:/') === 'C:/'` and `parentOf('//server/share/x') === '//server/share/'`.
 */
export function parentOf(p: CanonicalPath): CanonicalPath {
  return parentOrRoot(p) as CanonicalPath
}

/** Returns the final segment. `basenameOf('/') === ''`, and so for `C:/` and `//server/share/`. */
export function basenameOf(p: CanonicalPath): string {
  return lastName(p)
}
