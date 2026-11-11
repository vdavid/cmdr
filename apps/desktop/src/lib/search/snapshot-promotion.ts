/**
 * Promoting the current result set into a real pane view, and remembering the search
 * that produced it.
 *
 * "Show all in main window" (⌥⏎) mints a `SearchSnapshot`, stores it under a fresh
 * `search-results://<id>`, pins its refcount via `setLastAttemptId` so it survives the
 * moment before the pane's history push, hands a still-running walk over to
 * `walk-handoff.svelte.ts`, and persists the search. The caller closes the dialog and
 * routes the pane; everything above the wire stops here.
 */

import {
  addRecentSearch,
  type HistoryEntry,
  type SearchQuery,
  type SearchResultEntry,
} from '#lib/tauri-commands/index.js'
import type { LiveRunView } from '#lib/query-ui/query-stream.js'
import { fetchAllRows } from './snapshot-fill'
import {
  buildHistoryFilters,
  getCaseSensitive,
  getExcludeSystemDirs,
  getLastAiLabel,
  getLastAiPrompt,
  getMode,
  getQuery,
  getResults,
  getResultsVolumeId,
  getScope,
  getTotalCount,
} from './search-state.svelte'
import {
  getOrCreate as createSnapshot,
  nextSnapshotId,
  setLastAttemptId,
  type SearchSnapshot,
} from './snapshot-store.svelte'
import { buildSnapshotLabel } from './snapshot-label'
import { handOffWalk } from './walk-handoff.svelte'

/** What the promotion produced, for the wrapper to route and to remember. */
export interface PanePromotion {
  /** The id the host routes the active pane to (`search-results://<id>`). */
  snapshotId: string
  /**
   * The run the pane is now being fed by, or `null` when nothing was still walking.
   * The dialog's close must NAME it (`releaseSearchIndex(handedOffRun)`), or the walk
   * dies the instant the pane appears and nothing anywhere reports it.
   */
  handedOffRunId: string | null
}

/**
 * Persists the current search to recent searches. Called whenever the user acts on a
 * result, treating it as a signal-rich event worth remembering: "Show all in main
 * window" AND opening a single result ("Go to file"). Plain Enter / auto-apply runs
 * don't persist (they'd be keystroke noise). For AI mode the entry carries the
 * original natural-language prompt, not the translated pattern. Best-effort: a
 * persistence failure never blocks the open.
 *
 * A DEFAULTED scope is deliberately not persisted: `scope` is `''` until the user sets
 * one, so the entry records "wherever I was" rather than baking in a machine-specific
 * absolute path nobody chose. Replaying it later re-resolves against the pane you're
 * standing in then, which is what "search here" meant in the first place. It also keeps
 * the history dedupe key meaningful (one "report" entry, not one per folder visited).
 */
export function persistRecentSearch(): void {
  const historyEntry: HistoryEntry = {
    id: crypto.randomUUID(),
    timestamp: Date.now(),
    mode: getMode(),
    query: getMode() === 'ai' ? (getLastAiPrompt() ?? getQuery()) : getQuery(),
    filters: buildHistoryFilters(),
    scope: getScope(),
    caseSensitive: getCaseSensitive(),
    excludeSystemDirs: getExcludeSystemDirs(),
    resultCount: getTotalCount(),
  }
  void addRecentSearch(historyEntry).catch(() => {
    // Silent on history persistence failure: the open still proceeds.
  })
}

/**
 * Builds the stored record from live dialog state, over the volume its rows live on.
 * `entries` / `totalCount` are passed in rather than read here: they may be the fuller
 * set the index just answered with, not what the dialog is showing.
 */
