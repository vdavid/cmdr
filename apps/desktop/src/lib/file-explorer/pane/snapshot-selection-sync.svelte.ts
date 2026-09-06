/**
 * Keeps a search-results pane's cursor and selection pointing at the rows the
 * user picked while the snapshot underneath them shrinks.
 *
 * A normal pane gets this for free: the backend emits `directory-diff`, and
 * `listing-diff-sync.svelte.ts` remaps both across it (plus the per-item
 * deselection off `write-source-item-done`). A snapshot pane has no backend
 * listing and no listing id, so none of that runs for it — and the selection is
 * a set of INDICES into an array `snapshot-store::removeEntryFromAllSnapshots`
 * replaces with a shorter one. Delete rows 2 and 3 of five and the selection
 * still reads `{2, 3}`, which now names the fifth row and nothing; the next F8
 * deletes a file nobody selected.
 *
 * So the pane watches its own entries array and remaps by PATH, following the
 * same rule the diff path uses: a row that survived keeps its selection at its
 * new index, a row that is gone leaves the selection, and the cursor follows its
 * own row or slides to whatever took its place.
 *
 * ❌ Not a fix in `transfer-pane-effects::clearSourcePaneAfterTransfer`. That one
 * clears the SOURCE pane after an operation it started, gated on the pane still
 * showing the folder the operation was born in (`sourceFolderPath`), which a
 * snapshot pane's `search-results://<id>` never equals. Widening that gate would
 * only cover deletes this pane started; the array also shrinks when another
 * window deletes the same file, when a move purges its sources, and when a
 * result is trashed from a normal pane. Watching the array covers all of them.
 */

import { untrack } from 'svelte'

/** One row of a snapshot pane, as much of it as the remap needs. */
export interface SnapshotSelectionRow {
  path: string
}

export interface SnapshotSelectionSyncDeps {
  /**
   * The pane's snapshot rows, or `undefined` when it isn't showing a snapshot.
   * Must read the store's mutation tick so a purge or an append re-runs the
   * effect (snapshots themselves are not `$state`; see the store's header).
   */
  getSnapshotEntries: () => readonly SnapshotSelectionRow[] | undefined
  getCursorIndex: () => number
  getSelectedIndices: () => number[]
  setSelectedIndices: (indices: number[]) => void
  /** Direct cursor write, no scroll or fetch side effects. */
  applyCursorIndex: (index: number) => void
}

/**
 * Where the cursor and each selected index land once `previousPaths` has become
 * `currentPaths`. Pure; the effect below owns the before/after bookkeeping.
 *
 * A cursor whose own row survived follows it. One whose row is gone stays at the
 * same screen position, clamped into the shorter list, which is what
 * `listing-diff-sync::reconcileCursorAndSelection` does on a normal pane.
 */
export function remapSnapshotSelection(input: {
  previousPaths: readonly string[]
  currentPaths: readonly string[]
  cursorIndex: number
  selectedIndices: readonly number[]
}): { cursorIndex: number; selectedIndices: number[] } {
  const { previousPaths, currentPaths, cursorIndex, selectedIndices } = input
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- local scratch inside a pure function, built and dropped per call. Nothing subscribes to it, so a SvelteMap would only add proxy overhead.
  const indexByPath = new Map<string, number>()
  for (let i = 0; i < currentPaths.length; i++) {
    if (!indexByPath.has(currentPaths[i])) indexByPath.set(currentPaths[i], i)
  }

  const remappedSelection: number[] = []
  for (const index of selectedIndices) {
    const path = previousPaths[index] as string | undefined
    if (path === undefined) continue
    const next = indexByPath.get(path)
    if (next !== undefined) remappedSelection.push(next)
  }
  remappedSelection.sort((a, b) => a - b)

  const cursorPath = previousPaths[cursorIndex] as string | undefined
  const followed = cursorPath === undefined ? undefined : indexByPath.get(cursorPath)
  const remappedCursor = followed ?? Math.max(0, Math.min(cursorIndex, currentPaths.length - 1))

  return { cursorIndex: remappedCursor, selectedIndices: remappedSelection }
}

/** True when the two index lists differ, so an unchanged selection isn't rewritten. */
function differs(a: readonly number[], b: readonly number[]): boolean {
  return a.length !== b.length || a.some((value, i) => value !== b[i])
}

/**
 * Installs the effect. Create it from the pane's `<script>` so it keeps its place
 * in the pane's effect order; it is inert on every pane that isn't showing a
 * snapshot.
 */
export function createSnapshotSelectionSync(deps: SnapshotSelectionSyncDeps): void {
  /** The entries array this pane last reconciled against. */
  let seenEntries: readonly SnapshotSelectionRow[] | null = null

  $effect(() => {
    const entries = deps.getSnapshotEntries()
    if (!entries) {
      seenEntries = null
      return
    }
    const previous = seenEntries
    seenEntries = entries
    // Nothing to reconcile the first time we see a snapshot, and a tick bumped by
    // a mutation to a DIFFERENT snapshot hands back the same array.
    if (previous === null || previous === entries) return

    // Reading the cursor and selection must not subscribe the effect to them: it
    // reacts to the entries array alone, and writes both back.
    untrack(() => {
      const cursorIndex = deps.getCursorIndex()
      const selectedIndices = deps.getSelectedIndices()
      const remapped = remapSnapshotSelection({
        previousPaths: previous.map((e) => e.path),
        currentPaths: entries.map((e) => e.path),
        cursorIndex,
        selectedIndices,
      })
      if (differs(remapped.selectedIndices, selectedIndices)) deps.setSelectedIndices(remapped.selectedIndices)
      if (remapped.cursorIndex !== cursorIndex) deps.applyCursorIndex(remapped.cursorIndex)
    })
  })
}
