/**
 * Brief mode's per-column widths: what the backend measured, and every rule for
 * keeping that measurement honest when it doesn't arrive.
 *
 * The backend returns the widest filename's TEXT width per column, a pure function of
 * `(listing, itemsPerColumn, hasParent, fontId, includeHidden)`. What this owns is the
 * raw numbers plus the fetch policy; the chrome-and-clamp pass that turns them into
 * rendered widths is `clampColumnWidths`, a pure function the host runs in a `$derived`.
 *
 * Three properties are load-bearing, and each one exists because its absence was a bug:
 *
 * - **The raw widths are stored, the clamped ones derived.** The clamp depends on the
 *   pane width (`capPx`), the raw widths don't, so a resize is synchronous frontend
 *   math and never an IPC. Storing clamped widths made every pane resize a refetch, and
 *   each refetch a chance to fail.
 * - **A response is discarded only when it's genuinely stale**, meaning the listing
 *   changed under it (`epoch`) or a NEWER response already landed (`requestId`). Merely
 *   having asked again does not throw away an answer that arrived.
 * - **Readiness is a state, not an inference.** `widths.length === 0` is also what an
 *   empty directory legitimately returns, so `pending` / `ready` / `degraded` is tracked
 *   explicitly. Nothing user-visible may gate on it: the host renders provisional widths
 *   in `pending` and `degraded`, and the cursor is drawn regardless (see `DETAILS.md`
 *   § "Cursor visibility never waits on measurement").
 *
 * ❌ Each dep is its own GETTER, matching `full-list-cache.svelte.ts`; see the note there.
 */

import { getBriefColumnTextWidths } from '$lib/tauri-commands'
import { ensureFontMetricsLoaded, fillMissingFontMetrics, getCurrentFontId } from '$lib/font-metrics'
import { getAppLogger } from '$lib/logging/logger'
import type { BriefColumnsErrorKind } from '$lib/ipc/bindings'

const log = getAppLogger('fileExplorer')

/** Floor for a rendered column: narrower than this and a name is all ellipsis. */
export const MIN_COLUMN_WIDTH = 100

/**
 * CSS chrome around the measured text: icon (16) + gap (8) + left padding (8) +
 * right padding (8) + 2 px of rounding buffer for sub-pixel rendering.
 */
export const COLUMN_PADDING = 16 + 8 + 8 + 8 + 2

/**
 * Width a column renders at before (or instead of) a measurement.
 *
 * Picked to sit inside the range real columns land in: a short-name column
 * (`a-00.txt`) bottoms out at the `MIN_COLUMN_WIDTH` floor of 100 px, and the
 * `listing.briefColumnWidthMaxPx` ceiling users can opt into defaults to 400 px.
 * 260 px is comfortably between them, so an unmeasured column reads as a column
 * (several fit across a pane, most names fit inside one) rather than as a
 * pane-wide stripe. ❌ Never fall back to `capPx`: that IS the pane width, so a
 * single unmeasured column swallows the whole view.
 */
export const DEFAULT_BRIEF_COLUMN_WIDTH = 260

/** How long a re-fetch waits, so a resize burst produces one IPC rather than 60. */
const COALESCE_MS = 50

/**
 * Backoff before each retry of a transient failure. Two entries = two retries.
 * Short, because the pane is showing provisional widths the whole time; the point
 * is to ride out a `LISTING_CACHE` lock or a listing that hasn't been registered
 * yet, not to wait out a real outage.
 */
const RETRY_BACKOFF_MS = [150, 400]

/** Whether the widths on screen are provisional, measured, or given up on. */
export type BriefWidthsStatus = 'pending' | 'ready' | 'degraded'

/** The rendered width of a column whose text width hasn't been measured. */
export function provisionalColumnWidth(capPx: number): number {
  return Math.max(MIN_COLUMN_WIDTH, Math.min(capPx, DEFAULT_BRIEF_COLUMN_WIDTH))
}

/**
 * Turns backend text widths into rendered widths: add the CSS chrome, then clamp
 * into `[MIN_COLUMN_WIDTH, capPx]`. Pure, and the only place the pane width enters
 * the width math.
 */
