// Dedicated worker that measures character widths off the main thread.
//
// The measuring loop is tens of thousands of `measureText` calls, each of which
// can hit CoreText font fallback. Run on the main thread it froze the whole UI
// for seconds (and, under a busy machine, tens of seconds), because a
// `requestIdleCallback` defers only the START of a synchronous loop. Here it
// can take as long as it likes.
//
// Thin by design: everything but Canvas construction lives in `measure.ts`, so
// it stays unit-testable. Do not import Tauri, Svelte, or the logger here; a
// worker has no `window`, and the log bridge would have nothing to post to.

import { measureCodePoints, type FontSpec } from './measure'

/** A measurement job. `codePoints` arrives transferred, not copied. */
export interface MeasureRequest {
  requestId: number
  spec: FontSpec
  codePoints: Uint32Array
}

/** A finished job. Either `widths` (transferred) or `error` is set. */
export interface MeasureResponse {
  requestId: number
  codePoints?: Uint32Array
  widths?: Float32Array
  error?: string
}

/**
 * The slice of `DedicatedWorkerGlobalScope` this file uses.
 *
 * The app's tsconfig ships the DOM lib, not `webworker`, so the ambient `self`
 * is typed as a `Window` (whose `postMessage` takes a target origin). Naming
 * what a worker actually offers keeps this file type-safe without adding a
 * second lib to the whole build.
 */
interface WorkerScope {
  onmessage: ((event: MessageEvent<MeasureRequest>) => void) | null
  postMessage(message: MeasureResponse, transfer?: Transferable[]): void
}
declare const self: WorkerScope

self.onmessage = (event: MessageEvent<MeasureRequest>) => {
  const { requestId, spec, codePoints } = event.data

  try {
    // 1×1 is enough: `measureText` reports the advance width without drawing.
    const canvas = new OffscreenCanvas(1, 1)
    const ctx = canvas.getContext('2d')
    if (!ctx) {
      throw new Error('OffscreenCanvas 2D context unavailable')
    }

    const widths = measureCodePoints(ctx, spec, codePoints)
    const response: MeasureResponse = { requestId, codePoints, widths }
    // Transfer both buffers back; neither is reused on this side. Both arrays
    // were allocated here or arrived transferred, so their buffers are plain
    // `ArrayBuffer`s — `.buffer`'s wider `ArrayBufferLike` type is what needs
    // the assertion, not the values.
    self.postMessage(response, [codePoints.buffer as ArrayBuffer, widths.buffer as ArrayBuffer])
  } catch (error) {
    const response: MeasureResponse = {
      requestId,
      error: error instanceof Error ? error.message : String(error),
    }
    self.postMessage(response)
  }
}
