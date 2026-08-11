/**
 * Brief mode's column-width store. These pin the parts that fail INVISIBLY: a
 * measurement that never arrives and nobody says so, a good answer thrown away
 * because a newer one was requested, or a provisional width so wide it eats the pane.
 *
 * The pre-fix shape of all four is one bug: a single transient IPC failure left the
 * widths empty forever, every column fell back to the pane width, and the cursor
 * highlight was suppressed until widths existed. Nothing logged.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

const ipc = vi.hoisted(() => ({
  getBriefColumnTextWidths: vi.fn(),
  ensureFontMetricsLoaded: vi.fn(),
  fillMissingFontMetrics: vi.fn(),
  getCurrentFontId: vi.fn(() => 'system-400-12'),
  warn: vi.fn(),
  debug: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({ getBriefColumnTextWidths: ipc.getBriefColumnTextWidths }))
vi.mock('$lib/font-metrics', () => ({
  ensureFontMetricsLoaded: ipc.ensureFontMetricsLoaded,
  fillMissingFontMetrics: ipc.fillMissingFontMetrics,
  getCurrentFontId: ipc.getCurrentFontId,
}))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: ipc.warn, debug: ipc.debug, info: vi.fn(), error: vi.fn() }),
}))

import type { BriefColumnsErrorKind } from '$lib/ipc/bindings'
import {
  createBriefColumnWidths,
  clampColumnWidths,
  provisionalColumnWidth,
  COLUMN_PADDING,
  MIN_COLUMN_WIDTH,
  DEFAULT_BRIEF_COLUMN_WIDTH,
  type BriefColumnWidthsDeps,
} from './brief-column-widths.svelte'

/** A successful IPC reply. */
function ok(widths: number[], missingCodePoints: number[] = []) {
  return { status: 'ok' as const, data: { widths, missingCodePoints } }
}

/** A typed IPC failure. */
function fail(kind: BriefColumnsErrorKind, message = 'boom') {
  return { status: 'error' as const, error: { kind, message } }
}

/** A promise plus the handles to settle it later, for interleaving two in-flight fetches. */
function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((r) => {
    resolve = r
  })
  return { promise, resolve }
}

interface Props {
  listingId: string
  itemsPerColumn: number
  hasParent: boolean
  includeHidden: boolean
}

function makeStore(overrides: Partial<Props> = {}, onFirstWidths?: () => void) {
  const props: Props = { listingId: 'l1', itemsPerColumn: 10, hasParent: false, includeHidden: false, ...overrides }
  const deps: BriefColumnWidthsDeps = {
    listingId: () => props.listingId,
    itemsPerColumn: () => props.itemsPerColumn,
    hasParent: () => props.hasParent,
    includeHidden: () => props.includeHidden,
    onFirstWidths,
  }
  return { store: createBriefColumnWidths(deps), props }
}

/** Lets queued promise callbacks run without advancing the fake clock. */
async function flush(): Promise<void> {
  for (let i = 0; i < 8; i++) await Promise.resolve()
}

