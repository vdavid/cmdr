declare const __canonicalBrand: unique symbol

/**
 * An absolute path safe for `dirname` / `basename` arithmetic.
 *
 * Includes local filesystem paths (start with `/`) and virtual-volume URLs
 * (`mtp://...`, `smb://...`, `search-results://...`). Excludes `~`-rooted
 * paths, relative paths, and anything that would silently break `lastIndexOf('/')`
 * style derivations.
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
  if (raw.startsWith('/') || VIRTUAL_SCHEME.test(raw)) return raw as CanonicalPath
  throw new Error(`toCanonical: not absolute or ~-rooted: ${JSON.stringify(raw)}`)
}

/** Returns the parent of a canonical path. `parentOf('/') === '/'`. */
export function parentOf(p: CanonicalPath): CanonicalPath {
  const i = p.lastIndexOf('/')
  return (i <= 0 ? '/' : p.slice(0, i)) as CanonicalPath
}

/** Returns the final segment. `basenameOf('/') === ''`. */
export function basenameOf(p: CanonicalPath): string {
  return p.slice(p.lastIndexOf('/') + 1)
}
