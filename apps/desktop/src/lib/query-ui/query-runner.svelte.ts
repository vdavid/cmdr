/**
 * QueryDialog's run controller: everything between "the user wants results" and "state
 * holds them".
 *
 * It owns the auto-apply debounce and its gates, the IME guard, the AI translate
 * round-trip, the spinner's lifetime, and the `hasSearched` / `highlightedFields` flags the
 * dialog renders off. QueryDialog keeps the ownership contracts (see
 * `query-dialog-config.ts`); this module is where they're enforced in code.
 *
 * `getConfig` is a getter, not a value: the consumer's config is a `$derived` object that's
 * rebuilt whenever anything reactive under it changes, so a captured reference would freeze
 * gates like `isIndexReady` at their mount-time values.
 */

import { tick } from 'svelte'
import { SvelteSet } from 'svelte/reactivity'
import { showAiTranslateErrorToast } from '$lib/ai/translate-error-toast'
import { tString } from '$lib/intl/messages.svelte'
import type { SearchResultEntry } from '$lib/tauri-commands'
import { addToast } from '$lib/ui/toast/toast-store.svelte'
import type { QueryDialogConfig } from './query-dialog-config'
import { SEARCH_AUTO_APPLY_DEBOUNCE_MS, type QueryFilterState, type SearchMode } from './query-filter-state.svelte'
import type { LiveRunView, QueryStreamEnd, QueryStreamProgress, QueryStreamSource } from './query-stream'

/** How long the fields the AI touched stay flashed, so the user sees what changed. */
export const AI_HIGHLIGHT_FLASH_MS = 1500

/** Why a run is happening, for the two rules that care. */
export interface RunOptions {
  /**
   * True only from `runAiSearch`, after the translation has populated state: that
   * branch keeps `lastAiPrompt` / `lastAiCaveat`, every other one clears them.
   */
  fromAiTranslation?: boolean
  /**
   * True when the debounce fired this run rather than the user. An auto-applied run
   * NEVER streams: streaming means walking ground the index doesn't cover, and a user
   * typing six characters would start and abandon five multi-minute walks
   * (`docs/specs/unindexed-search-plan.md` Decision 7).
   */
  fromAutoApply?: boolean
}

export interface QueryRunnerDeps<E> {
  /** Reads the consumer's live config. Called fresh on every access; never cached. */
  getConfig: () => QueryDialogConfig<E>
  /** Live mirror of the `search.autoApply` setting. */
  isAutoApplyEnabled: () => boolean
  /** Brings the cursor row into view once results have rendered. */
  scrollCursorIntoView: () => void
}

/**
 * Every member is a function PROPERTY, not a method: the dialog hands them straight to
 * child components as callbacks, and method signatures trip `@typescript-eslint/unbound-method`.
 */
export interface QueryRunner {
  /** Whether a run's results are the current content (vs. the empty state). */
  readonly hasSearched: boolean
  /** Reset hook for ⌘N and the prefill path, which both go back to the empty state. */
  setHasSearched: (value: boolean) => void
  /** Filter-chip names the AI just touched, flashed for `AI_HIGHLIGHT_FLASH_MS`. */
  readonly highlightedFields: SvelteSet<string>
  /**
   * The live run's phase, counters, and end state, or `null` when the last run wasn't
   * a streaming one. Survives the run's end so the list can stay labelled.
   */
  readonly live: LiveRunView | null
  /**
   * Stops a running live search and the work behind it. Returns whether THIS ask is the
   * one that stopped it, which is what makes Escape a two-step (first stops, then
   * closes). Once per run: a run whose terminal event never arrives would otherwise
   * answer `true` forever and leave the dialog un-closable by keyboard.
   */
  cancelLive: () => boolean
  /**
   * Adopts a run the consumer kept alive across this dialog's last close, and reports
   * whether there was one.
   *
   * Called once on mount, BEFORE the reopen-with-results decision: a fresh run would
   * supersede the live one and strand whatever it was still feeding, so adopting has
   * to win.
   */
  resumeLive: () => boolean
  /** Debounced auto-apply, behind the AI / setting / IME gates. */
  scheduleSearch: () => void
  executeQuery: (options?: RunOptions) => Promise<void>
  runAiSearch: (prompt: string) => Promise<void>
  /** Runs the active mode's query. The `⏎` path, which has no disabled-inputs guard. */
  run: () => void
  /** Same, from a button, which does honor `inputsDisabled`. */
  runFromButton: () => void
  /** Runs an AI translation over whatever the bar holds. */
  runAiFromQuery: () => void
  handleCompositionStart: () => void
  handleCompositionEnd: () => void
  /** Cancels a pending auto-apply so an unmounted dialog stays quiet. */
  dispose: () => void
}

