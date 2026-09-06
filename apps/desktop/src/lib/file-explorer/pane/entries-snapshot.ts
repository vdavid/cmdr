/**
 * Materializing what a pane currently holds, for consumers that need a list
 * rather than a live view:
 *
 * - `fetchEntriesSnapshot` gives the Selection dialog every entry at open time,
 *   with the synthetic `..` row at index 0 so the returned indices line up with
 *   the pane's selection indices (the matcher then skips index 0 through the
 *   existing `hasParent` rule in `selection-state::applyIndices`).
 * - `fetchSelectedNames` pins the selection as NAMES before an operation starts,
 *   so the watcher diff can adjust the selection as files disappear under it.
 *   Names, not indices: indices shift the moment a row is removed.
 *
 * Both degrade quietly (an empty list, a short list) rather than throwing into a
 * dialog that's already opening.
 */

import { getFileAt, getFileRange } from '$lib/tauri-commands'
import type { FileEntry } from '../types'
import type { CanonicalPath } from '$lib/path/canonical'
import type { SearchSnapshot } from '$lib/search/snapshot-store.svelte'
import { createParentEntry } from './parent-entry'

export interface EntriesSnapshotInput {
  listingId: string
  totalCount: number
  hasParent: boolean
  showHiddenFiles: boolean
  /** `currentPath` with `~` expanded, for the synthetic `..` row. */
  canonicalPath: CanonicalPath | null
  isSearchResultsView: boolean
  searchSnapshot: SearchSnapshot | undefined
}

export async function fetchEntriesSnapshot(input: EntriesSnapshotInput): Promise<FileEntry[]> {
  if (input.isSearchResultsView) {
    // Adapt SearchResultEntry → FileEntry. `entry.name` is the BASENAME the
    // search matcher judged the row under (`search/engine.rs::build_result_entry`),
    // and it stays the basename here: the Selection dialog matches a mask like
    // `*.txt` against this field, and that has to mean the filename.
    //
    // ⚠️ It is NOT what the pane displays. `SearchResultsView::adaptEntry`
    // synthesizes its own `name` (the `~`-shortened full path) for the Name
    // column. Two adapters, deliberately different: one feeds a matcher, the
    // other a column. `parentPath` is home-relative (`~/…`) from the same Rust
    // builder, so it is display text, never a path to join onto.
    const sn = input.searchSnapshot
    if (!sn) return []
    return sn.entries.map((e): FileEntry => ({
      name: e.name,
      path: e.path,
      parentPath: e.parentPath,
      isDirectory: e.isDirectory,
      isSymlink: false,
      size: e.size ?? undefined,
      modifiedAt: e.modifiedAt ?? undefined,
      permissions: 0,
      owner: '',
      group: '',
      iconId: e.iconId,
      extendedMetadataLoaded: true,
    }))
  }

  const canonical = input.canonicalPath
  const synthetic = canonical ? createParentEntry(canonical) : null
  if (!input.listingId || input.totalCount === 0) {
    // Synthetic `..` entry (when present) keeps the index alignment.
    return input.hasParent && synthetic ? [synthetic] : []
  }
  try {
    const fetched = await getFileRange(input.listingId, 0, input.totalCount, input.showHiddenFiles)
    if (input.hasParent) {
      return synthetic ? [synthetic, ...fetched] : fetched
    }
    return fetched
  } catch {
    return []
  }
}

export interface SelectedNamesInput {
  listingId: string
  includeHidden: boolean
  hasParent: boolean
  /** Every selectable row is selected; the caller stores `'all'` instead of a list. */
  isAllSelected: boolean
  /** The pane's selected indices (frontend indices, `..` included when present). */
  selectedIndices: number[]
}

export async function fetchSelectedNames(input: SelectedNamesInput): Promise<string[] | 'all'> {
  // No backend listing, no operation snapshot. A search-results pane is the case
  // that reaches here: `getFileAt('')` rejects with "Listing not found", and the
  // caller starts this as `void snapshotSelectionForOperation()`, so the rejection
  // became an unhandled one on every operation started from a snapshot pane. The
  // snapshot would have nothing to feed anyway — the listing diff that consumes
  // it doesn't run on a pane without a listing.
  //
  // ⚠️ Ahead of the `all` short-circuit on purpose: `'all'` is a recorded snapshot
  // too, and `transfer-pane-effects::clearSourcePaneAfterTransfer` can't clear it
  // on such a pane (its gate compares the pane's path to the operation's birth
  // FOLDER, which a `search-results://<id>` path never equals).
  if (!input.listingId) return []

  if (input.isAllSelected) return 'all'

  const names: string[] = []
  for (const frontendIndex of input.selectedIndices) {
    const backendIndex = input.hasParent ? frontendIndex - 1 : frontendIndex
    if (backendIndex < 0) continue
    const entry = await getFileAt(input.listingId, backendIndex, input.includeHidden)
    if (entry) names.push(entry.name)
  }
  return names
}
