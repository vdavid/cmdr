/**
 * Quick Look cursor-follow: while the Quick Look panel is open, push the path
 * under the focused pane's cursor to the backend on every cursor move, pane
 * switch, or navigation — plus a companion effect that dismisses the panel when
 * the focused pane drops into an error state. Lifted out of `DualPaneExplorer`
 * into a `*.svelte.ts` factory owning both reactive `$effect`s and its debounce
 * timer.
 *
 * Created synchronously during component init (the `initListingDiffSync` pattern)
 * so the effects get Svelte's tracking context; `cleanup()` clears the pending
 * timer from `onDestroy`.
 */

import { quickLookSetPath } from '$lib/tauri-commands'
import { closeFromPaneError, quickLookState } from '$lib/file-explorer/quick-look/quick-look-state.svelte'
import { getAppLogger } from '$lib/logging/logger'
import { pathInsideArchive } from './volume-capabilities'
import type { FilePaneAPI } from './types'

const log = getAppLogger('fileExplorer')

const QUICK_LOOK_FOLLOW_DEBOUNCE_MS = 100

export interface QuickLookFollowDeps {
  getFocusedPane: () => 'left' | 'right'
  getPaneRef: (pane: 'left' | 'right') => FilePaneAPI | undefined
  getPaneVolumeId: (pane: 'left' | 'right') => string
}

export interface QuickLookFollow {
  /** Clears any pending debounce timer. Call from `onDestroy`. */
  cleanup: () => void
}

export function initQuickLookFollow(deps: QuickLookFollowDeps): QuickLookFollow {
  // The generation counter (same pattern as `type-to-jump-state.svelte.ts`)
  // drops out-of-order responses if the user nav-bursts faster than IPC round-trip;
  // each scheduled call captures its generation and bails on a stale fire.
  let quickLookFollowGeneration = 0
  let quickLookFollowTimer: ReturnType<typeof setTimeout> | null = null
  let quickLookLastSentPath: string | undefined

  // Quick Look cursor-follow: while the panel is open, push the path under the
  // focused pane's cursor to the backend on every cursor move, pane switch, or
  // directory navigation. The backend silently no-ops for volumes without local-fs
  // access (MTP, virtual git portal), so no skip logic is needed here.
  //
  // Trailing-edge debounce ~100 ms: holding ArrowDown shouldn't fire `reloadData`
  // 60×/s.
  $effect(() => {
    if (!quickLookState.isOpen) {
      // Panel closed → cancel any pending dispatch and forget the last-sent path
      // so re-opening on the same entry doesn't get suppressed by the dedupe.
      if (quickLookFollowTimer !== null) {
        clearTimeout(quickLookFollowTimer)
        quickLookFollowTimer = null
      }
      quickLookLastSentPath = undefined
      return
    }
    const pane = deps.getFocusedPane()
    const paneRef = deps.getPaneRef(pane)
    const path = paneRef?.getPathUnderCursor()
    const volId = deps.getPaneVolumeId(pane)
    // Bail when the pane isn't ready or the cursor isn't on a resolvable entry.
    // No path → don't reloadData with stale state; wait for the next $effect fire
    // once the entry resolves (FilePane fetches it on every cursorIndex change).
    if (!path || !volId) return
    // A file inside an archive has no real on-disk path, so Quick Look would blank
    // (the pane's `volId` is the writable parent drive, so the backend's non-local
    // no-op doesn't catch it). Skip it — the panel keeps its last valid preview,
    // matching the initial-open gate in the `file.quickLook` handler.
    if (pathInsideArchive(path)) return
    const generation = ++quickLookFollowGeneration
    if (quickLookFollowTimer !== null) clearTimeout(quickLookFollowTimer)
    quickLookFollowTimer = setTimeout(() => {
      quickLookFollowTimer = null
      // Stale-generation check: a newer cursor move bumped the generation
      // while this timer was waiting. Drop this fire — the newer one will
      // schedule its own.
      if (generation !== quickLookFollowGeneration) return
      // Panel could have closed during the debounce window. Skip the IPC.
      if (!quickLookState.isOpen) return
      // Skip if the path hasn't actually changed since the last dispatch:
      // a focused-pane setCursorIndex during nav can fire the $effect with
      // the same entry resolved (debounced re-fetch lands later). Cheap to
      // dedupe; backend's `reloadData` is fine but the round-trip isn't free.
      if (path === quickLookLastSentPath) return
      quickLookLastSentPath = path
      void quickLookSetPath(path, volId).catch((e: unknown) => {
        log.warn('quickLookSetPath failed: {error}', { error: String(e) })
      })
    }, QUICK_LOOK_FOLLOW_DEBOUNCE_MS)
  })

  // Quick Look error-state close: when the focused pane transitions into
  // an error state (volume unmounted, listing failed) while the panel is open,
  // dismiss the panel. Sitting on a stale path while the underlying volume is
  // gone is worse UX than just closing — and the cursor-follow effect above
  // would otherwise be stuck on the last-known path until the user moves
  // focus or navigates somewhere reachable.
  $effect(() => {
    if (!quickLookState.isOpen) return
    const paneRef = deps.getPaneRef(deps.getFocusedPane())
    // `isInErrorState` reads two `$state` fields under the hood
    // (`friendlyError`, `unreachable`), so Svelte tracks them through this
    // call and we re-run when either flips. Don't bother destructuring —
    // the call site is the dependency.
    if (paneRef?.isInErrorState()) {
      closeFromPaneError()
    }
  })

  return {
    cleanup: () => {
      if (quickLookFollowTimer !== null) clearTimeout(quickLookFollowTimer)
    },
  }
}
