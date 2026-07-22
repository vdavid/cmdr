/**
 * Fixtures for the five dialogs that do real work on mount: `delete-confirmation`,
 * `transfer-confirmation`, `mkdir-confirmation`, `new-file-confirmation`, and
 * `go-to-path`.
 *
 * These are BUILDERS, not data. Everything they'd otherwise invent (names, sizes,
 * folder flags, the listing handle) comes from the `GalleryDiskFixture` that
 * `disk-fixture.ts` resolved from the real fixture directory, so the scan tallies,
 * conflict warnings, and space figures on screen are the real ones. A hand-written
 * item list would have been the one thing this instrument can't do: fake the very
 * numbers the design displays.
 *
 * Raw copy on purpose: this module is dev-only and sits outside the i18n-enforced
 * areas, so fixture strings never reach the message catalog.
 */

import type { DeleteSourceItem } from '$lib/file-operations/delete/delete-dialog-utils'
import type { SortColumn, SortOrder, TransferOperationType } from '$lib/file-explorer/types'
import type { GalleryDiskFixture } from '../gallery-state.svelte'

/** Props of `DeleteDialog.svelte`, minus its callbacks. */
export interface DeleteFixture {
  sourceItems: DeleteSourceItem[]
  sourcePaths: string[]
  sourceFolderPath: string
  isPermanent: boolean
  supportsTrash: boolean
  isFromCursor: boolean
  sortColumn: SortColumn
  sortOrder: SortOrder
  sourceVolumeId: string
}

/** Props of `TransferDialog.svelte`, minus its callbacks. */
export interface TransferFixture {
  operationType: TransferOperationType
  sourcePaths: string[]
  destinationPath: string
  direction: 'left' | 'right'
  currentVolumeId: string
  fileCount: number
  folderCount: number
  sourceFolderPath: string
  sortColumn: SortColumn
  sortOrder: SortOrder
  sourceVolumeId: string
  destVolumeId: string
}

/** Props of `NewFolderDialog.svelte` / `NewFileDialog.svelte`, minus their callbacks. */
export interface NewEntryFixture {
  currentPath: string
  listingId: string
  showHiddenFiles: boolean
  initialName: string
  volumeId: string
}

/** Props of `GoToPathDialog.svelte`, minus its callbacks. */
export interface GoToPathFixture {
  baseDir: string
}

/** A builder turns the resolved fixture directory into one dialog's props. */
export type DiskFixtureBuilder<T> = (disk: GalleryDiskFixture) => T

/**
 * Real entries from the top of the fixture directory's listing, minus the folder
 * the transfer states copy INTO (copying a folder into itself is a different
 * dialog state than the one these rows advertise).
 */
function sources(disk: GalleryDiskFixture, count: number) {
  return disk.entries.filter((entry) => entry.path !== disk.destinationDir).slice(0, count)
}

/** The same `FileEntry` → `DeleteSourceItem` mapping the production trigger path does. */
function deleteFixture(
  disk: GalleryDiskFixture,
  count: number,
  options: { isPermanent: boolean; supportsTrash?: boolean },
): DeleteFixture {
  const entries = sources(disk, count)
  return {
    sourceItems: entries.map(
      (entry): DeleteSourceItem => ({
        name: entry.name,
        size: entry.size,
        isDirectory: entry.isDirectory,
        isSymlink: entry.isSymlink,
        recursiveSize: entry.recursiveSize,
        recursiveFileCount: entry.recursiveFileCount,
      }),
    ),
    sourcePaths: entries.map((entry) => entry.path),
    sourceFolderPath: disk.root,
    isPermanent: options.isPermanent,
    supportsTrash: options.supportsTrash ?? true,
    // One item is what a cursor delete looks like; several is a selection.
    isFromCursor: count === 1,
    sortColumn: disk.sortColumn,
    sortOrder: disk.sortOrder,
    sourceVolumeId: disk.volumeId,
  }
}

