// What image search can HONESTLY say about the folder a pane is standing in, derived
// from state the frontend already holds: the master toggle, the scope, the chosen and
// excluded folder lists, and the live per-volume enrichment activity.
//
// Deliberately NOT derived from per-folder counts: `media.db` has no cheap per-folder
// count (it would mean a prefix scan per folder per poll), so the states below are about
// COVERAGE (what the settings cover), never about completion. See
// `src-tauri/src/media_index/DETAILS.md` § Per-folder counts.
//
// The one thing this cannot resolve is the automatic scope: whether importance ranks a
// given folder above the threshold is a backend question with no frontend answer, so it
// gets its own honest state rather than a guessed yes or no.

/** The image-indexing scope, mirroring the backend `gate::IndexScope` tokens. */
export type MediaIndexScope = 'chosen' | 'importance'

/** What a pane can say about its current folder's image indexing. */
export type FolderIndexState =
  /** Image search is off (or there's no folder to judge): show nothing. */
  | 'off'
  /** The user excluded this folder (or an ancestor): the hard privacy veto. */
  | 'excluded'
  /** Covered by an explicit choice, and a pass is running on this drive right now. */
  | 'indexing'
  /** Covered by an explicit choice (this folder or an ancestor of it). */
  | 'indexed'
  /** The automatic scope with no explicit choice: importance decides, and we can't say. */
  | 'automatic'
  /** The narrow scope with no explicit choice: nothing here gets indexed. */
  | 'notIndexed'

export interface FolderIndexInputs {
  /** The `mediaIndex.enabled` master toggle. */
  enabled: boolean
  /** The `mediaIndex.scope` setting. */
  scope: MediaIndexScope
  /** `mediaIndex.alwaysIndexFolders`: absolute OS paths. */
  chosenFolders: string[]
  /** `mediaIndex.excludedFolders`: absolute OS paths. */
  excludedFolders: string[]
  /** The pane's current folder, as an absolute OS path. */
  folderPath: string
  /** Whether this pane's volume is ACTIVELY enriching (a paused pass doesn't count). */
  enriching: boolean
}

/**
 * Whether `path` is `ancestor` itself or lives under it. Trailing-slash-safe prefix
 * arithmetic, so `/Photos2` isn't "within" `/Photos`. Mirrors the Rust
 * `media_index::network::config::path_is_within`, which is what actually gates indexing;
 * keep the two in step or the pane voices a coverage the backend doesn't apply.
 */
export function pathIsWithin(path: string, ancestor: string): boolean {
  const base = ancestor.replace(/\/+$/, '')
  const target = path.replace(/\/+$/, '')
  if (base === '') return true
  return target === base || target.startsWith(`${base}/`)
}

/** The honest image-indexing state of one folder. */
export function deriveFolderIndexState(inputs: FolderIndexInputs): FolderIndexState {
  const { enabled, scope, chosenFolders, excludedFolders, folderPath, enriching } = inputs
  if (!enabled || folderPath === '') return 'off'

  // The exclusion is a hard veto backend-side, so it outranks every other answer.
  if (excludedFolders.some((folder) => pathIsWithin(folderPath, folder))) return 'excluded'

  const chosen = chosenFolders.some((folder) => pathIsWithin(folderPath, folder))
  if (chosen) return enriching ? 'indexing' : 'indexed'

  // No explicit choice: in the automatic scope importance decides (unknowable here), in
  // the narrow one nothing outside the list is covered.
  return scope === 'importance' ? 'automatic' : 'notIndexed'
}
