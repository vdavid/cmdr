/**
 * Quick filter for a file pane: Total Commander's "type to narrow the list".
 * One instance per pane, inside `FilePane`, alongside the type-to-jump
 * controller; which of the two a keystroke reaches is the
 * `fileExplorer.typeToJump.mode` setting (`routeTypingKey` in
 * `type-to-jump-keys.ts`).
 *
 * The filtering itself is the backend's: the pattern lives on the listing and is
 * an input to its ONE row-visibility predicate
 * (`src-tauri/src/file_system/listing/name_filter.rs`), so counts, ranges,
 * selection indices, and `directory-diff` rows all speak the filtered row space.
 * This controller only holds the pattern, ships it, and applies the answer
 * (count, cursor, selection) to the pane.
 *
 * ## One request in flight, latest pattern wins
 *
 * Each IPC call swaps the listing's row space, so two in flight could land in
 * either order and leave the pane drawing one pattern's rows under another's
 * indicator. So calls run one at a time, and each sends the pattern as it stands
 * when it STARTS: a successful burst costs at most one extra round trip.
 *
 * ## Typing stops at the last match
 *
 * A pattern that GROWS is sent with `refuseEmpty`: when it would match nothing,
 * the backend keeps the old filter and says so (`accepted: false`), and the
 * controller replays refused growth one character at a time, dropping only
 * characters that match nothing (Total Commander's rule). Successful bursts
 * stay batched. A shrinking pattern (Backspace) is never refused: it can't match
 * less than the longer one did. Clear, Backspace, and reset cancel replay so
 * their newer pattern stands; newly appended characters follow the replay.
 *
 * ## The new row space starts at a diff sequence
 *
 * A change of filter answers the `directory-diff` sequence its rows start at
 * (`sequence`). The pane takes it as its last applied one, so a diff the
 * backend numbered before the switch, which speaks the old rows, is skipped; the
 * full refetch the switch triggers already holds its change.
 *
 * A pattern belongs to its listing. A new listing (navigation, tab switch)
 * starts unfiltered on the backend, so `reset()` drops the pattern without IPC,
 * and an answer for a listing the pane has left is dropped.
 */

import { setListingNameFilter } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'
import type { ListingUpdateQueue } from './listing-update-queue'
import type { PaneRowState } from './pane-row-state'

const log = getAppLogger('fileExplorer')

/** What the pane needs to apply after a filter change, in FRONTEND indices (`..` included). */
export interface QuickFilterApplied {
  totalCount: number
  cursorIndex: number
  selectedIndices: number[]
  /** The diff sequence the new row space starts at; `null` when the row space didn't change. */
  sequence: number | null
}

export interface QuickFilterControllerDeps {
  rowState?: PaneRowState
  runListingUpdate: ListingUpdateQueue
  getListingId: () => string
  getLoading: () => boolean
  /** Whether the pane's volume kind has a real backend listing to filter. */
  getHasBackendListing: () => boolean
  getIncludeHidden: () => boolean
  getHasParent: () => boolean
  /** The file under the cursor (never `..`), to keep the cursor on it. */
  getCursorFilename: () => string | undefined
  /** Selected rows, FRONTEND indices. */
  getSelectedIndices: () => number[]
  /** Apply the new row space: count, cursor, selection, and a full refetch of the rows. */
  apply: (applied: QuickFilterApplied) => void
}

export interface QuickFilterController {
  /** The pattern the user has typed; empty when no filter is on. */
  readonly pattern: string
  isActive: () => boolean
  append: (char: string) => void
  backspace: () => void
  /** Clears the pattern and shows every row again. */
  clear: () => void
  /** Forgets the pattern without IPC: the pane moved to a listing that starts unfiltered. */
  reset: () => void
}

/** Backend row indices from frontend ones: drops `..` and shifts the rest. */
function toBackend(indices: number[], hasParent: boolean): number[] {
  if (!hasParent) return indices
  return indices.filter((i) => i > 0).map((i) => i - 1)
}