/**
 * Whether the current state has anything runnable: a non-empty query OR an active filter
 * (size ≠ any, date ≠ any, or type ≠ both). The single source of truth for "is there a
 * session worth running?", shared by `executeQuery`'s guard, `scheduleSearch`'s gate chain,
 * the dialog's `runOnMount` effect, and its reopen re-run gate. Type counts: a
 * "Folders"-only Selection run is a valid filter-only query.
 *
 * An empty pattern WITH an active filter is deliberately RUNNABLE: `≥ 1 MB` with no glob
 * selects every file ≥ 1 MB (Selection's `hasActiveFilter()` + `buildMatchQuery` encode the
 * same rule; see `lib/selection-dialog/CLAUDE.md`). Only "nothing at all" is refused.
 */
export function hasRunnableQuery(state: QueryFilterState): boolean {
  return (
    state.getQuery().trim() !== '' ||
    state.getSizeFilter() !== 'any' ||
    state.getDateFilter() !== 'any' ||
    state.getTypeFilter() !== 'both'
  )
}

/**
 * "Press Enter to search" hint visibility:
 *   1. Inputs disabled → hide.
 *   2. Trimmed query is empty → hide.
 *   3. Query unchanged since last run → hide.
 *   4. AI mode (never auto-applies) OR setting off → show.
 */
export function shouldShowRunHint(input: {
  inputsDisabled: boolean
  query: string
  lastRunQuery: string | null
  mode: SearchMode
  autoApplyEnabled: boolean
}): boolean {
  if (input.inputsDisabled) return false
  const trimmed = input.query.trim()
  if (!trimmed) return false
  if (trimmed === (input.lastRunQuery ?? '').trim()) return false
  return input.mode === 'ai' || !input.autoApplyEnabled
}

