/**
 * Search-results virtual-volume capability access.
 *
 * The per-kind capability table is the single source of truth, keyed by
 * `VolumeKind` in
 * [`lib/file-explorer/pane/volume-capabilities.ts`](../file-explorer/pane/volume-capabilities.ts).
 * Consumers that need the `search-results` row read it directly via
 * `capabilitiesForKind('search-results')` / `capabilitiesFor(volumeId)`; there's
 * no Search-specific capabilities shim anymore.
 *
 * This module keeps one Search-specific thing:
 *
 *  - `SEARCH_RESULTS_NOT_A_FOLDER_TOAST`: the L10 user-facing toast string shown
 *    when a keyboard shortcut tries a destination-side action (paste / mkdir /
 *    rename) on a search-results pane. Imported by the dispatcher, the transfer
 *    opener, and tests; it stays here so the wording lives next to its other
 *    Search consumers.
 *
 * The search-results pane (`volumeId === 'search-results'`, path
 * `search-results://<snapshot-id>`) is a read-only view of a snapshot, not a
 * real directory: paste-into / mkdir / mkfile / rename don't make sense there,
 * but source-side ops (copy/move/delete, drag out) stay enabled because the
 * underlying paths are real.
 */

import { tString } from '$lib/intl/messages.svelte'

/**
 * The user-facing toast text shown when a keyboard shortcut tries to do something
 * the search-results pane doesn't support (paste, mkdir, rename). Kept here so the
 * wording stays consistent between the dispatcher and tests.
 *
 * Resolved from the message catalog at module load (English-only ships today, no
 * locale picker). It stays a string const because out-of-scope consumers
 * (`command-dispatch.ts`, `transfer-entry.ts`) import it by name; a future locale
 * switch would turn this into a getter alongside those call sites.
 */
export const SEARCH_RESULTS_NOT_A_FOLDER_TOAST = tString('search.notAFolderToast')
