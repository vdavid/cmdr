/**
 * What a focused pane says through its live region (`FilePaneAPI.announce`) besides where it
 * is: a load that's taking a while, a folder that just went empty (or whose hidden-item count
 * moved while it looks empty), the selection count, and
 * the quick filter's match count (and its clearing, which brings every name back). Each is otherwise visible only (the loading icon, the
 * "Empty folder" text, the status bar, the filter badge).
 *
 * Where it is ("Left pane, Documents", plus "empty folder" when it arrives in one) is
 * `pane-location-announcer.svelte.ts`'s; this one speaks only about CHANGES in place, so it
 * re-baselines silently whenever the pane takes focus, changes folder, or finishes a load.
 *
 * ❗ Chatty-safe. A burst (a held ↓ while marking, a filter typed letter by letter, a
 * streaming listing) says only its end state, `STATUS_SETTLE_MS` after the changes pause, and
 * a change that lands back where it started says nothing. When several things changed at
 * once, the cause wins over its side effects: a filter's match count over the selection it
 * dropped, an emptied folder over the marks that went with it.
 */

import { untrack } from 'svelte'
import { tString } from '#lib/intl/messages.svelte.js'
import { formatInteger } from '#lib/intl/number-format.js'
import { emptyFolderSpokenText } from '../views/empty-folder-text'

/** How long a load runs before "Loading…" is said; a quicker one says nothing about itself. */
export const LOADING_ANNOUNCE_MS = 1000
/** How long the pane's state must hold still before a change is said. */
export const STATUS_SETTLE_MS = 750

export interface PaneStatusInput {
  isFocused: boolean
  /** What the pane shows (its path); a change starts over without a word. */
  location: string
  loading: boolean
  /** The pane shows an error, not a listing: zero entries then doesn't mean empty. */
  hasError: boolean
  /** Entries in the listing as shown (after the quick filter), `..` excluded. */
  entryCount: number
  /** Hidden items an empty pane hides (the backend's `hiddenCount`), zero while it shows rows. */
  hiddenCount: number
  selectedCount: number
  /** The quick filter's pattern while it narrows the list, else `null`. */
  filterPattern: string | null
}

export interface PaneStatusAnnouncerDeps {
  read: () => PaneStatusInput
  announce: (message: string) => void
}

/** The one message a settled change earns, or `null` when nothing worth saying changed. */
function messageFor(from: PaneStatusInput, to: PaneStatusInput): string | null {
  if (to.filterPattern !== null && (to.filterPattern !== from.filterPattern || to.entryCount !== from.entryCount)) {
    return tString('fileExplorer.pane.filterMatches', {
      count: to.entryCount,
      countText: formatInteger(to.entryCount),
      pattern: to.filterPattern,
    })
  }
  if (to.filterPattern === null && from.filterPattern !== null) {
    return tString('fileExplorer.pane.filterCleared', {
      count: to.entryCount,
      countText: formatInteger(to.entryCount),
    })
  }
  if (!to.hasError && to.entryCount === 0 && (from.entryCount > 0 || from.hiddenCount !== to.hiddenCount)) {
    // What the empty state shows, read aloud: "Empty folder", or the hidden items and how to see them.
    return emptyFolderSpokenText(to.hiddenCount)
  }
  if (to.selectedCount !== from.selectedCount) {
    return tString('fileExplorer.pane.selectedCount', {
      count: to.selectedCount,
      countText: formatInteger(to.selectedCount),
    })
  }
  return null
}

/** Call during component init (it installs effects). */
export function createPaneStatusAnnouncer(deps: PaneStatusAnnouncerDeps): void {
  /** What the listener last knew about: the state at arrival, or at the last thing said. */
  let baseline: PaneStatusInput | null = null
  let settleTimer: ReturnType<typeof setTimeout> | undefined
  let loadingTimer: ReturnType<typeof setTimeout> | undefined

  function stopTimers(): void {
    clearTimeout(settleTimer)
    clearTimeout(loadingTimer)
    settleTimer = undefined
    loadingTimer = undefined
  }

  function settle(): void {
    settleTimer = undefined
    const now = deps.read()
    if (!baseline || !now.isFocused || now.loading) return
    const message = messageFor(baseline, now)
    baseline = now
    if (message !== null) deps.announce(message)
  }

  $effect(() => {
    const now = deps.read()
    untrack(() => {
      if (!now.isFocused) {
        stopTimers()
        baseline = null
        return
      }
      if (now.loading) {
        clearTimeout(settleTimer)
        settleTimer = undefined
        // A load starts over: what the pane shows next is an arrival, not a change.
        if (baseline !== null || loadingTimer === undefined) {
          baseline = null
          clearTimeout(loadingTimer)
          loadingTimer = setTimeout(() => {
            if (deps.read().loading && deps.read().isFocused) deps.announce(tString('ui.loadingIcon.loading'))
          }, LOADING_ANNOUNCE_MS)
        }
        return
      }
      clearTimeout(loadingTimer)
      loadingTimer = undefined
      if (baseline === null || baseline.location !== now.location) {
        // Taking focus, a new folder, a load just done: start from here, silently.
        clearTimeout(settleTimer)
        settleTimer = undefined
        baseline = now
        return
      }
      clearTimeout(settleTimer)
      settleTimer = setTimeout(settle, STATUS_SETTLE_MS)
    })
  })
  $effect(() => () => {
    stopTimers()
  })
}