export function createQueryRunner<E>(deps: QueryRunnerDeps<E>): QueryRunner {
  let hasSearched = $state(deps.getConfig().state.getLastRunQuery() !== null)
  /**
   * One instance for the controller's whole life, mutated in place: `SvelteSet` is reactive
   * per key, so readers track the keys they ask about. Swapping in a fresh set instead would
   * leave every reader subscribed to the old one.
   */
  const highlightedFields = new SvelteSet<string>()
  let debounceTimer: ReturnType<typeof setTimeout> | undefined
  /**
   * IME composition flag. While true, `scheduleSearch` is a no-op so we don't fire
   * mid-character on Chinese/Japanese/Korean input. On `compositionend` the bar calls
   * `scheduleSearch` once so the user gets exactly one fire post-composition.
   */
  let imeComposing = false

  /**
   * The streaming run in flight, and what the dialog renders off it. Kept after the
   * run ends (with `running: false`) so the list stays labelled as short when it is.
   */
  let live = $state<LiveRunView | null>(null)
  /**
   * The run every update is measured against. An update naming anything else belongs
   * to a query the user has moved on from and is DROPPED — superseding a run doesn't
   * cancel the work behind it, so its batches keep arriving.
   */
  let liveRunId: string | null = null
  /** Teardown for the current run's subscription. */
  let stopLiveUpdates: (() => void) | null = null
  /**
   * Whether the current run has already been asked to stop.
   *
   * `running` only clears on the run's own terminal event, so a run that never sends
   * one would answer "there was one to stop" forever, and Escape's two-step, which
   * closes only once there is nothing left to stop, could never reach the close.
   */
  let stopAsked = false

  /** The backend's own message when there is one; a generic fallback otherwise. */
  function describeRunFailure(err: unknown): string {
    const message = err instanceof Error ? err.message : typeof err === 'string' ? err : ''
    return message.trim() || tString('queryUi.dialog.runQueryUnknownReason')
  }

  /** Warns with the run's own reason, the one place every run failure surfaces. */
  function toastRunFailure(message: string): void {
    addToast(tString('queryUi.dialog.runQueryToast', { message }), { level: 'warn', dismissal: 'transient' })
  }

  /**
   * Stops listening to the current run and forgets its id. NOT a cancellation: the
   * walk behind a superseded run carries on, and the ground it covers reaches the
   * index for the very next query (Decision 11).
   */
  function dropLiveSubscription(): void {
    stopLiveUpdates?.()
    stopLiveUpdates = null
    liveRunId = null
    stopAsked = false
  }

  /**
   * Writes `entries` while keeping the cursor on the row it was on, found by PATH.
   * Appending doesn't move an index, but the completion re-rank does, and losing the
   * cursor mid-list is how a growing list becomes unreadable.
   */
  function setResultsHoldingCursor(state: QueryFilterState, entries: SearchResultEntry[]): void {
    // `.at()` rather than `[i]`: the index type claims a row is always there, and an
    // empty list says otherwise. The cursor is never negative (it starts at 0 and
    // resets to 0), so `.at()`'s from-the-end reading can't apply.
    const cursorPath = state.getResults().at(state.getCursorIndex())?.path
    state.setResults(entries)
    if (cursorPath === undefined) return
    const next = entries.findIndex((entry) => entry.path === cursorPath)
    state.setCursorIndex(next === -1 ? 0 : next)
  }

  function applyLiveProgress(config: QueryDialogConfig<E>, update: QueryStreamProgress): void {
    live = {
      phase: update.phase,
      matchCount: update.matchCount,
      dirsFound: update.dirsFound,
      currentPath: update.currentPath,
      capped: update.capped,
      running: true,
      incomplete: false,
      // ❗ Carry the stamp forward while the phase HOLDS; re-stamp only on a change.
      // The waiting phase re-announces every 200 ms, so stamping per update would peg
      // the elapsed reading at zero — which is exactly the "is it alive?" question it
      // exists to answer.
      phaseSince: live !== null && live.phase === update.phase ? live.phaseSince : Date.now(),
    }
    config.state.setTotalCount(update.matchCount)
    if (update.entries.length === 0) return
    const hadRows = config.state.getResults().length > 0
    setResultsHoldingCursor(config.state, [...config.state.getResults(), ...update.entries])
    // D8: the first rows are what hands ⏎ to "go-to-file". Only the first batch, or
    // every later one would overwrite the 'cursor-moved' the user just caused — which
    // is also what tells us to leave the order alone on completion.
    if (!hadRows) config.state.setLastDialogEvent('results-arrived')
  }

  function finishLiveRun(config: QueryDialogConfig<E>, source: QueryStreamSource, end: QueryStreamEnd): void {
    dropLiveSubscription()
    live = {
      // The phase the RUN last reported, never a guess: a run that ended without
      // ever walking must not sign off as walking. The fallback is the phase every
      // run starts in, for the case where a terminal update arrives with no
      // progress update before it.
      phase: live?.phase ?? 'resolvingCoverage',
      matchCount: end.matchCount,
      dirsFound: live?.dirsFound ?? 0,
      currentPath: null,
      capped: end.capped,
      running: false,
      incomplete: end.incomplete,
      phaseSince: live?.phaseSince ?? Date.now(),
    }
    config.state.setTotalCount(end.matchCount)
    config.state.setIsSearching(false)
    // Arrival order is what a growing list has to be; one order for the finished list
    // is what it deserves. Skipped when the index answered everything (its rows came
    // ranked, and re-ranking would throw that away) and skipped once the user has
    // moved the cursor, because reordering under someone reading is worse than
    // arrival order (Decision 8).
    if (!end.walked || !source.rankOnCompletion) return
    if (config.state.getLastDialogEvent() === 'cursor-moved') return
    setResultsHoldingCursor(config.state, source.rankOnCompletion(config.state.getResults()))
  }

  function failLiveRun(config: QueryDialogConfig<E>, message: string): void {
    dropLiveSubscription()
    live = null
    config.state.setIsSearching(false)
    toastRunFailure(message)
  }

  /**
   * Picks up a run that outlived the last dialog, rows and all.
   *
   * The consumer has been listening the whole time (Search's walk keeps feeding the
   * pane it was opened into), so this only adds this dialog as a second reader: the
   * run id, where it had got to, and the rows found while nobody here was looking.
   * Without those rows the list and the count would disagree by however many arrived
   * in between, which is the silent kind of wrong.
   */
  function resumeLiveRun(config: QueryDialogConfig<E>, source: QueryStreamSource): boolean {
    // Filled the moment `resume` returns, and read only from callbacks that fire
    // later — the same generation guard a started run uses, against the same field.
    let adoptedRunId: string | null = null
    const mine = (): boolean => adoptedRunId !== null && liveRunId === adoptedRunId

    const resumed = source.resume?.({
      onProgress: (update) => {
        if (mine()) applyLiveProgress(config, update)
      },
      onEnd: (end) => {
        if (mine()) finishLiveRun(config, source, end)
      },
      onFailed: (message) => {
        if (mine()) failLiveRun(config, message)
      },
    })
    if (!resumed) return false

    dropLiveSubscription()
    adoptedRunId = resumed.runId
    liveRunId = resumed.runId
    stopLiveUpdates = resumed.stop
    live = resumed.view
    hasSearched = true
    config.state.setTotalCount(resumed.view.matchCount)
    config.state.setIsSearching(resumed.view.running)
    if (resumed.missedEntries.length > 0) {
      setResultsHoldingCursor(config.state, [...config.state.getResults(), ...resumed.missedEntries])
    }
    return true
  }

  /**
   * Runs the query as a stream: rows land as they're found and the run says which of
   * its three phases it's in. The run id is minted HERE, so no update can arrive
   * against an id this side hasn't seen.
   */
  async function startLiveRun(
    config: QueryDialogConfig<E>,
    source: QueryStreamSource,
    fromAiTranslation: boolean,
  ): Promise<void> {
    dropLiveSubscription()
    const runId = crypto.randomUUID()
    liveRunId = runId
    const mine = (): boolean => liveRunId === runId

    // The previous run's rows answered a different question, and this one appends to
    // what's on screen, so they go before the first batch rather than after it.
    config.state.setResults([])
    config.state.setTotalCount(0)
    config.state.setCursorIndex(0)
    config.state.setLastRunQuery(config.state.getQuery())
    if (!fromAiTranslation) {
      config.state.setLastAiPrompt(null)
      config.state.setLastAiCaveat(null)
    }
    live = {
      phase: 'resolvingCoverage',
      matchCount: 0,
      dirsFound: 0,
      currentPath: null,
      capped: false,
      running: true,
      incomplete: false,
      phaseSince: Date.now(),
    }
    config.state.setIsSearching(true)

    try {
      const stop = await source.start(runId, {
        onProgress: (update) => {
          if (mine()) applyLiveProgress(config, update)
        },
        onEnd: (end) => {
          if (mine()) finishLiveRun(config, source, end)
        },
        onFailed: (message) => {
          if (mine()) failLiveRun(config, message)
        },
      })
      // The run was superseded (or stopped) while `start` was in flight: tear its
      // subscription down rather than leaving it feeding a run nobody reads.
      if (!mine()) {
        stop()
        return
      }
      stopLiveUpdates = stop
    } catch (err) {
      if (!mine()) return
      failLiveRun(config, describeRunFailure(err))
    }
  }

  /**
   * Back to "nothing has been asked yet": drops the previous run's rows so they can't sit
   * there implying they still match, and puts the results area back on the empty state.
   * Called when the user empties the query with no filter left standing.
   */
  function resetToEmptyState(): void {
    // A live run has no promise whose `finally` clears the spinner: dropping its
    // subscription is the last anyone hears of it. So emptying the bar mid-walk has to
    // clear the flag here, or the dialog sits on "Searching…" with nothing coming.
    const wasStreaming = live !== null
    dropLiveSubscription()
    live = null
    const state = deps.getConfig().state
    if (wasStreaming) state.setIsSearching(false)
    state.setResults([])
    state.setTotalCount(0)
    state.setCursorIndex(0)
    state.setLastRunQuery(null)
    state.setLastAiPrompt(null)
    state.setLastAiCaveat(null)
    hasSearched = false
  }

  /**
   * Runs the consumer's `runQuery` callback and writes the result into state.
   * `fromAiTranslation` is true only when invoked from `runAiSearch` after the translation
   * has populated state; in that branch we keep `lastAiPrompt` / `lastAiCaveat` intact
   * (they were just set). Every other branch clears them so the strip doesn't outlive its
   * AI run.
   *
   * A consumer with a `streamingSource` takes it for every run the USER asked for, and
   * the one-shot `runQuery` for the auto-applied ones (Decision 7).
   */
  async function executeQuery(options: RunOptions = {}): Promise<void> {
    const { fromAiTranslation = false, fromAutoApply = false } = options
    const config = deps.getConfig()
    if (debounceTimer) clearTimeout(debounceTimer)
    // Nothing to run: an empty bar with every filter at its default isn't a query, and the
    // backend refuses it ("Query too broad"), which surfaced as a warning toast the moment
    // the user cleared the field. This is the choke point every path goes through
    // (auto-apply, the ⏎ button, bare Enter, the runOnMount prefill), so the rule holds for
    // all of them: fall back to the empty state instead of asking for everything.
    if (!hasRunnableQuery(config.state)) {
      resetToEmptyState()
      config.state.setIsSearching(false)
      return
    }
    hasSearched = true
    if (!config.isIndexReady) {
      // Bail before running, but clear any spinner `runAiSearch` turned on for the translate
      // round-trip (it sets `isSearching` before calling us). Without this the spinner sticks.
      config.state.setIsSearching(false)
      return
    }

    if (config.streamingSource && !fromAutoApply) {
      await startLiveRun(config, config.streamingSource, fromAiTranslation)
      return
    }
    // A one-shot answer replaces a streaming one: stop reading the run it supersedes
    // (without stopping its work) and drop the live labels with it.
    dropLiveSubscription()
    live = null

    config.state.setIsSearching(true)
    try {
      const result = await config.runQuery()
      config.state.setResults(result.entries)
      config.state.setTotalCount(result.totalCount)
      config.state.setCursorIndex(0)
      // D8: results just landed. ⏎ now owns "go-to-file" (when results > 0).
      config.state.setLastDialogEvent('results-arrived')
      config.state.setLastRunQuery(config.state.getQuery())
      if (!fromAiTranslation) {
        // Non-AI search completed cleanly. The AI strip belongs to the previous AI run, so
        // drop it. AI runs go through `runAiSearch`, which sets the strip and then calls us
        // with `fromAiTranslation = true`.
        config.state.setLastAiPrompt(null)
        config.state.setLastAiCaveat(null)
      }
    } catch (err) {
      // Surface WHY nothing came back. The backend refuses some runs with an actionable
      // message ("Query too broad. Add a filename pattern, size, date, or type filter");
      // swallowing it left the user staring at an empty list that reads as "nothing
      // matched". No typed variant crosses this IPC boundary, so we pass the message
      // through verbatim instead of classifying it by its text.
      toastRunFailure(describeRunFailure(err))
    } finally {
      config.state.setIsSearching(false)
    }
  }

  /**
   * Runs an AI translation for `prompt`, then executes the query. The consumer's
   * `translateAi` owns applying every AI-returned filter (size / date / scope / AI pattern +
   * label / etc); the runner captures the prompt, flashes any highlighted fields, sets the
   * caveat, and runs the query.
   *
   * The spinner covers the WHOLE round-trip: we flip `isSearching` on before the cloud
   * translate (the slow part: seconds) and leave it on through `executeQuery`, which clears
   * it in its own `finally`. The early-return paths (empty prompt, translate error, empty
   * result) reset it themselves so it can't stick on.
   */
  async function runAiSearch(prompt: string): Promise<void> {
    const config = deps.getConfig()
    const trimmed = prompt.trim()
    if (!trimmed) return
    if (!config.translateAi) return

    // Capture the prompt BEFORE calling the IPC so the user sees what they asked even if
    // the IPC fails. The AI bar in AI mode keeps the prompt as the bar's contents (the
    // pattern lives separately via the consumer's extras).
    config.state.setLastAiPrompt(trimmed)
    config.state.setLastAiCaveat(null)
    // Show the spinner for the slow cloud translate, not just the post-translate query.
    hasSearched = true
    config.state.setIsSearching(true)

    let result: Awaited<ReturnType<NonNullable<typeof config.translateAi>>>
    try {
      result = await config.translateAi(trimmed)
    } catch (err) {
      // Surface WHY the translation failed (quota, key, timeout, empty answer, …) as a
      // specific toast instead of a silent no-op. Both Search and Selection route here, so
      // the error UX lives in one place. The consumer's `translateAi` lets the typed error
      // throw; we map its `kind` to copy. A non-translation error (shouldn't happen) falls
      // through to a generic toast.
      config.state.setIsSearching(false)
      if (!showAiTranslateErrorToast(err)) {
        addToast(tString('queryUi.dialog.aiTranslateFailedToast'), { level: 'warn', dismissal: 'transient' })
      }
      return
    }
    if (!result) {
      config.state.setIsSearching(false)
      return
    }

    // Flash the changed fields so the user sees what the AI touched.
    if (result.highlightedFields && result.highlightedFields.length > 0) {
      highlightedFields.clear()
      for (const field of result.highlightedFields) highlightedFields.add(field)
      setTimeout(() => {
        highlightedFields.clear()
      }, AI_HIGHLIGHT_FLASH_MS)
    }
    config.state.setLastAiCaveat(result.caveat)

    // `executeQuery` sets `isSearching` true again (idempotent) and clears it in `finally`.
    await executeQuery({ fromAiTranslation: true })
    await tick()
    deps.scrollCursorIntoView()
  }

  function runAiFromQuery(): void {
    const config = deps.getConfig()
    if (!config.aiEnabled) return
    const trimmed = config.state.getQuery().trim()
    if (trimmed) void runAiSearch(trimmed)
  }

  function run(): void {
    if (deps.getConfig().state.getMode() === 'ai') {
      runAiFromQuery()
    } else {
      void executeQuery()
    }
  }

  /**
   * Schedules a debounced auto-apply. Four early-return gates:
   *   0. Nothing to run (empty bar, every filter at its default) — and that also drops the
   *      previous run's rows. Checked FIRST, before the mode / setting gates, so emptying
   *      the bar clears the list no matter how runs are triggered.
   *   1. AI mode never auto-applies (AI calls cost money; user must opt in).
   *   2. `search.autoApply === false`: user runs every query explicitly.
   *   3. IME composition is in progress.
   */
  function scheduleSearch(): void {
    if (debounceTimer) clearTimeout(debounceTimer)
    if (!hasRunnableQuery(deps.getConfig().state)) {
      resetToEmptyState()
      return
    }
    if (deps.getConfig().state.getMode() === 'ai') return
    if (!deps.isAutoApplyEnabled()) return
    if (imeComposing) return
    debounceTimer = setTimeout(() => {
      void executeQuery({ fromAutoApply: true })
    }, SEARCH_AUTO_APPLY_DEBOUNCE_MS)
  }

  return {
    get hasSearched() {
      return hasSearched
    },
    setHasSearched: (value: boolean) => {
      hasSearched = value
    },
    get highlightedFields() {
      return highlightedFields
    },
    get live() {
      return live
    },
    cancelLive: () => {
      if (liveRunId === null || live === null || !live.running || stopAsked) return false
      stopAsked = true
      deps.getConfig().streamingSource?.cancel(liveRunId)
      // The end state is the run's own word (the terminal update relabels it), so
      // nothing flips here. What this promises the caller is only "this ask is the
      // one that stopped it": a second ask answers `false`, so Escape closes rather
      // than sitting on a run whose terminal event may never come.
      return true
    },
    resumeLive: () => {
      const config = deps.getConfig()
      if (!config.streamingSource) return false
      return resumeLiveRun(config, config.streamingSource)
    },
    scheduleSearch,
    executeQuery,
    runAiSearch,
    run,
    runFromButton: () => {
      if (deps.getConfig().inputsDisabled) return
      run()
    },
    runAiFromQuery,
    handleCompositionStart: () => {
      imeComposing = true
      if (debounceTimer) clearTimeout(debounceTimer)
    },
    handleCompositionEnd: () => {
      imeComposing = false
      scheduleSearch()
    },
    dispose: () => {
      if (debounceTimer) clearTimeout(debounceTimer)
      // Stop LISTENING, don't cancel: the consumer's own teardown decides whether the
      // work outlives the dialog (Search's `releaseSearchIndex` stops every live run).
      dropLiveSubscription()
    },
  }
}
