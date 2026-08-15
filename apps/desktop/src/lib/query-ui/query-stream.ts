/**
 * The contract for a query whose answer arrives over time, plus the pure pieces
 * that turn a run's progress into something a person reads.
 *
 * Search is the only consumer: on a folder the index doesn't cover, a search walks
 * it, so the answer arrives in batches over seconds or minutes instead of one
 * promise. Selection matches a pane listing it already holds, so it never streams
 * and leaves `streamingSource` undefined.
 *
 * The consumer owns the transport (Search's `lib/search/live-search-source.ts` wires
 * the Tauri events); `query-runner.svelte.ts` owns the run: which run id is current,
 * appending batches, holding the cursor, and the final re-rank. Nothing about Tauri,
 * coverage, or walks reaches this file — that's what keeps the shared dialog free of
 * Search's vocabulary.
 */

import { tString } from '$lib/intl/messages.svelte'
import { formatInteger } from '$lib/intl/number-format'
import type { SearchResultEntry } from '$lib/tauri-commands'

/**
 * Which part of a live run produced an update.
 *
 * Four honest waits rather than one spinner: working out what's already covered can
 * mean a multi-second index load, being queued behind another run's walk is somebody
 * else's clock entirely, reading the index is fast, and the walk is unbounded.
 *
 * The run says which one it's in; ❌ nothing here infers it. A run queued behind
 * another walk has no walk of its own, so `waitingForAnotherWalk` must never render
 * as walking — that is what put "0 folders scanned" under a sentence about walking.
 */
export type QueryStreamPhase = 'resolvingCoverage' | 'waitingForAnotherWalk' | 'readingIndex' | 'walking'

/** A batch of rows, plus where the run has got to. */
export interface QueryStreamProgress {
  phase: QueryStreamPhase
  /** Rows found since the last update, in arrival order. Empty is normal. */
  entries: SearchResultEntry[]
  /** Matches so far, counting the ones past the row cap. */
  matchCount: number
  /** Directories the walk has turned up. No denominator by design: the total is unknowable. */
  dirsFound: number
  /** Where the walk was as of this batch. Indicative, not a cursor. */
  currentPath: string | null
  /** The row cap is reached: no more rows will arrive, the count keeps rising. */
  capped: boolean
}

/** How a live run ended. */
export interface QueryStreamEnd {
  /** The run's total. Exact for a run that covered its ground; a lower bound otherwise. */
  matchCount: number
  /**
   * The run left ground uncovered, so the list and the count are lower bounds:
   * somebody stopped it, or it stopped on its own (a drive that went away). WHY it's
   * short is the consumer's to say — Search's coverage note has the room for it.
   */
  incomplete: boolean
  /**
   * A walk contributed rows, so the list is arrival-ordered and worth re-ranking.
   * False when the index answered the whole scope, where the rows are already ranked
   * and re-ranking them would only throw the ranking away.
   */
  walked: boolean
  capped: boolean
}

/** What the runner hands a streaming source so the source can report back. */
export interface QueryStreamCallbacks {
  onProgress: (update: QueryStreamProgress) => void
  onEnd: (end: QueryStreamEnd) => void
  /** The run couldn't run at all. `message` is the sentence to show. */
  onFailed: (message: string) => void
}

/**
 * A run still going from before this dialog mounted, handed back so the runner can
 * pick it up where it was.
 */
export interface QueryStreamResumption {
  /** The run to measure every later update against. */
  runId: string
  /** Where the run has got to, so the dialog reopens showing it rather than a finished list. */
  view: LiveRunView
  /**
   * Rows the run found while nobody was listening, in arrival order. Appending them
   * is what keeps the list and the count telling the same story; without it the
   * dialog would say "1,200 matches" over 300 rows.
   */
  missedEntries: SearchResultEntry[]
  /** Detaches, without stopping the work — the same promise `start`'s teardown makes. */
  stop: () => void
}

/** The consumer-owned transport for a query that answers over time. */
export interface QueryStreamSource {
  /**
   * Starts the run named by `runId` and resolves with its teardown. The runner mints
   * the id, so no update can arrive against one it hasn't seen. Rejecting means the
   * run never started (a scope spanning two volumes, say), and the runner surfaces the
   * reason.
   */
  start: (runId: string, callbacks: QueryStreamCallbacks) => Promise<() => void>
  /**
   * Stops the run named by `runId` and whatever work stands behind it. Only Escape and
   * the dialog closing call this: a refined query SUPERSEDES its predecessor (the
   * runner drops the id) rather than cancelling the work already in flight.
   */
  cancel: (runId: string) => void
  /**
   * A run the consumer deliberately kept alive after the last dialog closed, or
   * `null` when there isn't one. Called once per runner, before the dialog decides
   * to re-run anything: adopting beats re-running, because a fresh run would
   * SUPERSEDE the live one and strand whatever it was still feeding.
   *
   * Synchronous by design — the decision gates the dialog's mount path, so it can't
   * wait on a round trip. The consumer is already listening; this only adds a second
   * reader.
   */
  resume?: (callbacks: QueryStreamCallbacks) => QueryStreamResumption | null
  /**
   * The order to leave a completed run's rows in. Called once, on completion, and only
   * for a run that walked; skipped once the user has moved the cursor, because
   * reordering the list under someone reading it is worse than arrival order.
   */
  rankOnCompletion?: (entries: SearchResultEntry[]) => SearchResultEntry[]
}

