/**
 * Which gallery dialog the main window is currently previewing.
 *
 * Deliberately tiny and dependency-free: `routes/(main)/+page.svelte` imports
 * `isGalleryDialogOpen()`, so this module is the only part of the gallery a
 * production bundle can even see, and it pulls in no fixtures, no registry, and no
 * dialog components. Nothing writes the store outside dev, so the getter is a
 * constant `false` there. Type-only imports are fine here: they're erased.
 */

import type { SoftDialogId } from '$lib/ui/dialog-registry'
import type { FileEntry, SortColumn, SortOrder } from '$lib/file-explorer/types'

/**
 * The real directory the disk-backed dialogs work against, plus the live pane
 * state they need to behave for real.
 *
 * `delete-confirmation`, `transfer-confirmation`, `mkdir-confirmation`,
 * `new-file-confirmation`, and `go-to-path` all do real work on mount (scans,
 * conflict lookups, space queries, path resolution). Faking that would fake the
 * numbers the design displays, so `disk-fixture.ts` resolves this from the
 * fixture directory the dev-only Rust command creates and the pane it navigates
 * there.
 */
export interface GalleryDiskFixture {
  /** The fixture directory (`<app data dir>/dialog-gallery-fixtures`). */
  root: string
  /** A folder inside `root` the copy / move states use as their destination. */
  destinationDir: string
  /** A folder name directly inside `root`, for the mkdir conflict state. */
  existingFolderName: string
  /** A file name directly inside `root`, for the mkfile conflict state. */
  existingFileName: string
  /** A deep path inside `root`, for the "Go to path" preview. */
  nestedPath: string
  /** The focused pane, navigated to `root`. */
  paneSide: 'left' | 'right'
  /** That pane's live listing of `root` — the handle mkdir / mkfile need. */
  listingId: string
  /** The volume `root` sits on, as the pane resolved it. */
  volumeId: string
  showHiddenFiles: boolean
  sortColumn: SortColumn
  sortOrder: SortOrder
  /** Real top-of-listing entries from `root`, in the pane's sort order. */
  entries: FileEntry[]
}

/** The dialog + named state the Debug window asked for. */
interface OpenGalleryDialog {
  dialogId: SoftDialogId
  /** A `DialogGalleryState.id` from the entry's `states` list. */
  stateId: string
  /** Present only for the disk-backed dialogs. */
  disk?: GalleryDiskFixture
}

const galleryState = $state<{ open: OpenGalleryDialog | null }>({ open: null })

/** Opens (or swaps to) a gallery preview. Re-triggering the same state remounts the dialog. */
export function openGalleryDialog(dialogId: SoftDialogId, stateId: string, disk?: GalleryDiskFixture): void {
  galleryState.open = { dialogId, stateId, disk }
}

/** Closes whatever the gallery is previewing. The harness passes this as every fixture's `onClose`. */
export function closeGalleryDialog(): void {
  galleryState.open = null
}

/** The current preview, or `null`. Read reactively by `DialogGallery.svelte`. */
export function getOpenGalleryDialog(): OpenGalleryDialog | null {
  return galleryState.open
}

/**
 * True while a gallery preview is up. `+page.svelte`'s `isModalDialogOpen()` reads
 * this so global shortcuts don't fire behind a previewed dialog, which would look
 * like a dialog bug and poison the design review.
 */
export function isGalleryDialogOpen(): boolean {
  return galleryState.open !== null
}
