/**
 * Selection-to-clipboard composable for the viewer.
 *
 * Owns the three-band copy flow (silent / confirm / refuse), the busy flag that
 * prevents overlapping copies, and the in-flight `read_id` used to cancel a long
 * read via Escape.
 *
 * The page component wires this in: it provides `getSessionId`, `getSelectionBytes`,
 * and `getRangeEnds` (the current selection's anchor/focus expressed as `RangeEnd`s
 * for the IPC layer). On a copy gesture, the page calls `runCopy()` which picks the
 * right band and returns a `CopyOutcome`. Toast/dialog presentation lives in the page
 * to keep this module independent of UI primitives.
 */

import {
  viewerCancelRead,
  viewerReadRange,
  viewerWriteRangeToFile,
  type RangeEnd,
  type ViewerError,
} from '$lib/tauri-commands'

import { selectCopyAction, type CopyAction } from './viewer-copy'

export type CopyOutcome =
  | { kind: 'silent'; text: string; bytes: number }
  | { kind: 'silent-error'; bytes: number; reason: 'cancelled' | 'timedOut' | 'other'; error?: ViewerError }
  | { kind: 'confirm'; bytes: number; proceed: () => Promise<CopyResult> }
  | { kind: 'refuse'; bytes: number }
  | { kind: 'unknown-size'; proceed: () => Promise<CopyResult>; bytes: null }
  | { kind: 'empty' }
  | { kind: 'busy' }

export type CopyResult =
  | { ok: true; text: string }
  | { ok: false; reason: 'cancelled' | 'timedOut' | 'other'; error?: ViewerError }

interface CopyDeps {
  getSessionId: () => string
  /**
   * Returns the byte estimate of the current selection, or `null` if the size can't be
   * estimated (ByteSeek-no-index in a range we haven't fetched). The composable treats
   * `null` as "ask before paying for the read"; the page can refuse outright if the
   * caller prefers.
   */
  getSelectionBytes: () => number | null
  /**
   * Returns the current selection's endpoints as `RangeEnd`s for the IPC layer, or
   * `null` if the selection is empty. The composable doesn't reach into the selection
   * model directly; the page maps `Selection | null` -> `(anchor, focus): RangeEnd`.
   */
  getRangeEnds: () => { anchor: RangeEnd; focus: RangeEnd } | null
}

export function createViewerCopy(deps: CopyDeps) {
  let busy = $state(false)
  let inFlightReadId = $state<number | null>(null)

  // Monotonic per-composable counter; only needs uniqueness within a session.
  let nextReadId = 0

  /** Picks the next `read_id` to send with `viewerReadRange`. */
  function allocateReadId(): number {
    const id = nextReadId
    nextReadId += 1
    return id
  }

  async function performRead(): Promise<CopyResult> {
    const sessionId = deps.getSessionId()
    const ends = deps.getRangeEnds()
    if (!sessionId || ends === null) {
      return { ok: false, reason: 'other' }
    }
    const readId = allocateReadId()
    inFlightReadId = readId
    busy = true
    try {
      const res = await viewerReadRange(sessionId, readId, ends.anchor, ends.focus)
      if (res.ok) return { ok: true, text: res.text }
      if (res.error.kind === 'cancelled') return { ok: false, reason: 'cancelled', error: res.error }
      if (res.error.kind === 'timedOut') return { ok: false, reason: 'timedOut', error: res.error }
      return { ok: false, reason: 'other', error: res.error }
    } finally {
      busy = false
      inFlightReadId = null
    }
  }

  /**
   * Picks the right copy band and either reads immediately (silent) or returns a
   * `proceed()` for the caller to drive after user confirmation.
   *
   * Empty selections return `{ kind: 'empty' }` (caller should no-op). A second copy
   * gesture while one is in flight returns `{ kind: 'busy' }` (caller should no-op).
   */
  async function runCopy(): Promise<CopyOutcome> {
    if (busy) return { kind: 'busy' }
    const ends = deps.getRangeEnds()
    if (ends === null) return { kind: 'empty' }

    const bytes = deps.getSelectionBytes()
    if (bytes === null) {
      // Unknown size: ask before paying. Caller decides how to render the confirm.
      return { kind: 'unknown-size', bytes: null, proceed: performRead }
    }

    const action: CopyAction = selectCopyAction(bytes)
    if (action === 'silent') {
      const result = await performRead()
      if (result.ok) return { kind: 'silent', text: result.text, bytes }
      // Surface the failure to the caller. Per design-principles top-5 §3
      // ("communicate what's actually happening"), a silent-band IO error gets a
      // brief toast instead of pretending nothing happened. The caller suppresses
      // the toast for `cancelled` (the user pressed Escape, intentional).
      return { kind: 'silent-error', bytes, reason: result.reason, error: result.error }
    }
    if (action === 'refuse') return { kind: 'refuse', bytes }
    return { kind: 'confirm', bytes, proceed: performRead }
  }

  /**
   * Cancels the in-flight read, if any. Safe to call when no read is running (no-op).
   * Returns immediately; the actual cancel propagates through the cancel flag on the
   * backend.
   */
  async function cancelInFlight(): Promise<void> {
    const sessionId = deps.getSessionId()
    const readId = inFlightReadId
    if (!sessionId || readId === null) return
    await viewerCancelRead(sessionId, readId)
  }

  /**
   * Writes the selection to a file at `destPath`. Same band-agnostic API as `runCopy`,
   * but the result is the typed write outcome rather than the read text. Uses the
   * same busy / cancel plumbing.
   */
  async function saveAs(destPath: string): Promise<CopyResult> {
    const sessionId = deps.getSessionId()
    const ends = deps.getRangeEnds()
    if (!sessionId || ends === null) {
      return { ok: false, reason: 'other' }
    }
    const readId = allocateReadId()
    inFlightReadId = readId
    busy = true
    try {
      const res = await viewerWriteRangeToFile(sessionId, readId, ends.anchor, ends.focus, destPath)
      if (res.ok) return { ok: true, text: '' }
      if (res.error.kind === 'cancelled') return { ok: false, reason: 'cancelled', error: res.error }
      if (res.error.kind === 'timedOut') return { ok: false, reason: 'timedOut', error: res.error }
      return { ok: false, reason: 'other', error: res.error }
    } finally {
      busy = false
      inFlightReadId = null
    }
  }

  return {
    get busy() {
      return busy
    },
    get inFlightReadId() {
      return inFlightReadId
    },
    runCopy,
    cancelInFlight,
    saveAs,
  }
}
