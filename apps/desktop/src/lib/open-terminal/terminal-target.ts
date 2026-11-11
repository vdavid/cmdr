/**
 * Which folder "Open terminal here" opens, and where it's offered at all.
 *
 * Both answers are pure: the caller reads the pane, this decides. That keeps the
 * cursor rules testable without a pane, and keeps ONE definition of "here" behind
 * the four surfaces (File menu, context menu, palette, shortcut).
 */

import type { VolumeKind } from '#lib/file-explorer/pane/volume-capabilities.js'
import { folderContainingArchive, pathCrossesArchiveBoundary } from '#lib/file-explorer/pane/archive-paths.js'
import { resticRepoRoot } from '#lib/file-explorer/restic/path-detection.js'
import { virtualGitBoundaryIndex } from '#lib/file-explorer/git/path-detection.js'

/** What the resolver needs to know about the pane it's acting on. */
export interface TerminalTargetPane {
  /** The pane's own directory (the active tab's path). */
  panePath: string
  /**
   * The kind of the pane's VOLUME, from `capabilitiesFor(volumeId).kind`.
   * ❗ Never `capabilitiesForPane`: an archive pane's kind-from-path would hide
   * the drive the archive actually lives on, which is what decides this.
   */
  volumeKind: VolumeKind
  /** The row under the cursor, or `null` when the pane has none. */
  cursorEntry: TerminalTargetCursorEntry | null
  /**
   * The live `fileExplorer.git.showVirtualGitPortal` toggle. With it off nothing
   * routes `.git/branches/…`, so such a path is whatever sits on disk.
   */
  virtualGitPortalOn: boolean
}

/** The cursor row, as much of it as the folder rules need. */
export interface TerminalTargetCursorEntry {
  /** The row's own name, so the synthetic `..` row can be told apart. */
  name: string
  path: string
  isDirectory: boolean
}

/**
 * Whether a pane on this kind of volume hands out paths a shell can `cd` into.
 *
 * `local` and `smb` do: an OS-mounted share and a direct smb2 session both keep an
 * ordinary `/Volumes/…` mount alive, which is the same reading Rust takes with
 * `Volume::paths_are_os_visible()`. MTP, ADB, SFTP, WebDAV, and S3 don't — none of
 * them has a mount, and their paths carry a scheme a shell can't `cd` into — and
 * the two virtual kinds (`network`, `search-results`) aren't folders at all.
 * The routed kinds (`archive`, `git-portal`, `restic`) never reach here: a pane
 * inside one is classified by the drive underneath it.
 *
 * ❌ Never a test on the path string. A share whose mount went away still looks
 * local; Rust catches that one at launch time and answers `not_a_local_path`.
 */
export function canOpenTerminalIn(volumeKind: VolumeKind): boolean {
  return volumeKind === 'local' || volumeKind === 'smb'
}

/**
 * The folder to open, or `null` when this pane has none.
 *
 * The rules, in order:
 *
 * 1. A volume with no OS-visible paths gets nothing.
 * 2. Inside an archive, the folder holding the archive FILE; inside a restic repo's
 *    snapshot view, the repo folder; inside the virtual `.git` portal (while it's
 *    on), the working tree, where git commands run. The inner path isn't on disk,
 *    and the holding folder is the nearest useful place a shell can stand. When
 *    they nest, the outermost wins, and its holding folder is the shortest path.
 * 3. A folder under the cursor is what the user is pointing at, so it wins over the
 *    pane's own folder. An archive file under the cursor doesn't: a shell can't
 *    enter it either. Nor does a routed folder row (a repo's `Browse snapshots`, a
 *    portal category), which has no inode.
 * 4. Otherwise the pane's own folder, which also covers `..`, a file, and an empty
 *    listing with no cursor at all.
 */
export function resolveTerminalFolder(pane: TerminalTargetPane): string | null {
  if (!canOpenTerminalIn(pane.volumeKind)) return null
  const holdingFolder = outermostHoldingFolder(pane.panePath, pane.virtualGitPortalOn)
  if (holdingFolder !== null) return holdingFolder

  const cursor = pane.cursorEntry
  const cursorIsEnterableFolder =
    cursor !== null &&
    cursor.name !== '..' &&
    cursor.isDirectory &&
    outermostHoldingFolder(cursor.path, pane.virtualGitPortalOn) === null
  return cursorIsEnterableFolder ? cursor.path : pane.panePath
}

/** The on-disk folder holding the outermost container `path` is inside, or `null` when it's on disk itself. */
function outermostHoldingFolder(path: string, virtualGitPortalOn: boolean): string | null {
  const candidates = [
    pathCrossesArchiveBoundary(path) ? folderContainingArchive(path) : null,
    resticRepoRoot(path),
    virtualGitPortalOn ? gitWorkingTree(path) : null,
  ].filter((folder): folder is string => folder !== null)
  if (candidates.length === 0) return null
  return candidates.reduce((outer, folder) => (folder.length < outer.length ? folder : outer))
}

/** The working tree a virtual `.git` portal path belongs to (the folder holding its `.git`), or `null`. */
function gitWorkingTree(path: string): string | null {
  // eslint-disable-next-line cmdr/no-raw-path-arithmetic -- finds a `.git` portal component by name; the working tree above it is rejoined with the same `/`
  const components = path.split('/')
  const gitAt = virtualGitBoundaryIndex(components)
  if (gitAt === -1) return null
  return components.slice(0, gitAt).join('/') || '/'
}
