/**
 * Cursor movement for the regular (Brief / Full) list views: arrow keys, Page
 * Up/Down, Home/End, and the Shift-extend keyboard selection. Lifted out of
 * `FilePane.svelte`; the pure per-view step math lives in
 * `../navigation/keyboard-shortcuts` and the list components, so this factory is
 * the thin glue that turns a keystroke into a cursor move + scroll + selection
 * fill. `applyNavigation` stays public because FilePane also calls it from
 * `toggleSelectionAndMoveDownAtCursor`.
 */

import { handleNavigationShortcut } from '../navigation/keyboard-shortcuts'
import { comboMatchesCommand } from '$lib/shortcuts'
import { formatKeyCombo } from '$lib/shortcuts/key-capture'
import type { CommandId } from '$lib/commands'
import type { ListViewAPI } from './types'

/**
 * Every command whose key moves the file-list cursor, across both view modes. The
 * fixed-key six (`nav.up`/`down`/`left`/`right`/`firstInFull`/`lastInFull`) can't be
 * rebound; the paged four can, and this list is how a rebind reaches the cursor.
 */
const cursorCommands = [
  'nav.up',
  'nav.down',
  'nav.left',
  'nav.right',
  'nav.firstInFull',
  'nav.lastInFull',
  'nav.home',
  'nav.end',
  'nav.pageUp',
  'nav.pageDown',
] as const satisfies readonly CommandId[]

/**
 * Whether this keypress is a cursor move, matched on the WHOLE combo. Without it a
 * bare `e.key === 'ArrowDown'` test also fires for `⌘↓` (open), `⌥↓` (go to end), and
 * every other modifier superset, so the pane would move the cursor on its way to
 * running a completely different command.
 *
 * Shift is allowed because the file list uses it to extend the selection while the
 * cursor moves (`⇧↓`); the per-view handlers below read `e.shiftKey` for that fill.
 */
function isCursorKey(event: KeyboardEvent): boolean {
  const combo = formatKeyCombo(event)
  return cursorCommands.some((commandId) => comboMatchesCommand(combo, commandId, { allowShift: true }))
}

/** The minimal scroll target `applyNavigation` needs (a list view or any scrollable). */
export interface ScrollTarget {
  scrollToIndex: (index: number) => void
}

/** Toggle-and-fill keyboard selection args, across a jump from `fromIndex` to `toIndex`. */
export interface ExtendSelectionArgs {
  fromIndex: number
  toIndex: number
  overflow: boolean
  hasParent: boolean
}

export interface CursorNavKeysDeps {
  getCursorIndex: () => number
  /** Commit a new cursor index (the component owns the `cursorIndex` $state). */
  applyCursor: (index: number) => void
  /** Toggle-and-fill keyboard selection across a jump. */
  extendSelection: (args: ExtendSelectionArgs) => void
  getHasParent: () => boolean
  /** Total cursor-addressable rows (includes the `..` row). */
  getEffectiveTotalCount: () => number
  getBriefListRef: () => ListViewAPI | undefined
  getFullListRef: () => ListViewAPI | undefined
}

/**
 * Args for `applyNavigation`: land the cursor on `newIndex`. `overflow` (intended
 * jump clamped at a boundary) decides whether the landing item is included in the
 * Shift range fill.
 */
export interface ApplyNavigationArgs {
  newIndex: number
  listRef: ScrollTarget | undefined
  shiftKey?: boolean
  overflow?: boolean
}

export interface CursorNavKeys {
  /**
   * Land the cursor on `newIndex`: fill the selection on Shift, commit the index,
   * and scroll it into view.
   */
  applyNavigation: (args: ApplyNavigationArgs) => void
  /** Handle a keydown in Brief mode. Returns true if the key moved the cursor. */
  handleBriefModeKeys: (e: KeyboardEvent) => boolean
  /** Handle a keydown in Full mode. Returns true if the key moved the cursor. */
  handleFullModeKeys: (e: KeyboardEvent) => boolean
}

export function createCursorNavKeys(deps: CursorNavKeysDeps): CursorNavKeys {
  function applyNavigation({ newIndex, listRef, shiftKey = false, overflow = false }: ApplyNavigationArgs): void {
    if (shiftKey) {
      deps.extendSelection({
        fromIndex: deps.getCursorIndex(),
        toIndex: newIndex,
        overflow,
        hasParent: deps.getHasParent(),
      })
    }
    deps.applyCursor(newIndex)
    listRef?.scrollToIndex(newIndex)
    // fetchEntryUnderCursor is handled by the $effect tracking cursorIndex
  }

  function handleBriefModeKeys(e: KeyboardEvent): boolean {
    if (!isCursorKey(e)) return false
    const briefListRef = deps.getBriefListRef()
    const result = briefListRef?.handleKeyNavigation?.(e.key, e)
    if (result !== undefined) {
      e.preventDefault()
      applyNavigation({
        newIndex: result.newIndex,
        listRef: briefListRef,
        shiftKey: e.shiftKey,
        overflow: result.overflow,
      })
      return true
    }
    return false
  }

  function handleFullModeKeys(e: KeyboardEvent): boolean {
    if (!isCursorKey(e)) return false
    const fullListRef = deps.getFullListRef()
    const cursorIndex = deps.getCursorIndex()
    const effectiveTotalCount = deps.getEffectiveTotalCount()
    const visibleItems: number = fullListRef?.getVisibleItemsCount?.() ?? 20
    const shortcutResult = handleNavigationShortcut(e, {
      currentIndex: cursorIndex,
      totalCount: effectiveTotalCount,
      visibleItems,
    })
    if (shortcutResult) {
      e.preventDefault()
      applyNavigation({
        newIndex: shortcutResult.newIndex,
        listRef: fullListRef,
        shiftKey: e.shiftKey,
        overflow: shortcutResult.overflow,
      })
      return true
    }

    // Handle arrow navigation. Overflow = the step was clamped at a boundary.
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      const newIndex = Math.min(cursorIndex + 1, effectiveTotalCount - 1)
      applyNavigation({ newIndex, listRef: fullListRef, shiftKey: e.shiftKey, overflow: newIndex === cursorIndex })
      return true
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault()
      const newIndex = Math.max(cursorIndex - 1, 0)
      applyNavigation({ newIndex, listRef: fullListRef, shiftKey: e.shiftKey, overflow: newIndex === cursorIndex })
      return true
    }
    // Left/Right arrows jump to first/last (same as Brief mode at boundaries).
    // These always overflow: intended distance = infinity.
    if (e.key === 'ArrowLeft') {
      e.preventDefault()
      applyNavigation({ newIndex: 0, listRef: fullListRef, shiftKey: e.shiftKey, overflow: true })
      return true
    }
    if (e.key === 'ArrowRight') {
      e.preventDefault()
      applyNavigation({ newIndex: effectiveTotalCount - 1, listRef: fullListRef, shiftKey: e.shiftKey, overflow: true })
      return true
    }
    return false
  }

  return { applyNavigation, handleBriefModeKeys, handleFullModeKeys }
}
