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
 *   - P3: Left / Right jump to the first / last row, like Full view
 *   - P4: Space toggles selection at cursor (already wired in round 1)
 *   - P5: Shift+Up / Shift+Down extends selection
 *   - P7: F3 (view) / F4 (edit) route into the file action commands
 *
 * Cmd+A is handled by the document-level command dispatch (see `command-dispatch.ts`), not here:
 * it's a unified app shortcut, and the snapshot pane already exposes `selectAll` via `FilePane`.
 */

import { eventMatchesCommand } from '#lib/shortcuts/index.js'

/** Each action the search-pane key dispatch can request from the caller. */
export type SearchPaneKeyAction =
  | { kind: 'move-cursor'; index: number; overflow: boolean; shiftKey: boolean }
  | { kind: 'open-cursor' }
  | { kind: 'reveal-cursor' }
  | { kind: 'toggle-selection-at-cursor' }
  | { kind: 'toggle-selection-and-advance' }
  | { kind: 'view-file' }
  | { kind: 'edit-file' }

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
 * Discrete actions (Enter, F3, F4, Insert) that don't depend on counts or modifiers. Split out from the main dispatch to keep cyclomatic complexity in check.
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
  // Left / Right jump to the first / last row, the same as Home / End and as Full view.
  if (key === 'Home' || key === 'ArrowLeft') {
    return { kind: 'move-cursor', index: 0, overflow: true, shiftKey }
  }
  if (key === 'End' || key === 'ArrowRight') {
    return { kind: 'move-cursor', index: Math.max(0, count - 1), overflow: true, shiftKey }
  }
  return null
}

/**
 * Returns the action the snapshot pane should take for a given keyboard event, or `null` when
 * the event isn't one we handle. Any action means we ARE handling the key: the caller
 * (`FilePane.handleSearchResultsKeyDown`) calls `preventDefault` + `stopPropagation` so it doesn't
 * also reach the regular pane handler. `null` means leave it alone.
 *
 * Modifier semantics:
 *   - Cmd / Ctrl modify behavior elsewhere (Cmd+A select all, Cmd+arrow back/forward) and so are
 *     rejected here, so we don't accidentally eat them.
 *   - Shift on the arrows / PageUp / PageDown / Home / End extends the selection.
 *   - Alt+Up / Alt+Down mirror Home / End (per the full-pane navigation convention).
 */
export function computeSearchPaneKeyAction(
  event: KeyboardEvent,
  ctx: SearchPaneKeyContext,
): SearchPaneKeyAction | null {
  const { key, shiftKey, metaKey, ctrlKey, altKey } = event

  // Delete wins if customization ever puts both commands on one combo. In
  // particular, the default Cmd+Backspace must keep reaching the delete dispatcher.
  if (eventMatchesCommand(event, 'file.delete')) return null

  // Snapshot panes have no `..` row, so the configured parent action reveals the
  // result instead. Match before rejecting modified combos so the default Cmd+Up
  // binding works.
  if (eventMatchesCommand(event, 'nav.parent')) return { kind: 'reveal-cursor' }

  // Cmd / Ctrl combos belong to the unified dispatch layer. Don't shadow them here.
  if (metaKey || ctrlKey) return null

  // Space (unshifted, no other modifiers): toggle selection at cursor.
  if (key === ' ' && !shiftKey && !altKey) {
    return { kind: 'toggle-selection-at-cursor' }
  }

  return discreteAction(key) ?? moveAction(key, altKey, shiftKey, ctx)
}
