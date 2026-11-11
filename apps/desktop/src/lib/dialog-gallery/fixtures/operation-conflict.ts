/**
 * The `operation-conflict` preview's clash per state.
 *
 * No fixture DATA here: the prompt is raised by a real copy, so a state only
 * picks which clash that copy walks into. The names and folders come from the
 * dev-only fixture command (`ConflictPreviewFixtures`), the side that creates
 * them on disk, and `operation-conflict-preview.ts` starts the copy.
 */

import type { ConflictPreviewFixtures } from '#lib/ipc/bindings.js'

/** What one state copies, and where to. */
export interface ConflictPreviewCopy {
  sourcePath: string
  destinationDir: string
}

function copyOf(fixtures: ConflictPreviewFixtures, name: string): ConflictPreviewCopy {
  // eslint-disable-next-line cmdr/no-raw-path-arithmetic -- a gallery fixture's own POSIX path
  return { sourcePath: `${fixtures.fromDir}/${name}`, destinationDir: fixtures.toDir }
}

export const operationConflictFixtures: Record<
  string,
  ((fixtures: ConflictPreviewFixtures) => ConflictPreviewCopy) | undefined
> = {
  'folder-over-file': (fixtures) => copyOf(fixtures, fixtures.folderOverFile),
  'file-over-folder': (fixtures) => copyOf(fixtures, fixtures.fileOverFolder),
  'file-over-file': (fixtures) => copyOf(fixtures, fixtures.fileOverFile),
}