function buildSnapshot(
  id: string,
  label: string,
  volumeId: string,
  entries: SearchResultEntry[],
  totalCount: number,
): SearchSnapshot {
  // `HistoryFilters` (IPC type) uses `number | null` for absent fields; the
  // snapshot store uses `number | undefined`. Coerce so `null` doesn't sneak
  // into the snapshot's runtime shape.
  const hf = buildHistoryFilters()
  const snapshotFilters = {
    ...(hf.sizeMin != null ? { sizeMin: hf.sizeMin } : {}),
    ...(hf.sizeMax != null ? { sizeMax: hf.sizeMax } : {}),
    // Snapshot date filters intentionally omitted: the search-results pane
    // doesn't need them post-run (the snapshot stores the matched paths
    // directly, not the date predicate).
  }
  return {
    id,
    query: getQuery(),
    mode: getMode(),
    filters: snapshotFilters,
    scope: getScope(),
    volumeId,
    caseSensitive: getCaseSensitive(),
    excludeSystemDirs: getExcludeSystemDirs(),
    entries,
    totalCount,
    createdAt: Date.now(),
    label,
    // Every snapshot opens in the engine's ranked order; a header click is what
    // ever puts it in another one (`snapshot-sort.svelte.ts`).
    sort: null,
  }
}

/**
 * Promotes the current results into a snapshot the host can open in a pane. Returns
 * `null` when there's nothing to promote (the button is disabled in that state, but a
 * keyboard path can still reach here).
 *
 * **The pane gets every hit, not the rows the dialog is showing.** The dialog asks for
 * 30; before minting the snapshot this asks the index the same question with the pane's
 * ceiling (`snapshot-fill.ts`), so promoting 112 matches opens 112 rows. That holds with
 * a live run too: the index answers for what it covers right away, and a still-running
 * walk appends on top and tops the pane up when it ends. The rows on screen are the
 * fallback, for an ask that fails or answers with no more than them.
 *
 * `getLiveRun` is the run the dialog is still following, if any: the ONE case where a
 * search outlives its dialog, because its rows are about to be on screen in a pane. Read
 * AFTER the index ask, so a walk that ended meanwhile isn't handed off. Everything after
 * the handoff — confirming the run is still live, the toast, the snapshot appends, the
 * top-up when the walk ends, handing the run back if the dialog reopens — belongs to
 * `walk-handoff.svelte.ts`.
 *
 * `buildFullRowQuery` comes from `search-runners.ts`, which owns the query builder.
 */
export async function promoteResultsToPane(
  getLiveRun: () => { runId: string; view: LiveRunView } | null,
  buildFullRowQuery: () => Promise<SearchQuery>,
): Promise<PanePromotion | null> {
  const volumeId = getResultsVolumeId()
  // Rows and the volume they live on arrive in the same answer (the one-shot result, or
  // a live batch), so rows with no volume shouldn't exist. If they ever do, refusing
  // beats guessing `root` and pointing every action on them at the wrong drive.
  if (getResults().length === 0 || volumeId === null) return null
  const label = buildSnapshotLabel({
    mode: getMode(),
    query: getQuery(),
    aiPrompt: getLastAiPrompt(),
    aiLabel: getLastAiLabel(),
  })

  // Read BEFORE the awaits below: the dialog stays open across them, and an auto-apply
  // landing meanwhile would otherwise promote rows the user never saw.
  const shownRows = getResults()
  const shownTotal = getTotalCount()
  const fullQuery = await buildFullRowQuery().catch(() => null)
  const full = fullQuery && shownRows.length < shownTotal ? await fetchAllRows(fullQuery) : null
  // Never take rows away from what the user is looking at.
  const rows = full && full.entries.length > shownRows.length ? full : { entries: shownRows, totalCount: shownTotal }
  const liveRun = getLiveRun()

  const id = nextSnapshotId()
  createSnapshot(id, buildSnapshot(id, label, volumeId, rows.entries, rows.totalCount))
  setLastAttemptId(id)

  const handedOffRunId = liveRun
    ? handOffWalk({ runId: liveRun.runId, snapshotId: id, label, view: liveRun.view, refillQuery: fullQuery })
    : null

  persistRecentSearch()

  return { snapshotId: id, handedOffRunId }
}