/** What the dialog renders while (and after) a live run. `null` when the last run wasn't one. */
export interface LiveRunView {
  phase: QueryStreamPhase
  matchCount: number
  dirsFound: number
  currentPath: string | null
  capped: boolean
  /** Still going, so it can still be stopped. */
  running: boolean
  /** Set by the terminal update: the answer is a lower bound. */
  incomplete: boolean
}

/** The spinner label for a run with nothing to show yet. One sentence per phase. */
export function livePhaseLabel(phase: QueryStreamPhase): string {
  if (phase === 'resolvingCoverage') return tString('queryUi.results.live.resolvingCoverage')
  if (phase === 'waitingForAnotherWalk') return tString('queryUi.results.live.waitingForAnotherWalk')
  if (phase === 'readingIndex') return tString('queryUi.results.live.readingIndex')
  return tString('queryUi.results.live.walking')
}

/**
 * The status-bar sentence for a live run, in the four shapes it can take.
 *
 * Rules, in order:
 *   1. Still running → "N matches so far", never a total (the walk hasn't finished
 *      counting, and a live count-only run can over-count its overlap).
 *   2. Ended short (stopped, or the drive went away) → the counts plus the admission
 *      that Cmdr didn't finish. The reason lives in the consumer's own note.
 *   3. Ended capped, with matches BEHIND the cap → the rows stopped at the cap while
 *      the count carried on past it. A run whose matches total exactly the cap
 *      reports `capped` too (the flag is "the cap was reached"), and there is nothing
 *      behind that last row, so it takes rule 4 instead — same guard the snapshot
 *      label uses (`search/snapshot-store.svelte.ts::labelFor`).
 *   4. Ended covered → `''`, so the caller falls back to its ordinary result line.
 */
export function liveStatusLine(view: LiveRunView, shownCount: number): string {
  if (view.running) {
    return tString('queryUi.results.live.matchesSoFar', {
      count: view.matchCount,
      countText: formatInteger(view.matchCount),
    })
  }
  if (view.incomplete) {
    // "0 of 0 results" in front of "didn't finish looking" is two numbers saying
    // nothing. Stopping a search before anything matched is ordinary, so that case
    // gets the sentence without the arithmetic.
    if (view.matchCount === 0) return tString('queryUi.results.live.incompleteEmpty')
    return tString('queryUi.results.live.incomplete', {
      shownText: formatInteger(shownCount),
      totalText: formatInteger(view.matchCount),
    })
  }
  if (view.capped && shownCount < view.matchCount) {
    return tString('queryUi.results.live.capped', {
      shownText: formatInteger(shownCount),
      totalText: formatInteger(view.matchCount),
    })
  }
  return ''
}

/** The walk's own progress, shown beside the match count while it runs. */
export function liveWalkProgress(view: LiveRunView): string {
  if (!view.running || view.phase !== 'walking') return ''
  return tString('queryUi.results.live.foldersScanned', {
    count: view.dirsFound,
    countText: formatInteger(view.dirsFound),
  })
}

/** How often a live run may interrupt a screen reader. */
export const LIVE_ANNOUNCE_INTERVAL_MS = 2000

/** Throttles what an `aria-live` region says, so a per-batch counter can't flood it. */
export interface AnnouncementThrottle {
  /** The text most recently taken. Starts empty. */
  readonly text: string
  /**
   * Offers `text` for announcement. Returns whether it was taken: at most one every
   * [`LIVE_ANNOUNCE_INTERVAL_MS`], plus every `final` one (a run's last word is always
   * worth hearing, and it's the only announcement a fast run makes at all).
   */
  offer: (text: string, final: boolean) => boolean
}

/**
 * A live run emits a batch every 100 ms, and the status bar is an `aria-live` region:
 * announcing every one turns a search into a stream of numbers nobody can listen
 * through. An axe audit sees a valid live region and says nothing, so this is the only
 * thing standing between a screen-reader user and that flood.
 *
 * `now` is injectable so the rule can be tested without waiting two seconds.
 */
export function createAnnouncementThrottle(now: () => number = Date.now): AnnouncementThrottle {
  let taken = ''
  let lastAt: number | null = null

  return {
    get text() {
      return taken
    },
    offer(text: string, final: boolean): boolean {
      if (text === taken) return false
      const at = now()
      if (!final && lastAt !== null && at - lastAt < LIVE_ANNOUNCE_INTERVAL_MS) return false
      taken = text
      lastAt = at
      return true
    },
  }
}
