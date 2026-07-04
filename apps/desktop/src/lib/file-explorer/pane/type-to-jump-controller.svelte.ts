/**
 * Type-to-jump controller for a file pane: wraps the buffer/indicator state
 * (`createTypeToJumpState`) together with the IPC fuzzy-match runner and the MCP
 * mirror of the last matched name. Lifted out of `FilePane.svelte`, which keeps
 * one-line `handleJumpKeystroke` / `isJumpActive` / `clearJumpState` delegates
 * (the `FilePaneAPI` surface DualPaneExplorer drives).
 *
 * The match runner's generation tag discards out-of-order IPC responses (a slow
 * keystroke resolving after a faster later one); it also re-checks the buffer and
 * listing id so a match arriving after a clear / listing swap is dropped. The
 * frontend cursor index adds the `..` offset when the pane has a parent row.
 */

import { findFirstFuzzyMatch, getFileAt } from '$lib/tauri-commands'
import { getIpcErrorMessage } from '$lib/tauri-commands/ipc-types'
import { createTypeToJumpState } from './type-to-jump-state.svelte'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('fileExplorer')

export interface TypeToJumpControllerDeps {
  /** Current buffer-reset timeout (read live so a Settings change takes effect next keystroke). */
  getResetMs: () => number
  getListingId: () => string
  getLoading: () => boolean
  /** Whether the pane's volume kind has a real backend listing to jump within. */
  getHasBackendListing: () => boolean
  /** The MTP not-yet-connected sub-state (a connected MTP pane jumps fine). */
  getIsMtpDeviceOnly: () => boolean
  getIncludeHidden: () => boolean
  getHasParent: () => boolean
  /** Move the cursor to a frontend index (scrolls + syncs MCP). */
  setCursorIndex: (index: number) => void
  /** Push pane state to MCP (debounced by the caller). */
  onSyncMcp: () => void
}

export interface TypeToJumpController {
  /** Current buffer (what the user has typed since the last reset). */
  readonly buffer: string
  /** Whether the "Jump: …" indicator is shown. */
  readonly indicatorVisible: boolean
  /** Whether the indicator is in its "stale" (buffer-reset-but-still-shown) state. */
  readonly indicatorStale: boolean
  /** Name the most recent successful match landed on (for the MCP mirror), or null. */
  readonly lastMatchedName: string | null
  /** Handle one keystroke: append + fire the match (no-op when there's nothing to jump within). */
  handleJumpKeystroke: (char: string) => void
  /** True while a jump is active (buffer non-empty before the reset timeout empties it). */
  isJumpActive: () => boolean
  /** Clear the buffer + indicator + timers only (does NOT touch the last-matched name). */
  clear: () => void
  /** Clear the buffer AND the last-matched name (+ MCP sync). The `FilePaneAPI` `clearJumpState`. */
  clearJumpState: () => void
  /** Stop pending timers so they can't fire after the pane is gone. Call from `onDestroy`. */
  dispose: () => void
}

export function createTypeToJumpController(deps: TypeToJumpControllerDeps): TypeToJumpController {
  let lastMatchedName = $state<string | null>(null)

  const typeToJump = createTypeToJumpState({
    getResetMs: () => deps.getResetMs(),
    onMatch: (buffer, generation) => {
      void runJumpMatch(buffer, generation)
    },
    onIndicatorHide: () => {
      // Stale match info is meaningless once the indicator is gone.
      lastMatchedName = null
      deps.onSyncMcp()
    },
  })

  /**
   * Runs the IPC fuzzy match and applies the result if it's still fresh. The
   * generation tag guards against out-of-order responses (slow keystroke 1
   * resolving after fast keystroke 2, same pattern as `diffGeneration`).
   */
  async function runJumpMatch(buffer: string, generation: number): Promise<void> {
    const listingId = deps.getListingId()
    if (!listingId || buffer === '') return
    const capturedListingId = listingId
    const includeHidden = deps.getIncludeHidden()
    try {
      const backendIndex = await findFirstFuzzyMatch(capturedListingId, buffer, includeHidden)
      // Discard stale responses (newer keystroke fired) or responses arriving
      // after a buffer clear / listing swap.
      if (generation !== typeToJump.generation) return
      if (typeToJump.buffer === '') return
      if (capturedListingId !== deps.getListingId()) return
      if (backendIndex === null) return
      const frontendIndex = deps.getHasParent() ? backendIndex + 1 : backendIndex
      deps.setCursorIndex(frontendIndex)
      // Remember where the match landed so MCP can surface it. Use the entry from
      // the cache rather than the visible-range slice (the matched index may be
      // off-screen until the scroll catches up).
      try {
        const entry = await getFileAt(capturedListingId, backendIndex, includeHidden)
        if (entry && generation === typeToJump.generation) {
          lastMatchedName = entry.name
          deps.onSyncMcp()
        }
      } catch {
        // Cache lookup failure is non-fatal: MCP just lacks the name.
      }
    } catch (e) {
      log.warn('type-to-jump match failed: {error}', { error: getIpcErrorMessage(e) })
    }
  }

  function handleJumpKeystroke(char: string): void {
    // No real listing to jump within (network / search-results) folds into
    // `!getHasBackendListing()`. `getIsMtpDeviceOnly` STAYS: it's the MTP
    // not-yet-connected runtime sub-state, not a kind capability (a CONNECTED MTP
    // pane has a backend listing and jumps fine).
    if (!deps.getListingId() || deps.getLoading() || !deps.getHasBackendListing() || deps.getIsMtpDeviceOnly()) return
    typeToJump.appendChar(char)
    // Surface the buffer change to MCP (`runJumpMatch` syncs again on success, but
    // a no-match keystroke would otherwise leave MCP stale).
    deps.onSyncMcp()
  }

  function clearJumpState(): void {
    typeToJump.clear()
    // Clearing the buffer invalidates whatever the last match landed on.
    if (lastMatchedName !== null) {
      lastMatchedName = null
      deps.onSyncMcp()
    }
  }

  return {
    get buffer() {
      return typeToJump.buffer
    },
    get indicatorVisible() {
      return typeToJump.indicatorVisible
    },
    get indicatorStale() {
      return typeToJump.indicatorStale
    },
    get lastMatchedName() {
      return lastMatchedName
    },
    handleJumpKeystroke,
    isJumpActive: () => typeToJump.buffer.length > 0,
    clear: () => { typeToJump.clear(); },
    clearJumpState,
    dispose: () => { typeToJump.dispose(); },
  }
}