export function createQuickFilterController(deps: QuickFilterControllerDeps): QuickFilterController {
  let pattern = $state('')
  /** The pattern the backend's listing holds, per our last answered call. */
  let applied = ''
  let inFlight = false
  // Destructive edits invalidate a replay; appending can keep its untested suffix.
  let edit = 0
  let generation = 0

  function canFilter(): boolean {
    return deps.getListingId() !== '' && !deps.getLoading() && deps.getHasBackendListing()
  }

  async function sendOnce(listingId: string, sent: string): Promise<boolean | undefined> {
    const startedGeneration = generation
    if (deps.rowState) {
      let accepted: boolean | undefined
      const hasParent = deps.getHasParent()
      await deps.rowState.changeView({
        isCurrent: () => deps.getListingId() === listingId && generation === startedGeneration,
        request: async (token) => {
          const result = await setListingNameFilter(
            listingId,
            sent === '' ? null : sent,
            token.includeHidden,
            deps.getCursorFilename(),
            toBackend(deps.getSelectedIndices(), hasParent),
            sent.length > applied.length,
            token.sequence,
          )
          accepted = result.accepted
          return { ...result, sequence: result.sequence ?? token.sequence }
        },
        install: (result) => {
          if (!accepted) return
          applied = sent
          const offset = hasParent ? 1 : 0
          deps.apply({
            totalCount: result.totalCount,
            cursorIndex:
              result.newCursorIndex === null ? (result.totalCount > 0 ? offset : 0) : result.newCursorIndex + offset,
            selectedIndices: (result.newSelectedIndices ?? []).map((i) => i + offset),
            sequence: result.sequence,
          })
        },
      })
      return accepted
    }
    return deps.runListingUpdate(async () => {
      if (deps.getListingId() !== listingId || generation !== startedGeneration) return
      const hasParent = deps.getHasParent()
      const result = await setListingNameFilter(
        listingId,
        sent === '' ? null : sent,
        deps.getIncludeHidden(),
        deps.getCursorFilename(),
        toBackend(deps.getSelectedIndices(), hasParent),
        sent.length > applied.length,
      )
      // The pane moved on while we waited: this answer describes rows it no longer shows.
      if (deps.getListingId() !== listingId || generation !== startedGeneration) return
      if (!result.accepted) return false
      applied = sent
      const offset = hasParent ? 1 : 0
      const firstRow = result.totalCount > 0 ? offset : 0
      deps.apply({
        totalCount: result.totalCount,
        cursorIndex: result.newCursorIndex === null ? firstRow : result.newCursorIndex + offset,
        selectedIndices: result.newSelectedIndices.map((i) => i + offset),
        sequence: result.sequence,
      })
      return true
    })
  }

  async function sendPattern(listingId: string): Promise<void> {
    const sent = pattern
    const base = applied
    const startedEdit = edit
    const accepted = await sendOnce(listingId, sent)
    if (accepted !== false || edit !== startedEdit) return
    if (!sent.startsWith(base)) {
      if (pattern.startsWith(sent)) pattern = applied
      return
    }
    const suffix = Array.from(sent.slice(base.length))
    for (const char of suffix) {
      const candidate = applied + char
      // A single-character refusal already tells us everything; don't retry it.
      const matches = suffix.length === 1 ? false : await sendOnce(listingId, candidate)
      if (matches === undefined || edit !== startedEdit) return
      if (!matches) pattern = applied + pattern.slice(candidate.length)
    }
  }

  async function sync(): Promise<void> {
    if (inFlight) return
    inFlight = true
    const startedGeneration = generation
    try {
      // Re-check after each call: keystrokes that landed meanwhile only moved `pattern`.
      while (pattern !== applied) {
        const listingId = deps.getListingId()
        if (listingId === '') return
        await sendPattern(listingId)
        if (deps.getListingId() !== listingId) return
      }
    } catch (e) {
      // The backend refusal is typed (`ListingLookupError`); a gone listing is the
      // ordinary race with a navigation, and the next listing starts unfiltered.
      log.warn("quick filter couldn't apply: {reason}", { reason: (e as { type?: string }).type ?? String(e) })
    } finally {
      inFlight = false
      // New-listing typing may have queued behind a departed listing's call.
      if (generation !== startedGeneration && pattern !== applied) void sync()
    }
  }

  function setPattern(next: string): void {
    pattern = next
    void sync()
  }

  return {
    get pattern() {
      return pattern
    },
    isActive: () => pattern !== '',
    append: (char: string) => {
      if (!canFilter()) return
      setPattern(pattern + char)
    },
    backspace: () => {
      if (pattern === '') return
      edit++
      setPattern(Array.from(pattern).slice(0, -1).join(''))
    },
    clear: () => {
      if (pattern === '') return
      edit++
      setPattern('')
    },
    reset: () => {
      edit++
      generation++
      pattern = ''
      applied = ''
    },
  }
}