function transferFixture(disk: GalleryDiskFixture, operationType: TransferOperationType, count: number) {
  const entries = sources(disk, count)
  return {
    operationType,
    sourcePaths: entries.map((entry) => entry.path),
    destinationPath: disk.destinationDir,
    // The pane the files come FROM sits opposite the arrow the dialog draws.
    direction: disk.paneSide === 'left' ? ('right' as const) : ('left' as const),
    currentVolumeId: disk.volumeId,
    fileCount: entries.filter((entry) => !entry.isDirectory).length,
    folderCount: entries.filter((entry) => entry.isDirectory).length,
    sourceFolderPath: disk.root,
    sortColumn: disk.sortColumn,
    sortOrder: disk.sortOrder,
    sourceVolumeId: disk.volumeId,
    destVolumeId: disk.volumeId,
  }
}

function newEntryFixture(disk: GalleryDiskFixture, initialName: string): NewEntryFixture {
  return {
    currentPath: disk.root,
    listingId: disk.listingId,
    showHiddenFiles: disk.showHiddenFiles,
    initialName,
    volumeId: disk.volumeId,
  }
}

/**
 * A name well past the 255-byte limit, so the length validator's error state is
 * reviewable without typing 300 characters by hand.
 */
const OVER_LONG_NAME = `Every folder name I have ever wanted to type at once ${'and then some more '.repeat(14)}`

/** Keyed by the `delete-confirmation` entry's state ids in `gallery-registry.ts`. */
export const deleteFixtures: Record<string, DiskFixtureBuilder<DeleteFixture> | undefined> = {
  'trash-single': (disk) => deleteFixture(disk, 1, { isPermanent: false }),
  'trash-many': (disk) => deleteFixture(disk, 5, { isPermanent: false }),
  'permanent-single': (disk) => deleteFixture(disk, 1, { isPermanent: true }),
  'permanent-many': (disk) => deleteFixture(disk, 5, { isPermanent: true }),
  // A volume with no Trash (MTP, most network shares): the toggle is gone and
  // permanent is the only option, which is a different dialog to review.
  'no-trash-support': (disk) => deleteFixture(disk, 3, { isPermanent: true, supportsTrash: false }),
}

/** Keyed by the `transfer-confirmation` entry's state ids in `gallery-registry.ts`. */
export const transferFixtures: Record<string, DiskFixtureBuilder<TransferFixture> | undefined> = {
  copy: (disk) => transferFixture(disk, 'copy', 5),
  move: (disk) => transferFixture(disk, 'move', 5),
  'copy-single': (disk) => transferFixture(disk, 'copy', 1),
}

/** Keyed by the `mkdir-confirmation` entry's state ids in `gallery-registry.ts`. */
export const mkdirFixtures: Record<string, DiskFixtureBuilder<NewEntryFixture> | undefined> = {
  empty: (disk) => newEntryFixture(disk, ''),
  prefilled: (disk) => newEntryFixture(disk, 'Stockholm archipelago, July 2026'),
  // The name of a folder that really is in there, so the conflict warning comes
  // from the live listing rather than from a fixture claiming there's a clash.
  conflict: (disk) => newEntryFixture(disk, disk.existingFolderName),
  'too-long': (disk) => newEntryFixture(disk, OVER_LONG_NAME),
}

/** Keyed by the `new-file-confirmation` entry's state ids in `gallery-registry.ts`. */
export const newFileFixtures: Record<string, DiskFixtureBuilder<NewEntryFixture> | undefined> = {
  empty: (disk) => newEntryFixture(disk, ''),
  prefilled: (disk) => newEntryFixture(disk, 'trip-notes.md'),
  conflict: (disk) => newEntryFixture(disk, disk.existingFileName),
  'too-long': (disk) => newEntryFixture(disk, `${OVER_LONG_NAME}.md`),
}

/** Keyed by the `go-to-path` entry's state ids in `gallery-registry.ts`. */
export const goToPathFixtures: Record<string, DiskFixtureBuilder<GoToPathFixture> | undefined> = {
  'fixture-dir': (disk) => ({ baseDir: disk.root }),
}
