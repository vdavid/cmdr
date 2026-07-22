/**
 * Which gallery dialog the main window is currently previewing.
 *
 * Deliberately tiny and dependency-free: `routes/(main)/+page.svelte` imports
 * `isGalleryDialogOpen()`, so this module is the only part of the gallery a
 * production bundle can even see, and it pulls in no fixtures, no registry, and no
 * dialog components. Nothing writes the store outside dev, so the getter is a
 * constant `false` there.
 */

import type { SoftDialogId } from '$lib/ui/dialog-registry'

/** The dialog + named state the Debug window asked for. */
interface OpenGalleryDialog {
  dialogId: SoftDialogId
  /** A `DialogGalleryState.id` from the entry's `states` list. */
  stateId: string
}

const galleryState = $state<{ open: OpenGalleryDialog | null }>({ open: null })

/** Opens (or swaps to) a gallery preview. Re-triggering the same state remounts the dialog. */
export function openGalleryDialog(dialogId: SoftDialogId, stateId: string): void {
  galleryState.open = { dialogId, stateId }
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
