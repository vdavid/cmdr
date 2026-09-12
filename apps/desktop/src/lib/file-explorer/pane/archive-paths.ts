/**
 * Archive-path vocabulary: which suffixes make a name an archive, which of those
 * are writable, and the two boundary checks (`pathCrossesArchiveBoundary` /
 * `pathInsideArchive`) every archive-aware site picks between. Pure, no I/O, no
 * store: a leaf `volume-capabilities.ts` and its consumers build on.
 */

/**
 * The supported archive-name SUFFIXES, MIRRORING the backend's `format_for_name`
 * (`crates/cmdr-fs/src/archive_format.rs`). Kept in lockstep — the FE does the
 * cheap suffix pre-filter, the backend stat- and magic-confirms on actual
 * navigation. Suffix-based (not just the last `.ext`) so `.tar.gz` matches while
 * a bare `.gz` doesn't. Longest-first so `.tar.gz` wins over `.tar`.
 */
export const SUPPORTED_ARCHIVE_SUFFIXES: readonly string[] = [
  '.tar.gz',
  '.tar.bz2',
  '.tar.xz',
  '.tar.zst',
  '.tgz',
  '.tbz2',
  '.tbz',
  '.txz',
  '.tzst',
  '.tar',
  '.zip',
  '.7z',
  // Zip containers that are a DOCUMENT or an app package rather than an archive
  // the user assembled. Browsable, never writable — see below.
  '.docx',
  '.xlsx',
  '.pptx',
  '.jar',
  '.apk',
]

/**
 * The WRITABLE archive suffixes: only zip. tar and 7z are browse + extract only,
 * so a pane inside one gets the read-only archive capability. Mirrors the backend
 * write chokepoint (`archive_edit::ensure_zip_writable`).
 *
 * A DOCUMENT container (`.docx`, `.jar`, …) is absent for a stronger reason than
 * tar and 7z are: those simply have no mutator, while a `.docx` IS a zip and the
 * mutator would happily rewrite one. Letting a user rename or delete parts while
 * wandering inside a Word file hands them a corrupt document, so the backend
 * refuses it by TYPE (`ArchiveFormat::Ooxml` never satisfies `ensure_zip_writable`)
 * and this list keeps the UI honest about it. ❌ Never add one here.
 */
export const WRITABLE_ARCHIVE_SUFFIXES: readonly string[] = ['.zip']

/** Whether `name` ends with `suffix` and has a real stem before it. */
function nameHasSuffix(name: string, suffix: string): boolean {
  const lower = name.toLowerCase()
  return lower.endsWith(suffix) && lower.length > suffix.length
}

/**
 * True if `name`'s suffix is a supported archive format (case-insensitive).
 * Mirrors the backend's `has_supported_archive_extension`.
 */
export function hasSupportedArchiveExtension(name: string): boolean {
  return SUPPORTED_ARCHIVE_SUFFIXES.some((s) => nameHasSuffix(name, s))
}

/** True if `name` is a WRITABLE archive (zip) — tar/7z return false. */
export function isWritableArchiveName(name: string): boolean {
  return WRITABLE_ARCHIVE_SUFFIXES.some((s) => nameHasSuffix(name, s))
}

/**
 * Whether `path` is AT or inside a supported archive — the WIDE half of the pair,
 * a pure extension-only string check (NO I/O) mirroring the backend's
 * `path_crosses_archive_boundary`: ANY path component (not just the last)
 * carrying a supported archive extension crosses. `/a/foo.zip` (the archive root)
 * and `/a/foo.zip/inner` both return true; `/a` (a plain folder that merely
 * CONTAINS `foo.zip`) does not.
 *
 * This is the ENTER-IT question, so it's what a site gating on the PANE's path
 * wants: a pane sitting at `/a/foo.zip` is showing the archive's contents, and
 * a git lookup, a disk-space query, or a write-capability row must treat it as
 * such. For a site that operates ON a path — preview it, move it, rename it —
 * reach for `pathInsideArchive` instead: the `.zip` file itself is an ordinary
 * file there.
 *
 * This is a lower bound the backend corrects: a real directory literally named
 * `foo.zip`, or a mislabeled non-archive file, is NOT decidable here (it needs a
 * stat + magic sniff). The FE uses it only for read-only capability gating, where
 * a false "read-only" is safe (the backend rejects a genuinely writable-target
 * mistake) and a missed one is caught by the backend `ReadOnlyDevice` net.
 */
export function pathCrossesArchiveBoundary(path: string): boolean {
  return path.split('/').some((segment) => hasSupportedArchiveExtension(segment))
}

/**
 * Whether `path` points at something strictly INSIDE a supported archive — the
 * NARROW half, mirroring the backend's `path_is_inside_archive`. True only when
 * the archive boundary is followed by a non-empty inner path:
 * `/a/foo.zip/inner` yes, `/a/foo.zip` (and `/a/foo.zip/`) no.
 *
 * The distinction is load-bearing, in the backend's own words: an archive-inner
 * path has no real file behind it, while the `.zip` file ITSELF is a regular file
 * that must be copied, moved, renamed, previewed, and Quick Looked exactly like
 * any other. Sites that operate ON a path use this one; sites that navigate INTO
 * a path use `pathCrossesArchiveBoundary`.
 *
 * Gets sharper the more zip-container formats Cmdr browses: with `.docx` a
 * supported suffix, the wide check here would refuse Quick Look on every Word
 * document, which is what this half exists to prevent.
 */
export function pathInsideArchive(path: string): boolean {
  const segments = path.split('/')
  const boundary = segments.findIndex((segment) => hasSupportedArchiveExtension(segment))
  if (boundary === -1) return false
  // A trailing slash leaves an empty segment, which is still the archive ROOT —
  // so ask for a non-empty inner component rather than just a longer array.
  return segments.slice(boundary + 1).some((segment) => segment.length > 0)
}

/**
 * The display name of the archive a path is at or inside: the FIRST path segment
 * carrying a supported archive extension (leftmost wins, matching the backend's
 * boundary resolution and `pathCrossesArchiveBoundary`), so `/a/photos.zip/inner/x.jpg`
 * returns `photos.zip`. Falls back to the path's basename when no segment is an
 * archive (a caller should only reach here for an in-archive path, but the
 * fallback keeps it total). Pure, no I/O.
 */
export function archiveNameFromPath(path: string): string {
  const segments = path.split('/').filter((s) => s.length > 0)
  const archiveSegment = segments.find((s) => hasSupportedArchiveExtension(s))
  if (archiveSegment) return archiveSegment
  return segments.length > 0 ? segments[segments.length - 1] : path
}

/**
 * The real folder on disk that CONTAINS the archive a path is at or inside: the
 * directory holding the FIRST archive-extension segment, so
 * `/a/b/photos.zip/inner/x.jpg` and `/a/b/photos.zip` both return `/a/b`. The
 * leftmost-wins rule matches `pathCrossesArchiveBoundary` and the backend's boundary
 * resolution, so a nested `foo.tar/bar.zip/…` resolves against the outer tar.
 *
 * Returns `'/'` when the archive sits at the filesystem root, and the path
 * unchanged when no segment is an archive (a caller should only reach here for an
 * in-archive path, but the fallback keeps it total). Pure, no I/O.
 */
export function folderContainingArchive(path: string): string {
  const segments = path.split('/')
  const boundary = segments.findIndex((segment) => hasSupportedArchiveExtension(segment))
  if (boundary === -1) return path
  const parent = segments.slice(0, boundary).join('/')
  return parent === '' ? '/' : parent
}