describe('brief column widths', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    ipc.getBriefColumnTextWidths.mockReset()
    ipc.ensureFontMetricsLoaded.mockReset().mockResolvedValue(undefined)
    ipc.fillMissingFontMetrics.mockReset().mockResolvedValue(false)
    ipc.warn.mockReset()
    ipc.debug.mockReset()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  describe('rendered widths', () => {
    it('never renders an unmeasured column at the pane width', () => {
      // The whole reason the cursor highlight used to be suppressed: a `capPx`
      // fallback makes one column fill the pane, so the cursor stripe did too.
      const paneWidth = 1600
      expect(provisionalColumnWidth(paneWidth)).toBe(DEFAULT_BRIEF_COLUMN_WIDTH)
      expect(provisionalColumnWidth(paneWidth)).toBeLessThan(paneWidth)
    })

    it('shrinks the provisional width to the pane when the pane is the narrower of the two', () => {
      expect(provisionalColumnWidth(180)).toBe(180)
      // ...but never below the floor, or the name is pure ellipsis.
      expect(provisionalColumnWidth(20)).toBe(MIN_COLUMN_WIDTH)
    })

    it('adds chrome and clamps into [floor, cap]', () => {
      expect(clampColumnWidths([200], 1000)).toEqual([200 + COLUMN_PADDING])
      expect(clampColumnWidths([10], 1000)).toEqual([MIN_COLUMN_WIDTH])
      expect(clampColumnWidths([900], 400)).toEqual([400])
    })

    it('re-clamps to a new pane width with no IPC, because the raw widths do not depend on it', async () => {
      ipc.getBriefColumnTextWidths.mockResolvedValue(ok([500, 120]))
      const { store } = makeStore()
      store.request()
      await flush()
      expect(ipc.getBriefColumnTextWidths).toHaveBeenCalledTimes(1)

      // A pane resize is pure frontend math over the widths already in hand.
      expect(clampColumnWidths(store.rawWidths, 1000)).toEqual([500 + COLUMN_PADDING, 120 + COLUMN_PADDING])
      expect(clampColumnWidths(store.rawWidths, 300)).toEqual([300, 120 + COLUMN_PADDING])
      expect(ipc.getBriefColumnTextWidths).toHaveBeenCalledTimes(1)
    })
  })

  describe('readiness', () => {
    it('starts pending and only reports ready once an answer lands', async () => {
      const pending = deferred<ReturnType<typeof ok>>()
      ipc.getBriefColumnTextWidths.mockReturnValue(pending.promise)
      const { store } = makeStore()
      store.request()
      await flush()
      expect(store.status).toBe('pending')

      pending.resolve(ok([300]))
      await flush()
      expect(store.status).toBe('ready')
    })

    it('reports ready for an empty directory, which legitimately measures to no columns', async () => {
      ipc.getBriefColumnTextWidths.mockResolvedValue(ok([]))
      const { store } = makeStore()
      store.request()
      await flush()
      // Inferring readiness from `rawWidths.length` would call this pending forever.
      expect(store.rawWidths).toEqual([])
      expect(store.status).toBe('ready')
    })

    it('announces the first measurement so the host can snap the width transition, once per listing', async () => {
      const onFirstWidths = vi.fn()
      ipc.getBriefColumnTextWidths.mockResolvedValue(ok([300]))
      const { store } = makeStore({}, onFirstWidths)
      store.request()
      await flush()
      expect(onFirstWidths).toHaveBeenCalledTimes(1)

      store.request()
      await vi.advanceTimersByTimeAsync(60)
      await flush()
      expect(onFirstWidths).toHaveBeenCalledTimes(1)
    })
  })

  describe('staleness', () => {
    it('discards a response for a listing that has been navigated away from', async () => {
      const first = deferred<ReturnType<typeof ok>>()
      ipc.getBriefColumnTextWidths.mockReturnValueOnce(first.promise)
      const { store, props } = makeStore()
      store.request()
      await flush()

      props.listingId = 'l2'
      store.reset()
      first.resolve(ok([999]))
      await flush()

      expect(store.rawWidths).toEqual([])
      expect(store.status).toBe('pending')
    })

    it('keeps a successful response even though another refresh was already requested', async () => {
      // Pre-fix, every fetch bumped the generation on the way OUT, so simply asking
      // again invalidated the answer still in flight — and if the second ask failed,
      // the pane was left with nothing.
      const first = deferred<ReturnType<typeof ok>>()
      const second = deferred<ReturnType<typeof ok>>()
      ipc.getBriefColumnTextWidths.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise)

      const { store } = makeStore()
      store.request()
      await flush()
      store.request() // widths still empty, so this fires immediately too
      await flush()

      first.resolve(ok([310]))
      await flush()
      expect(store.rawWidths).toEqual([310])
      expect(store.status).toBe('ready')
    })

    it('does not let a late older response overwrite the newer one that already painted', async () => {
      const first = deferred<ReturnType<typeof ok>>()
      const second = deferred<ReturnType<typeof ok>>()
      ipc.getBriefColumnTextWidths.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise)

      const { store } = makeStore()
      store.request()
      await flush()
      store.request()
      await flush()

      second.resolve(ok([222]))
      await flush()
      first.resolve(ok([111]))
      await flush()

      expect(store.rawWidths).toEqual([222])
    })
  })

  describe('recovery', () => {
    it('recovers from a transient failure by retrying', async () => {
      ipc.getBriefColumnTextWidths.mockResolvedValueOnce(fail('timeout')).mockResolvedValueOnce(ok([340]))
      const { store } = makeStore()
      store.request()
      await flush()
      expect(store.rawWidths).toEqual([])

      await vi.advanceTimersByTimeAsync(200)
      await flush()
      expect(store.rawWidths).toEqual([340])
      expect(store.status).toBe('ready')
    })

    it('retries a listing that is not registered yet, then settles', async () => {
      ipc.getBriefColumnTextWidths.mockResolvedValueOnce(fail('listingNotFound')).mockResolvedValueOnce(ok([280]))
      const { store } = makeStore()
      store.request()
      await flush()
      await vi.advanceTimersByTimeAsync(200)
      await flush()
      expect(store.status).toBe('ready')
    })

    it('recovers from a thrown IPC, not only a typed error', async () => {
      ipc.getBriefColumnTextWidths.mockRejectedValueOnce(new Error('bridge gone')).mockResolvedValueOnce(ok([260]))
      const { store } = makeStore()
      store.request()
      await flush()
      await vi.advanceTimersByTimeAsync(200)
      await flush()
      expect(store.rawWidths).toEqual([260])
    })

    it('treats a reply that lied about its shape as a failure, not an unhandled rejection', async () => {
      // A stubbed-out IPC bridge answers `{ status: 'ok', data: undefined }`. Reading
      // through it must be classified like any other bad round trip, or the rejection
      // escapes the store and lands in whatever test or pane happens to be running.
      ipc.getBriefColumnTextWidths.mockResolvedValue({ status: 'ok', data: undefined })
      const { store } = makeStore()
      store.request()
      await flush()
      await vi.advanceTimersByTimeAsync(1000)
      await flush()

      expect(store.status).toBe('degraded')
      expect(ipc.warn).toHaveBeenCalled()
    })

    it('waits for the font metrics and asks again, without spending a retry', async () => {
      ipc.getBriefColumnTextWidths.mockResolvedValueOnce(fail('fontMetricsNotReady')).mockResolvedValueOnce(ok([400]))
      const { store } = makeStore()
      store.request()
      await flush()
      expect(ipc.ensureFontMetricsLoaded).toHaveBeenCalledTimes(1)
      expect(store.rawWidths).toEqual([400])
    })

    it('gives up after the bounded retries and says so out loud', async () => {
      ipc.getBriefColumnTextWidths.mockResolvedValue(fail('timeout', 'cache write-locked'))
      const { store } = makeStore()
      store.request()
      await flush()
      await vi.advanceTimersByTimeAsync(1000)
      await flush()

      expect(store.status).toBe('degraded')
      // Three attempts total: the original plus two retries.
      expect(ipc.getBriefColumnTextWidths).toHaveBeenCalledTimes(3)
      // The bug was invisible in production because nothing logged. Every attempt says something.
      expect(ipc.warn).toHaveBeenCalledTimes(3)
      const lastCall = ipc.warn.mock.calls.at(-1)
      expect(lastCall?.[1]).toMatchObject({ listingId: 'l1', kind: 'timeout', detail: 'cache write-locked' })
    })

    it('does not retry a caller bug, and still logs it', async () => {
      ipc.getBriefColumnTextWidths.mockResolvedValue(fail('invalidItemsPerColumn'))
      const { store } = makeStore()
      store.request()
      await flush()
      await vi.advanceTimersByTimeAsync(1000)
      await flush()

      expect(ipc.getBriefColumnTextWidths).toHaveBeenCalledTimes(1)
      expect(store.status).toBe('degraded')
      expect(ipc.warn).toHaveBeenCalledTimes(1)
    })
  })

  describe('cancellation', () => {
    it('cancels a pending retry when the listing changes', async () => {
      ipc.getBriefColumnTextWidths.mockResolvedValue(fail('timeout'))
      const { store, props } = makeStore()
      store.request()
      await flush()
      expect(ipc.getBriefColumnTextWidths).toHaveBeenCalledTimes(1)

      props.listingId = 'l2'
      store.reset()
      await vi.advanceTimersByTimeAsync(1000)
      await flush()
      expect(ipc.getBriefColumnTextWidths).toHaveBeenCalledTimes(1)
    })

    it('cancels a coalesced refresh and a pending retry on unmount', async () => {
      ipc.getBriefColumnTextWidths.mockResolvedValueOnce(ok([300])).mockResolvedValue(fail('timeout'))
      const { store } = makeStore()
      store.request()
      await flush()

      store.request() // coalesced
      store.destroy()
      await vi.advanceTimersByTimeAsync(1000)
      await flush()
      expect(ipc.getBriefColumnTextWidths).toHaveBeenCalledTimes(1)
    })
  })
})
