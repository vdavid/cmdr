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

import { addRecentSearch, type HistoryEntry } from '$lib/tauri-commands'
import type { LiveRunView } from '$lib/query-ui/query-stream'
import {
  buildHistoryFilters,
  getCaseSensitive,
  getExcludeSystemDirs,
  getLastAiLabel,
  getLastAiPrompt,
  getMode,
  getQuery,
  getResults,
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

/** Builds the stored record from live dialog state. */
function buildSnapshot(id: string, label: string): SearchSnapshot {
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
    caseSensitive: getCaseSensitive(),
    excludeSystemDirs: getExcludeSystemDirs(),
    entries: getResults(),
    totalCount: getTotalCount(),
    createdAt: Date.now(),
    label,
  }
}

/**
 * Promotes the current results into a snapshot the host can open in a pane. Returns
 * `null` when there's nothing to promote (the button is disabled in that state, but a
 * keyboard path can still reach here).
 *
 * `liveRun` is the run still walking, if any: the ONE case where a search outlives its
 * dialog, because its rows are about to be on screen in a pane. Everything after the
 * handoff — the toast, the snapshot appends, handing the run back if the dialog reopens —
 * belongs to `walk-handoff.svelte.ts`.
 */
export function promoteResultsToPane(liveRun: { runId: string; view: LiveRunView } | null): PanePromotion | null {
  if (getResults().length === 0) return null
  const id = nextSnapshotId()
  const label = buildSnapshotLabel({
    mode: getMode(),
    query: getQuery(),
    aiPrompt: getLastAiPrompt(),
    aiLabel: getLastAiLabel(),
  })
  createSnapshot(id, buildSnapshot(id, label))
  setLastAttemptId(id)

  const handedOffRunId = liveRun
    ? handOffWalk({ runId: liveRun.runId, snapshotId: id, label, view: liveRun.view })
    : null

  persistRecentSearch()

  return { snapshotId: id, handedOffRunId }
}
