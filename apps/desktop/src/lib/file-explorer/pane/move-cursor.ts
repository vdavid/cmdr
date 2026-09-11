/**
 * `moveCursor`'s body, split out of `DualPaneExplorer.svelte` to keep it under
 * its length cap: pure orchestration logic over a `FilePaneAPI`, so a plain
 * `.ts` helper (not a `*.svelte.ts` factory) — nothing here is `$state` or
 * `$derived`.
 */
import { pluralize } from '$lib/utils/pluralize'
import type { FilePaneAPI } from './types'

export interface MoveCursorDeps {
  getPaneRef: (pane: 'left' | 'right') => FilePaneAPI | undefined
  setFocusedPane: (pane: 'left' | 'right') => void
  moveCursorByName: (paneRef: FilePaneAPI, name: string) => Promise<boolean>
  focusContainer: () => void
}

/**
 * Move cursor to a specific index or filename.
 * Used by MCP move_cursor tool.
 *
 * Throws when the target doesn't exist (filename not in the listing, index out
 * of range, pane unavailable). The `cursor.moveTo` dispatch awaits this, so the
 * exception reaches the MCP adapter's try/catch and the tool reports the real
 * failure instead of a false-positive "OK: Moved cursor".
 */
export async function moveCursorToTarget(pane: 'left' | 'right', to: number | string, deps: MoveCursorDeps) {
  deps.setFocusedPane(pane)
  const paneRef = deps.getPaneRef(pane)
  if (!paneRef) throw new Error(`The ${pane} pane is unavailable`)

  // Wait for the pane's current load (if any) to settle before touching
  // the listing. Without this, an MCP-driven `move_cursor` that lands
  // mid-navigation reads the FE's freshly-assigned `listingId` while the
  // backend's `LISTING_CACHE` insert is still in flight, surfacing as
  // "Listing not found" from `find_file_index`.
  await paneRef.whenLoadSettles()

  if (typeof to === 'number') {
    // `setCursorIndex` stores the value unclamped, so range-check first. A
    // network view counts its own rows (hosts or shares, `0` while a mount runs
    // or its failure is on screen) rather than the file listing; without that
    // count the check was skipped there and the host browser silently CLAMPED
    // an out-of-range index, so the tool reported a move it hadn't made.
    const total = paneRef.isInNetworkView() ? paneRef.getNetworkItemCount() : paneRef.getEffectiveTotalCount()
    if (to < 0 || to >= total) {
      throw new Error(
        `Index ${String(to)} is out of range in the ${pane} pane (${String(total)} ${pluralize(total, 'item')})`,
      )
    }
    await paneRef.setCursorIndex(to)
  } else {
    const found = await deps.moveCursorByName(paneRef, to)
    if (!found) {
      throw new Error(`"${to}" not found in the ${pane} pane listing`)
    }
  }
  // MCP-driven cursor placement: re-anchor DOM focus on the explorer container
  // so the next keystroke (the agent often follows move_cursor with a shortcut)
  // lands in the right dispatcher chain. Also makes the awaited completion
  // genuine; `void` swallowed the cursor-set promise and let MCP report `OK`
  // before the cursor was observably positioned.
  deps.focusContainer()

  // Flush the new cursor position to the backend's PaneStateStore BEFORE the
  // round-trip replies ok, so a follow-up tool call (move_cursor → copy/move/
  // delete) reads fresh state. Without this, the cursor lives only in FE state
  // until the debounced pane→MCP sync fires; the immediately-following file-op
  // runs `check_operation_has_target` against a stale store (cursor still on
  // `..`) and rejects with "Nothing to copy". This mirrors `select`, which
  // flushes for the same reason (see pane-commands.ts handleMcpSelect*). Not a
  // per-keystroke path — keyboard cursor moves use `setCursorIndex` directly via
  // handleKeyDown, never this exported MCP/search entry.
  await paneRef.syncStateToMcpNow()
}
