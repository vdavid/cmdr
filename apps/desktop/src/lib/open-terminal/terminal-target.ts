/**
 * Which folder "Open terminal here" opens, and where it's offered at all.
 *
 * Both answers are pure: the caller reads the pane, this decides. That keeps the
 * cursor rules testable without a pane, and keeps ONE definition of "here" behind
 * the four surfaces (File menu, context menu, palette, shortcut).
 */

import { folderContainingArchive, pathInsideArchive, type VolumeKind } from '$lib/file-explorer/pane/volume-capabilities'

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
 * `Volume::paths_are_os_visible()`. MTP and ADB don't, and the two virtual kinds
 * (`network`, `search-results`) aren't folders at all. `archive` never reaches
 * here: an archive pane is classified by the drive underneath it.
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
 * 2. Inside an archive, the folder holding the archive FILE. The inner path isn't
 *    on disk, and the containing folder is the nearest place a shell can stand.
 * 3. A folder under the cursor is what the user is pointing at, so it wins over the
 *    pane's own folder. An archive file under the cursor doesn't: a shell can't
 *    enter it either.
 * 4. Otherwise the pane's own folder, which also covers `..`, a file, and an empty
 *    listing with no cursor at all.
 */
export function resolveTerminalFolder(pane: TerminalTargetPane): string | null {
  if (!canOpenTerminalIn(pane.volumeKind)) return null
  if (pathInsideArchive(pane.panePath)) return folderContainingArchive(pane.panePath)

  const cursor = pane.cursorEntry
  const cursorIsEnterableFolder =
    cursor !== null && cursor.name !== '..' && cursor.isDirectory && !pathInsideArchive(cursor.path)
  return cursorIsEnterableFolder ? cursor.path : pane.panePath
}
