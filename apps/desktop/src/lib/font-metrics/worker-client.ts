// Runs measurement jobs, off the main thread when the platform allows it.
//
// Two paths, same result:
//  - **Worker** (the normal one): `measure-worker.ts` owns an `OffscreenCanvas`
//    and the main thread never touches the loop.
//  - **Chunked main thread** (the fallback): used when a WebView has no
//    `Worker` or no `OffscreenCanvas`. It measures against a plain `<canvas>`
//    and yields on a time budget, so it is slow but never freezes the UI.
//
// The fallback exists because a stalled measurement is not an option: every
// unmeasured code point renders at the average width until it's filled in.

import { getAppLogger } from '$lib/logging/logger'
import { measureCodePointsChunked, type FontSpec } from './measure'
import type { MeasureRequest, MeasureResponse } from './measure-worker'

const log = getAppLogger('fontMetrics')

/** How long a single worker job may take before we give up and fall back. */
const WORKER_TIMEOUT_MS = 60_000

/**
 * Main-thread measuring budget per slice. Chosen to stay inside a frame: the
 * loop checks the clock rather than counting characters, so a machine where
 * each `measureText` is 20× slower yields just as promptly.
 */
const MAIN_THREAD_SLICE_MS = 8

let worker: Worker | undefined
/** Set once the worker path has proven unusable; stops us retrying it. */
let workerUnavailable = false
let nextRequestId = 1

const pending = new Map<number, { resolve: (widths: Float32Array) => void; reject: (error: Error) => void }>()

/**
 * Lazily constructs the worker. Returns `undefined` when the platform can't
 * run one, which sends the caller to the chunked fallback.
 *
 * Constructed on first use rather than at module load so importing this module
 * stays free (and so unit tests that never measure never touch `new Worker`).
 */
function getWorker(): Worker | undefined {
  if (workerUnavailable) return undefined
  if (worker) return worker

  try {
    worker = new Worker(new URL('./measure-worker.ts', import.meta.url), { type: 'module' })
    worker.onmessage = (event: MessageEvent<MeasureResponse>) => {
      const { requestId, widths, error } = event.data
      const entry = pending.get(requestId)
      if (!entry) return
      pending.delete(requestId)
      if (widths) {
        entry.resolve(widths)
      } else {
        entry.reject(new Error(error ?? 'Worker returned no widths'))
      }
    }
    worker.onerror = (event) => {
      // A worker-level error kills every job in flight; there's no request ID
      // to attribute it to.
      const error = new Error(event.message || 'Font metrics worker error')
      for (const [, entry] of pending) entry.reject(error)
      pending.clear()
      retireWorker()
    }
    return worker
  } catch (error) {
    log.warn('Could not start the measuring worker, using the chunked fallback: {error}', { error })
    workerUnavailable = true
    return undefined
  }
}

/** Drops the worker so the next job takes the fallback path. */
function retireWorker(): void {
  workerUnavailable = true
  try {
    worker?.terminate()
  } catch {
    // Already dead; nothing to clean up.
  }
  worker = undefined
}

/** Posts one job to the worker and waits for its widths. */
function measureInWorker(activeWorker: Worker, spec: FontSpec, codePoints: Uint32Array): Promise<Float32Array> {
  return new Promise<Float32Array>((resolve, reject) => {
    const requestId = nextRequestId++
    const timer = setTimeout(() => {
      pending.delete(requestId)
      reject(new Error(`Measuring worker did not answer within ${String(WORKER_TIMEOUT_MS)}ms`))
    }, WORKER_TIMEOUT_MS)

    pending.set(requestId, {
      resolve: (widths) => {
        clearTimeout(timer)
        resolve(widths)
      },
      reject: (error) => {
        clearTimeout(timer)
        reject(error)
      },
    })

    // `codePoints` is transferred, so hand the worker a copy and keep ours.
    const owned = codePoints.slice()
    const request: MeasureRequest = { requestId, spec, codePoints: owned }
    activeWorker.postMessage(request, [owned.buffer])
  })
}

/**
 * Measures on the main thread in time-boxed slices, yielding between them.
 *
 * Only reached when the worker path is unavailable. Slower overall than the
 * worker (each yield costs a task hop) but it keeps the UI interactive, which
 * is the property that matters.
 */
function measureChunkedOnMainThread(spec: FontSpec, codePoints: Uint32Array): Promise<Float32Array> {
  const canvas = document.createElement('canvas')
  const ctx = canvas.getContext('2d')
  if (!ctx) {
    throw new Error('2D canvas context unavailable')
  }

  return measureCodePointsChunked(
    ctx,
    spec,
    codePoints,
    MAIN_THREAD_SLICE_MS,
    () => new Promise((resolve) => setTimeout(resolve, 0)),
  )
}

/** Which path a measurement actually took. Reported in the timing log. */
export type MeasurePath = 'worker' | 'main-thread'

export interface MeasureResult {
  widths: Float32Array
  via: MeasurePath
}

/**
 * Measures `codePoints` for `spec`, preferring the worker.
 *
 * Falls back to the chunked main-thread path if the worker can't start or
 * fails mid-job, and retires the worker so later jobs don't pay the timeout
 * again.
 */
export async function measureOffMainThread(spec: FontSpec, codePoints: Uint32Array): Promise<MeasureResult> {
  const activeWorker = getWorker()
  if (activeWorker) {
    try {
      const widths = await measureInWorker(activeWorker, spec, codePoints)
      return { widths, via: 'worker' }
    } catch (error) {
      log.warn('Measuring worker failed, falling back to the chunked main thread: {error}', { error })
      retireWorker()
    }
  }

  const widths = await measureChunkedOnMainThread(spec, codePoints)
  return { widths, via: 'main-thread' }
}