export function clampColumnWidths(rawTextWidths: readonly number[], capPx: number): number[] {
  const clamped = new Array<number>(rawTextWidths.length)
  for (let i = 0; i < rawTextWidths.length; i++) {
    clamped[i] = Math.max(MIN_COLUMN_WIDTH, Math.min(capPx, rawTextWidths[i] + COLUMN_PADDING))
  }
  return clamped
}

/** Live reads of the host's props. One getter per prop; see the ❌ note above. */
export interface BriefColumnWidthsDeps {
  listingId: () => string
  itemsPerColumn: () => number
  hasParent: () => boolean
  includeHidden: () => boolean
  /**
   * Called when the first measurement of a listing lands, so the host can snap the
   * column-width CSS transition for one paint instead of letting the columns slide
   * from their provisional width to their measured one.
   */
  onFirstWidths?: () => void
}

export interface BriefColumnWidthsStore {
  /** Backend text widths, chrome-free and unclamped. Reactive. */
  readonly rawWidths: number[]
  /** Reactive. See `BriefWidthsStatus`. */
  readonly status: BriefWidthsStatus
  /**
   * Asks for widths. Coalesced by `COALESCE_MS` once a listing has widths on
   * screen; the first request after a reset fires immediately, so the provisional
   * columns are replaced as soon as possible.
   */
  request: () => void
  /**
   * Drops the current measurement and every answer still in flight for it. For a
   * cold context change (navigation, hidden-files toggle, explicit refresh).
   */
  reset: () => void
  /** Cancels pending work. Call from the host's unmount teardown. */
  destroy: () => void
}

/** Which recovery steps an attempt has already spent. Each flag bounds its own recursion. */
interface WidthFetchAttempt {
  /** Already waited for `ensureFontMetricsLoaded` once. */
  afterFontLoad?: boolean
  /** Already measured the reported code points once. */
  afterFill?: boolean
  /** Transient retries already spent. Indexes `RETRY_BACKOFF_MS`. */
  retries?: number
}

/**
 * Whether a failure of this kind can plausibly succeed on a second ask.
 *
 * `invalidItemsPerColumn` is the caller passing 0, so retrying re-asks the same
 * wrong question forever. Everything else is a lock, a deadline, or a listing that
 * hasn't been registered yet.
 */
function isTransient(kind: BriefColumnsErrorKind): boolean {
  return kind !== 'invalidItemsPerColumn'
}

