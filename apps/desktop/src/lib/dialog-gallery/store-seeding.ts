/**
 * The store-seeded mechanism: patch a real app store, let the app's own mount
 * site render the dialog, and put the store back exactly as it was.
 *
 * Some dialogs take no content props at all (`BulkRenameReviewDialog`), or
 * self-gate on a flow store and are mounted bare by the app
 * (`FeedbackDialog`, `ErrorReportDialog`), so rendering them from the harness
 * renders them EMPTY. Seeding is the faithful path for those: the real trigger
 * state, the real mount site, the real component.
 *
 * It's also the one mechanism that MUTATES the running app, so the undo is
 * derived from the patch's own keys rather than written by hand per fixture. A
 * fixture can't forget to restore a field it set, and can't restore a field it
 * never touched. `DialogGallery.svelte` runs the returned closure as an
 * `$effect` cleanup, so closing the dialog, swapping to another preview, and
 * unmounting all put the store back.
 *
 * Details: [DETAILS.md](DETAILS.md) § Store-seeded dialogs.
 */

/**
 * Applies `patch` to `store` and returns the exact undo: the pre-patch value of
 * every key the patch names, restored by reference. Fields the patch didn't
 * name are never read and never written.
 */
export function seedStore<T extends object>(store: T, patch: Partial<T>): () => void {
  const keys = Object.keys(patch) as Array<keyof T>
  const snapshot = new Map<keyof T, T[keyof T]>()
  for (const key of keys) snapshot.set(key, store[key])
  Object.assign(store, patch)

  return () => {
    for (const [key, value] of snapshot) store[key] = value
  }
}

/** One store-seeded preview, ready for the harness to apply and watch. */
export interface StoreSeed {
  /** Seeds the store and returns the undo for exactly what it changed. */
  apply: () => () => void
  /**
   * True while the app's own mount site is showing the seeded dialog. The
   * harness watches it because such a dialog closes through ITS OWN store
   * (Escape, its Cancel button), never through `closeGalleryDialog()`; without
   * this the gallery would still believe a preview is up and `+page.svelte`
   * would keep suppressing global shortcuts.
   */
  isOpen: () => boolean
  /**
   * The store the patch lands on. Exposed so the tests can snapshot it whole
   * and prove the restore left nothing behind.
   */
  store: object
}

/** Binds a store, a fixture patch, and the store's own "is it showing?" read into one seed. */
export function storeSeed<T extends object>(store: T, patch: Partial<T>, isOpen: () => boolean): StoreSeed {
  return { apply: () => seedStore(store, patch), isOpen, store }
}
