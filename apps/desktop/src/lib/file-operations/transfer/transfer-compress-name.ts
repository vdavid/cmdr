/**
 * Pure helper deriving the suggested archive filename for the Transfer dialog's
 * compress mode. No reactivity, no IPC — unit-testable in isolation.
 *
 * The suggestion is only a default: the path field stays editable, so this never
 * has to be perfect, just a sensible starting point that matches the orthodox
 * two-pane convention (Total Commander defaults the archive name to the item
 * under the cursor, or the parent folder for a multi-selection).
 */

/** The last path segment, trailing slashes stripped. Empty at a volume root. */
function leafName(path: string): string {
  const norm = path.replace(/\/+$/, '')
  const idx = norm.lastIndexOf('/')
  return idx >= 0 ? norm.slice(idx + 1) : norm
}

/**
 * Suggests a `<name>.zip` filename for compressing `sourcePaths`:
 *  - single source → `<basename>.zip` (the leaf as-is; a folder `photos` becomes
 *    `photos.zip`, a file `report.pdf` becomes `report.pdf.zip`). A `.zip` source
 *    gets no special treatment (`data.zip` → `data.zip.zip`): it targets a NEW
 *    archive, so we don't dedupe the extension.
 *  - multiple sources → `<source-directory-basename>.zip`, falling back to the
 *    first selection's basename when the source directory is a volume root (its
 *    basename is empty).
 *
 * The extension is never stripped, so a folder whose name contains a dot
 * (`my.photos`) is never mangled, and the suggestion never silently collides
 * with a source. Returns `archive.zip` for the degenerate empty-basename case
 * (a volume root selected as the only source), which the dialog never hits in
 * practice but keeps the helper total.
 */
export function suggestCompressArchiveName(sourcePaths: string[], sourceFolderPath: string): string {
  if (sourcePaths.length <= 1) {
    const base = leafName(sourcePaths[0] ?? '')
    return base === '' ? 'archive.zip' : `${base}.zip`
  }
  const dirBase = leafName(sourceFolderPath)
  const base = dirBase !== '' ? dirBase : leafName(sourcePaths[0])
  return base === '' ? 'archive.zip' : `${base}.zip`
}