export function createBriefColumnWidths(deps: BriefColumnWidthsDeps): BriefColumnWidthsStore {
  let rawWidths = $state<number[]>([])
  let status = $state<BriefWidthsStatus>('pending')

  // Bumped by `reset()` and `destroy()`: every response captured under an older
  // epoch belongs to a listing that's gone, and is dropped on arrival.
  let epoch = 0
  // Monotonic per request, so responses can be ordered. A response older than one
  // already applied is dropped; a response merely older than one still IN FLIGHT is
  // applied, because an unanswered request is not an answer.
  let nextRequestId = 0
  let lastAppliedRequestId = -1
  // Whether this epoch has painted a measured width yet, so `onFirstWidths` fires
  // once per listing rather than on every refresh.
  let sawWidths = false

  let coalesceTimer: ReturnType<typeof setTimeout> | null = null
  let retryTimer: ReturnType<typeof setTimeout> | null = null

  function cancelTimers(): void {
    if (coalesceTimer !== null) {
      clearTimeout(coalesceTimer)
      coalesceTimer = null
    }
    if (retryTimer !== null) {
      clearTimeout(retryTimer)
      retryTimer = null
    }
  }

  /** True once the listing this response was asked for is no longer the one on screen. */
  function isStale(capturedEpoch: number, capturedListingId: string): boolean {
    return capturedEpoch !== epoch || capturedListingId !== deps.listingId()
  }

  function giveUp(listingId: string, kind: BriefColumnsErrorKind, detail: string, attempt: WidthFetchAttempt): void {
    status = 'degraded'
    log.warn(
      'Brief column widths unavailable for listing {listingId} after {attempts} attempt(s): {kind} ({detail}). Columns stay at their provisional width.',
      { listingId, attempts: (attempt.retries ?? 0) + 1, kind, detail },
    )
  }

  /**
   * Decides what a failed attempt does next: recover the font, back off and retry,
   * or settle into `degraded`. Every path leaves a log line; a silent bail here is
   * what made the original bug invisible in production.
   */
  function handleFailure(
    kind: BriefColumnsErrorKind,
    detail: string,
    attempt: WidthFetchAttempt,
    capturedEpoch: number,
    capturedListingId: string,
  ): void {
    if (kind === 'fontMetricsNotReady' && !attempt.afterFontLoad) {
      log.debug('Brief column widths for listing {listingId} are waiting on font metrics', {
        listingId: capturedListingId,
      })
      void ensureFontMetricsLoaded().then(() => {
        if (isStale(capturedEpoch, capturedListingId)) return
        void run({ ...attempt, afterFontLoad: true }, capturedEpoch)
      })
      return
    }

    const retries = attempt.retries ?? 0
    if (!isTransient(kind) || retries >= RETRY_BACKOFF_MS.length) {
      giveUp(capturedListingId, kind, detail, attempt)
      return
    }

    log.warn(
      'Brief column widths for listing {listingId} did not land (attempt {attempt}): {kind} ({detail}). Retrying in {backoffMs} ms.',
      { listingId: capturedListingId, attempt: retries + 1, kind, detail, backoffMs: RETRY_BACKOFF_MS[retries] },
    )
    if (retryTimer !== null) clearTimeout(retryTimer)
    retryTimer = setTimeout(() => {
      retryTimer = null
      if (isStale(capturedEpoch, capturedListingId)) return
      void run({ ...attempt, retries: retries + 1 }, capturedEpoch)
    }, RETRY_BACKOFF_MS[retries])
  }

  async function run(attempt: WidthFetchAttempt, capturedEpoch: number): Promise<void> {
    const capturedListingId = deps.listingId()
    const requestId = nextRequestId++
    const fontId = getCurrentFontId()
    const itemsPerColumn = Math.max(1, deps.itemsPerColumn())
    const hasParent = deps.hasParent()
    const includeHidden = deps.includeHidden()

    // The whole round trip lives in the try: a thrown IPC (no handler, bridge gone)
    // and a reply that doesn't have the shape it promised are both "we got no usable
    // widths", and both must land in `handleFailure` rather than escaping as an
    // unhandled rejection.
    try {
      const result = await getBriefColumnTextWidths(capturedListingId, itemsPerColumn, hasParent, fontId, includeHidden)
      if (isStale(capturedEpoch, capturedListingId)) return

      if (result.status === 'error') {
        handleFailure(result.error.kind, result.error.message, attempt, capturedEpoch, capturedListingId)
        return
      }

      // A newer response already painted: this one is stale by ordering, not by listing.
      if (requestId <= lastAppliedRequestId) return
      lastAppliedRequestId = requestId

      const { widths, missingCodePoints } = result.data
      const wasFirst = !sawWidths
      sawWidths = true
      rawWidths = widths
      status = 'ready'
      if (wasFirst && widths.length > 0) {
        deps.onFirstWidths?.()
      }

      // Exact unless the backend had to estimate some characters. Measure those off
      // the main thread and come back once; `afterFill` bounds it to one extra round,
      // so a permanently unmeasurable code point can't drive an endless loop.
      if (missingCodePoints.length > 0 && !attempt.afterFill) {
        void fillMissingFontMetrics(fontId, missingCodePoints).then((filled) => {
          if (!filled || isStale(capturedEpoch, capturedListingId)) return
          void run({ ...attempt, afterFill: true }, capturedEpoch)
        })
      }
    } catch (err) {
      if (isStale(capturedEpoch, capturedListingId)) return
      handleFailure('other', String(err), attempt, capturedEpoch, capturedListingId)
    }
  }

  return {
    get rawWidths() {
      return rawWidths
    },
    get status() {
      return status
    },

    request: () => {
      if (!deps.listingId() || deps.itemsPerColumn() <= 0) return
      cancelTimers()
      // Nothing measured yet (fresh listing, or a give-up we're re-attempting):
      // fire immediately so the provisional columns are replaced as soon as they can be.
      if (rawWidths.length === 0) {
        void run({}, epoch)
        return
      }
      coalesceTimer = setTimeout(() => {
        coalesceTimer = null
        void run({}, epoch)
      }, COALESCE_MS)
    },

    reset: () => {
      cancelTimers()
      epoch++
      rawWidths = []
      status = 'pending'
      sawWidths = false
    },

    destroy: () => {
      cancelTimers()
      epoch++
    },
  }
}
