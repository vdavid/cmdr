/**
 * Fixtures for the viewer copy dialogs (`routes/viewer/ViewerCopyDialogs.svelte`).
 *
 * One component renders both `viewer-copy-confirm` and `viewer-copy-refuse`,
 * picked by which byte count is non-null, so both gallery rows feed the same
 * component with a different field set.
 *
 * These live in the VIEWER window in the shipping app; the gallery shows them
 * over the main window and the rows say so.
 *
 * Raw copy on purpose: this module is dev-only and sits outside the i18n-enforced
 * areas, so fixture strings never reach the message catalog.
 */

/** The one number either viewer copy dialog renders. */
export interface ViewerCopyFixture {
  bytes: number
}

/**
 * Keyed by the `viewer-copy-confirm` entry's state ids in `gallery-registry.ts`.
 * The `-1` sentinel means "size unknown" (a ByteSeek range we never scrolled
 * through) and swaps in a size-free title, so it's a real second state.
 */
export const viewerCopyConfirmFixtures: Record<string, ViewerCopyFixture | undefined> = {
  'known-size': { bytes: 48_312_320 },
  'unknown-size': { bytes: -1 },
}

/** Keyed by the `viewer-copy-refuse` entry's state ids in `gallery-registry.ts`. */
export const viewerCopyRefuseFixtures: Record<string, ViewerCopyFixture | undefined> = {
  'too-large': { bytes: 1_503_238_553 },
}
