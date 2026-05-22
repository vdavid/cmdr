/**
 * Pure key-dispatch helper for the search-results (snapshot) pane.
 *
 * Centralizes the keyboard contract that `FilePane.handleSearchResultsKeyDown` follows. Tested in
 * isolation so the snapshot pane's keyboard behavior is pinned without spinning up the entire
 * `FilePane` component. The `FilePane` wrapper translates the returned action into actual state
 * mutations (cursor moves, selection toggles, IPC calls).
 *
 * Covers David's round-2 P-series items:
 *   - P1: PgUp / PgDown
 *   - P2: Home / End
 *   - P3: Left / Right (intentional no-op for now in a flat snapshot)
 *   - P4: Space toggles selection at cursor (already wired in round 1)
 *   - P5: Shift+Up / Shift+Down extends selection
 *   - P7: F3 (view) / F4 (edit) route into the file action commands
 *
 * Cmd+A is handled by the document-level command dispatch (see `command-dispatch.ts`), not here:
 * it's a unified app shortcut, and the snapshot pane already exposes `selectAll` via `FilePane`.
 */

/** Each action the search-pane key dispatch can request from the caller. */
export type SearchPaneKeyAction =
  | { kind: 'move-cursor'; index: number; overflow: boolean; shiftKey: boolean }
  | { kind: 'open-cursor' }
  | { kind: 'toggle-selection-at-cursor' }
  | { kind: 'toggle-selection-and-advance' }
  | { kind: 'view-file' }
  | { kind: 'edit-file' }
  | { kind: 'noop' }

export interface SearchPaneKeyContext {
  /** Current cursor index inside the snapshot. */
  cursorIndex: number
  /** Snapshot entry count. Drives the clamp at both ends. */
  count: number
  /** Visible items in the list view, used for PageUp / PageDown step size. */
  visibleItems: number
}

/** Clamp a target index to `[0, count - 1]`. Returns `0` when the snapshot is empty. */
function clamp(target: number, count: number): number {
  if (count <= 0) return 0
  if (target < 0) return 0
  if (target >= count) return count - 1
  return target
}

/**
 * Compute the page step for PageUp / PageDown. Mirrors `handleFullPageNavigation` in
 * `navigation/keyboard-shortcuts.ts` -- `visibleItems - 1` so the eye keeps one row of context
 * across the jump, with a hard floor of 1.
 */
function pageStep(visibleItems: number): number {
  return Math.max(1, visibleItems - 1)
}

/**
 * Discrete actions (Enter, F3, F4, Insert, Left, Right) that don't depend on counts or
 * modifiers. Split out from the main dispatch to keep cyclomatic complexity in check.
 */
function discreteAction(key: string): SearchPaneKeyAction | null {
  switch (key) {
    case 'Enter':
      return { kind: 'open-cursor' }
    case 'F3':
      return { kind: 'view-file' }
    case 'F4':
      return { kind: 'edit-file' }
    case 'Insert':
      return { kind: 'toggle-selection-and-advance' }
    // Round-2 P3: a flat snapshot has no parent-folder semantics. Explicit no-op so the
    // outer dispatcher can call preventDefault + stopPropagation and not let the event
    // reach the full-pane handler (whose Left / Right jumps to first / last row).
    case 'ArrowLeft':
    case 'ArrowRight':
      return { kind: 'noop' }
    default:
      return null
  }
}

/**
 * Move-cursor actions for the arrow / page / home / end family. Returns `null` if the key
 * isn't one of those. `shiftKey` propagates so the caller can extend the selection across the
 * jump; `altKey` on Up / Down mirrors Home / End (per the full-pane convention).
 */
function moveAction(
  key: string,
  altKey: boolean,
  shiftKey: boolean,
  ctx: SearchPaneKeyContext,
): SearchPaneKeyAction | null {
  const { cursorIndex, count, visibleItems } = ctx

  if (altKey && (key === 'ArrowUp' || key === 'ArrowDown')) {
    const target = key === 'ArrowUp' ? 0 : Math.max(0, count - 1)
    return { kind: 'move-cursor', index: clamp(target, count), overflow: true, shiftKey }
  }
  if (key === 'ArrowDown') {
    const target = cursorIndex + 1
    return { kind: 'move-cursor', index: clamp(target, count), overflow: target >= count, shiftKey }
  }
  if (key === 'ArrowUp') {
    const target = cursorIndex - 1
    return { kind: 'move-cursor', index: clamp(target, count), overflow: target < 0, shiftKey }
  }
  if (key === 'PageDown') {
    const raw = cursorIndex + pageStep(visibleItems)
    return { kind: 'move-cursor', index: clamp(raw, count), overflow: raw >= count, shiftKey }
  }
  if (key === 'PageUp') {
    const raw = cursorIndex - pageStep(visibleItems)
    return { kind: 'move-cursor', index: clamp(raw, count), overflow: raw < 0, shiftKey }
  }
  if (key === 'Home') {
    return { kind: 'move-cursor', index: 0, overflow: true, shiftKey }
  }
  if (key === 'End') {
    return { kind: 'move-cursor', index: Math.max(0, count - 1), overflow: true, shiftKey }
  }
  return null
}

/**
 * Returns the action the snapshot pane should take for a given keyboard event, or `null` when
 * the event isn't one we handle. The caller (`FilePane.handleSearchResultsKeyDown`) decides what
 * to do with `'noop'` versus `null`: `'noop'` means we ARE handling the key (call `preventDefault`
 * + don't bubble to the regular pane handler), `null` means leave it alone.
 *
 * Modifier semantics:
 *   - Cmd / Ctrl modify behavior elsewhere (Cmd+A select all, Cmd+arrow back/forward) and so are
 *     rejected here, so we don't accidentally eat them.
 *   - Shift on ArrowUp / ArrowDown / PageUp / PageDown / Home / End extends the selection.
 *   - Alt+Up / Alt+Down mirror Home / End (per the full-pane navigation convention).
 */
export function computeSearchPaneKeyAction(
  event: { key: string; shiftKey: boolean; metaKey: boolean; ctrlKey: boolean; altKey: boolean },
  ctx: SearchPaneKeyContext,
): SearchPaneKeyAction | null {
  const { key, shiftKey, metaKey, ctrlKey, altKey } = event

  // Cmd / Ctrl combos belong to the unified dispatch layer. Don't shadow them here.
  if (metaKey || ctrlKey) return null

  // Space (unshifted, no other modifiers): toggle selection at cursor.
  if (key === ' ' && !shiftKey && !altKey) {
    return { kind: 'toggle-selection-at-cursor' }
  }

  return discreteAction(key) ?? moveAction(key, altKey, shiftKey, ctx)
}
