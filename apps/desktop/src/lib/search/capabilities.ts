/**
 * Search-results virtual-volume capability access.
 *
 * The per-kind capability table now lives in
 * [`lib/file-explorer/pane/volume-capabilities.ts`](../file-explorer/pane/volume-capabilities.ts)
 * (the single source of truth, keyed by `VolumeKind`). This module keeps two
 * Search-specific things:
 *
 *  - `SEARCH_RESULTS_NOT_A_FOLDER_TOAST`: the L10 user-facing toast string shown
 *    when a keyboard shortcut tries a destination-side action (paste / mkdir /
 *    rename) on a search-results pane. Imported by the dispatcher and tests; it
 *    stays here so the wording lives next to its other Search consumers.
 *  - `searchResultsVolumeCapabilities()`: a thin shim returning the
 *    `search-results` row of the per-kind table, for the one remaining caller
 *    (`SearchResultsView.svelte`). It yields the new `VolumeCapabilities` shape
 *    (so `canRename` is now `canRenameInPlace`). The shim retires when that view
 *    moves onto the FilePane `caps` descriptor.
 *
 * The search-results pane (`volumeId === 'search-results'`, path
 * `search-results://<snapshot-id>`) is a read-only view of a snapshot, not a
 * real directory: paste-into / mkdir / mkfile / rename don't make sense there,
 * but source-side ops (copy/move/delete, drag out) stay enabled because the
 * underlying paths are real.
 */

import { capabilitiesForKind, type VolumeCapabilities } from '$lib/file-explorer/pane/volume-capabilities'

/** Returns the capability flag set for the search-results virtual volume. */
export function searchResultsVolumeCapabilities(): VolumeCapabilities {
  return capabilitiesForKind('search-results')
}

/**
 * Returns the user-facing toast text shown when a keyboard shortcut tries to
 * do something the search-results pane doesn't support (paste, mkdir, rename).
 * Kept here so the wording stays consistent between the dispatcher and tests.
 */
export const SEARCH_RESULTS_NOT_A_FOLDER_TOAST = "Search results aren't a folder. Paste into a real folder instead."
