// Which measuring path runs, and what happens when the worker path fails.
//
// The module keeps the worker in module-level state, so each case re-imports it
// via `vi.resetModules()` rather than reaching for a production reset seam.

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { measureCodePoints } from './measure'
import type { MeasureRequest, MeasureResponse } from './measure-worker'

const spec = { fontFamily: 'Menlo', fontWeight: 400, fontSize: 12 }

const { logger } = vi.hoisted(() => ({
  logger: { debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() },
}))

vi.mock('$lib/logging/logger', () => ({ getAppLogger: () => logger }))

/** How the fake worker should behave for a case. */
type WorkerBehavior = 'answer' | 'error-response' | 'silent' | 'throw-on-construct'

const constructed: FakeWorker[] = []

class FakeWorker {
  onmessage: ((event: MessageEvent<MeasureResponse>) => void) | null = null
  onerror: ((event: { message: string }) => void) | null = null
  terminated = false
  static behavior: WorkerBehavior = 'answer'

  constructor() {
    if (FakeWorker.behavior === 'throw-on-construct') {
      throw new Error('Worker blocked')
    }
    constructed.push(this)
  }

  postMessage(request: MeasureRequest) {
    if (FakeWorker.behavior === 'silent') return
    const response: MeasureResponse =
      FakeWorker.behavior === 'error-response'
        ? { requestId: request.requestId, error: 'measurement blew up' }
        : {
            requestId: request.requestId,
            codePoints: request.codePoints,
            widths: new Float32Array(request.codePoints.length).fill(9),
          }
    queueMicrotask(() => this.onmessage?.(new MessageEvent('message', { data: response })))
  }

  terminate() {
    this.terminated = true
  }
}

/** Makes the main-thread fallback viable under happy-dom. */
function installCanvas() {
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockImplementation(((kind: string) =>
    kind === '2d'
      ? { font: '', measureText: (text: string) => ({ width: text.length }) }
      : null) as typeof HTMLCanvasElement.prototype.getContext)
}

/** Which of the two worker prerequisites the fake WebView offers. Both, unless a case says otherwise. */
interface Platform {
  worker?: boolean
  offscreenCanvas?: boolean
}

async function loadClient(behavior: WorkerBehavior, platform: Platform = {}) {
  FakeWorker.behavior = behavior
  constructed.length = 0
  vi.stubGlobal('Worker', platform.worker === false ? undefined : FakeWorker)
  vi.stubGlobal('OffscreenCanvas', platform.offscreenCanvas === false ? undefined : vi.fn())
  vi.resetModules()
  return import('./worker-client')
}

beforeEach(() => {
  installCanvas()
})

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
  vi.clearAllMocks()
})

describe('measureOffMainThread', () => {
  it('measures in the worker when one can start', async () => {
    const { measureOffMainThread } = await loadClient('answer')

    const result = await measureOffMainThread(spec, new Uint32Array([0x41, 0x42]))

    expect(result.via).toBe('worker')
    expect(Array.from(result.widths)).toEqual([9, 9])
  })

  it('reuses one worker across jobs instead of spawning per call', async () => {
    const { measureOffMainThread } = await loadClient('answer')

    await measureOffMainThread(spec, new Uint32Array([0x41]))
    await measureOffMainThread(spec, new Uint32Array([0x42]))

    expect(constructed).toHaveLength(1)
  })

  it('keeps concurrent jobs apart by request ID', async () => {
    const { measureOffMainThread } = await loadClient('answer')

    const [first, second] = await Promise.all([
      measureOffMainThread(spec, new Uint32Array([0x41])),
      measureOffMainThread(spec, new Uint32Array([0x41, 0x42, 0x43])),
    ])

    expect(first.widths.length).toBe(1)
    expect(second.widths.length).toBe(3)
  })

  it('falls back to the main thread when a worker cannot be constructed', async () => {
    // A WebView with no `Worker` must still get correct widths, just slower.
    // Compared against the shared core: the fallback is a different scheduler,
    // not a different measurement.
    const codePoints = new Uint32Array([0x41, 0x1f600, 0x2500])
    const stubCtx = { font: '', measureText: (text: string) => ({ width: text.length }) }
    const expected = measureCodePoints(stubCtx, spec, codePoints)
    const { measureOffMainThread } = await loadClient('throw-on-construct')

    const result = await measureOffMainThread(spec, codePoints)

    expect(result.via).toBe('main-thread')
    expect(Array.from(result.widths)).toEqual(Array.from(expected))
  })

  it('falls back to the main thread when the worker reports a failure', async () => {
    const { measureOffMainThread } = await loadClient('error-response')

    const result = await measureOffMainThread(spec, new Uint32Array([0x41]))

    expect(result.via).toBe('main-thread')
    expect(Array.from(result.widths)).toEqual([1])
  })

  it('retires a failed worker so later jobs skip straight to the fallback', async () => {
    const { measureOffMainThread } = await loadClient('error-response')

    await measureOffMainThread(spec, new Uint32Array([0x41]))
    const second = await measureOffMainThread(spec, new Uint32Array([0x41]))

    expect(second.via).toBe('main-thread')
    expect(constructed).toHaveLength(1)
    expect(constructed[0].terminated).toBe(true)
  })

  it('falls back rather than hanging when the worker never answers', async () => {
    vi.useFakeTimers()
    try {
      const { measureOffMainThread } = await loadClient('silent')

      const pending = measureOffMainThread(spec, new Uint32Array([0x41]))
      await vi.advanceTimersByTimeAsync(60_000)
      const result = await pending

      expect(result.via).toBe('main-thread')
    } finally {
      vi.useRealTimers()
    }
  })

  it('surfaces a failure when neither path can measure', async () => {
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(null)
    const { measureOffMainThread } = await loadClient('throw-on-construct')

    await expect(measureOffMainThread(spec, new Uint32Array([0x41]))).rejects.toThrow('context unavailable')
  })

  // WebKitGTK on Linux starts a `Worker` but has no `OffscreenCanvas`, so the worker used to fail its first job
  // and log a warn on every launch.
  it('skips the worker without a warn when the WebView has no OffscreenCanvas, and says so once', async () => {
    const { measureOffMainThread } = await loadClient('answer', { offscreenCanvas: false })

    const first = await measureOffMainThread(spec, new Uint32Array([0x41]))
    const second = await measureOffMainThread(spec, new Uint32Array([0x42]))

    expect(first.via).toBe('main-thread')
    expect(second.via).toBe('main-thread')
    expect(constructed).toHaveLength(0)
    expect(logger.info).toHaveBeenCalledOnce()
    expect(logger.warn).not.toHaveBeenCalled()
  })

  it('skips the worker without a warn when the WebView has no Worker', async () => {
    const { measureOffMainThread } = await loadClient('answer', { worker: false })

    const result = await measureOffMainThread(spec, new Uint32Array([0x41]))

    expect(result.via).toBe('main-thread')
    expect(logger.info).toHaveBeenCalledOnce()
    expect(logger.warn).not.toHaveBeenCalled()
  })

  it('still warns when a supported worker fails for real', async () => {
    const { measureOffMainThread } = await loadClient('error-response')

    await measureOffMainThread(spec, new Uint32Array([0x41]))

    expect(logger.warn).toHaveBeenCalledOnce()
  })
})
