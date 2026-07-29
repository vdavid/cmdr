/**
 * The synthetic `..` row a pane shows above its listing when `hasParent`.
 *
 * It never comes from a backend listing, so the pane builds it: the cursor
 * fetch resolves it at index 0, and the entries snapshot puts it back at index 0
 * so selection indices line up (frontend index = backend index + 1). Its `path`
 * is the PARENT of the current directory, which is what Enter on the row
 * navigates to.
 */

import type { FileEntry } from '../types'
import { parentOf, type CanonicalPath } from '$lib/path/canonical'

/** Returns `null` at the filesystem root, where there's nothing above to show. */
export function createParentEntry(path: CanonicalPath): FileEntry | null {
  if (path === '/') return null
  return {
    name: '..',
    path: parentOf(path),
    isDirectory: true,
    isSymlink: false,
    permissions: 0o755,
    owner: '',
    group: '',
    iconId: 'dir',
    extendedMetadataLoaded: true,
  }
}
