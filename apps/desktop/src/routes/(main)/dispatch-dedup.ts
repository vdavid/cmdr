/**
 * Cross-source double-fire dedup for the command dispatch core.
 *
 * On macOS a key combo that matches a native menu accelerator can fire TWICE:
 * AppKit runs the menu item (Rust emits `execute-command` → the menu listener)
 * AND the keydown can leak to the webview, where the centralized shortcut
 * dispatch fires the same command. Observed empirically for `file.quickLook`
 * (see `lib/shortcuts/CLAUDE.md` § "Modifier-key accelerators may fire twice").
 * Harmless for idempotent commands, but every toggle is a trap.
 *
 * The guard is keyed on the SOURCE PAIR, not just a time window: the two
 * spurious fires arrive milliseconds apart from DIFFERENT sources (keyboard +
 * menu), while genuine rapid input (double-press, key auto-repeat, two menu
 * clicks) is same-source and always passes. That removes the classic time-window
 * failure mode of swallowing real input. Untagged dispatches (palette, MCP,
 * cross-window) never participate.
 */

export type DispatchSource = 'keyboard' | 'menu'

/**
 * The two spurious fires of one keypress arrive within a few ms of each other
 * (same event-loop turn plus one Tauri event hop). 300ms is generous headroom
 * for a slow main thread; a human can't press the key AND click the menu item
 * for the same command inside it.
 */
const CROSS_SOURCE_DEDUP_WINDOW_MS = 300

let pendingSource: DispatchSource | null = null
let lastTaggedFire: { commandId: string; source: DispatchSource; at: number } | null = null

/**
 * Tags the NEXT dispatch with its source. Called by the two callers that can
 * double-fire one keypress: the centralized keyboard dispatch (`'keyboard'`)
 * and the `execute-command` menu listener (`'menu'`). The tag is consumed by
 * exactly one `shouldDropCrossSourceDuplicate` call.
 */
export function markDispatchSource(source: DispatchSource): void {
  pendingSource = source
}

/**
 * Consumes the pending source tag and reports whether this dispatch is the
 * spurious second half of a keyboard+menu pair: same command id, the OTHER
 * source, within the window. A dropped fire doesn't refresh the window, so a
 * genuine later fire passes once the original window expires.
 *
 * `now` is injectable for tests; production callers omit it.
 */
export function shouldDropCrossSourceDuplicate(commandId: string, now: number = Date.now()): boolean {
  const source = pendingSource
  pendingSource = null
  if (source === null) return false

  const isDuplicate =
    lastTaggedFire !== null &&
    lastTaggedFire.commandId === commandId &&
    lastTaggedFire.source !== source &&
    now - lastTaggedFire.at < CROSS_SOURCE_DEDUP_WINDOW_MS

  if (!isDuplicate) {
    lastTaggedFire = { commandId, source, at: now }
  }
  return isDuplicate
}

/** Test-only: clear the tag and pairing state between tests. */
export function _resetDedupForTests(): void {
  pendingSource = null
  lastTaggedFire = null
}
