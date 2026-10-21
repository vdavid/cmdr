import type { DirectoryDiffBatch, ResortResult } from '../types'

export interface RowStateToken {
  listingId: string
  generation: number
  sequence: number
  workGeneration: number
  includeHidden: boolean
}

/** One owner for the pane's applied row space, reconfiguration queue, and async continuations. */
export function createPaneRowState(deps: {
  getListingId: () => string
  getLoading: () => boolean
  getOperationActive: () => boolean
  getIncludeHidden?: () => boolean
  onReconfigure?: () => void
}) {
  let sequence = 0
  let generation = 0
  let lifetime = 0
  let workGeneration = 0
  let initialized = false
  let disposed = false
  let pendingViews = 0
  let asyncWork = 0
  let appliedHidden: boolean | undefined
  let queue = Promise.resolve()
  const buffered = new Map<number, DirectoryDiffBatch>()
  let applyBatch: (batch: DirectoryDiffBatch) => undefined | (() => void) = () => {}
  /** Callers of `whenReady` still waiting, each answered once with `true` (settled) or `false`. */
  const readyWaiters = new Set<(ready: boolean) => void>()

  function isReady(): boolean {
    return (
      initialized &&
      !disposed &&
      !deps.getLoading() &&
      pendingViews === 0 &&
      asyncWork === 0 &&
      deps.getListingId() !== '' &&
      !deps.getOperationActive() &&
      buffered.size === 0 &&
      (appliedHidden === undefined || !deps.getIncludeHidden || appliedHidden === deps.getIncludeHidden())
    )
  }

  /** Answers every `whenReady` waiter once the rows are settled, or `false` once the pane is gone. */
  function notifyReadiness(): void {
    if (readyWaiters.size === 0) return
    if (!disposed && !isReady()) return
    const answer = !disposed
    for (const answerWaiter of [...readyWaiters]) answerWaiter(answer)
  }

  function drain(): void {
    if (!initialized || disposed || deps.getLoading()) return
    for (const [start, batch] of buffered) if (batch.sequence <= sequence) buffered.delete(start)
    let effects: undefined | (() => void) = undefined
    for (;;) {
      const next = buffered.get(sequence)
      if (!next) break
      buffered.delete(sequence)
      effects = applyBatch(next)
      sequence = next.sequence
    }
    // Preserve each coordinate transformation without fetching the UI once per file.
    if (typeof effects === 'function') effects()
    notifyReadiness()
  }

  function capture(): RowStateToken {
    return {
      listingId: deps.getListingId(),
      generation,
      sequence,
      workGeneration,
      includeHidden: appliedHidden ?? deps.getIncludeHidden?.() ?? true,
    }
  }

  function matches(token: RowStateToken): boolean {
    return (
      !disposed &&
      token.listingId === deps.getListingId() &&
      token.generation === generation &&
      token.sequence === sequence &&
      token.workGeneration === workGeneration
    )
  }

  return {
    getSequence: () => sequence,
    getGeneration: () => generation,
    isReady,
    /**
     * Resolves `true` the moment the rows are settled (at once if they already are), or
     * `false` after `timeoutMs` or once the pane is disposed. Event-driven: every transition
     * that can settle the rows calls `notifyReadiness`, and the pane calls `recheck` when one
     * of the outside conditions `isReady` reads (loading, an operation's snapshot) changes.
     */
    whenReady(timeoutMs: number): Promise<boolean> {
      if (isReady()) return Promise.resolve(true)
      if (disposed || timeoutMs <= 0) return Promise.resolve(false)
      return new Promise<boolean>((resolve) => {
        const answer = (ready: boolean) => {
          readyWaiters.delete(answer)
          clearTimeout(timer)
          resolve(ready)
        }
        const timer = setTimeout(() => {
          answer(false)
        }, timeoutMs)
        readyWaiters.add(answer)
      })
    },
    /** An outside condition `isReady` reads may have changed; answer any waiter it settles. */
    recheck: notifyReadiness,
    capture,
    matches,
    invalidateWork: () => {
      workGeneration++
    },
    beginAsyncWork() {
      asyncWork++
      let ended = false
      return () => {
        if (!ended) {
          ended = true
          asyncWork--
          notifyReadiness()
        }
      }
    },
    reset() {
      deps.onReconfigure?.()
      generation++
      lifetime++
      workGeneration++
      sequence = 0
      appliedHidden = deps.getIncludeHidden?.()
      initialized = false
      buffered.clear()
    },
    setSequence(value: number) {
      sequence = value
    },
    initialize() {
      initialized = true
      if (pendingViews === 0) drain()
      notifyReadiness()
    },
    setBatchApplier(applier: (batch: DirectoryDiffBatch) => undefined | (() => void)) {
      applyBatch = applier
    },
    receive(batches: DirectoryDiffBatch[]) {
      for (const batch of batches) if (batch.sequence > sequence) buffered.set(batch.fromSequence, batch)
      if (pendingViews === 0) drain()
    },
    /** Gate immediately, serialize requests, recapture rows after every typed changed refusal. */
    changeView(input: {
      request: (token: RowStateToken) => Promise<ResortResult>
      install: (result: ResortResult) => undefined | (() => void)
      isCurrent?: () => boolean
      includeHidden?: boolean
    }): Promise<boolean> {
      deps.onReconfigure?.()
      generation++
      workGeneration++
      pendingViews++
      const myLifetime = lifetime
      const listingId = deps.getListingId()
      const current = () =>
        !disposed && lifetime === myLifetime && deps.getListingId() === listingId && (input.isCurrent?.() ?? true)
      const run = queue
        .then(async () => {
          if (!current() || !initialized || deps.getLoading()) return false
          for (let attempt = 0; attempt < 5; attempt++) {
            try {
              const result = await input.request(capture())
              if (!current()) return false
              const effects = input.install(result)
              sequence = result.sequence
              if (input.includeHidden !== undefined) appliedHidden = input.includeHidden
              if (typeof effects === 'function') effects()
              drain()
              return true
            } catch (error) {
              if (!current()) return false
              const refusal = error as { type?: string; failure?: { type?: string } }
              if ((refusal.failure?.type ?? refusal.type) !== 'changed') throw error
              drain()
              if (buffered.size > 0 || attempt < 4) await new Promise((resolve) => setTimeout(resolve, 100))
            }
          }
          return false
        })
        .finally(() => {
          pendingViews--
          if (pendingViews === 0) drain()
          notifyReadiness()
        })
      queue = run.then(
        () => {},
        () => {},
      )
      return run
    },
    dispose() {
      disposed = true
      generation++
      workGeneration++
      buffered.clear()
      notifyReadiness()
    },
  }
}

export type PaneRowState = ReturnType<typeof createPaneRowState>
