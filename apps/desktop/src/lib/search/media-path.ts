// Reconstruct an openable OS path from a `media_index` OCR hit. Stored hit paths
// are INDEX-RELATIVE (the media.db row keeps the index identity, matching the index
// + GC set): for a local volume that's already the absolute OS path (mount root is
// `/`), but for a network (SMB) volume it's mount-relative (like `/DCIM/x.jpg`) and
// needs the volume's mount root prepended to reach the real file
// (`/Volumes/naspi/DCIM/x.jpg`). Mirrors the backend `os_join` (media_index/DETAILS.md
// § "The byte-fetch decision" → Path mapping), kept pure + FE-side so the search grid
// can mint thumbnail tokens and open results with the correct absolute path.

/**
 * Prepend `mountRoot` to an index-relative OCR hit `path`. When `mountRoot` is the
 * filesystem root (`/`) or empty, the index-relative path is already absolute and
 * passes through unchanged (the local case). Otherwise the mount-relative path
 * (leading `/`) is joined onto the mount root with exactly one separator.
 */
export function resolveMediaHitPath(mountRoot: string, indexRelativePath: string): string {
  if (mountRoot === '' || mountRoot === '/') return indexRelativePath
  const base = mountRoot.endsWith('/') ? mountRoot.slice(0, -1) : mountRoot
  const rel = indexRelativePath.startsWith('/') ? indexRelativePath : `/${indexRelativePath}`
  return `${base}${rel}`
}
