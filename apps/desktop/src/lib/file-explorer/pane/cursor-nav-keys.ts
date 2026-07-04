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
import type { ListViewAPI } from './types'

/** The minimal scroll target `applyNavigation` needs (a list view or any scrollable). */
export interface ScrollTarget {
  scrollToIndex: (index: number) => void
}

export interface CursorNavKeysDeps {
  getCursorIndex: () => number
  /** Commit a new cursor index (the component owns the `cursorIndex` $state). */
  applyCursor: (index: number) => void
  /** Toggle-and-fill keyboard selection across a jump. */
  extendSelection: (fromIndex: number, toIndex: number, overflow: boolean, hasParent: boolean) => void
  getHasParent: () => boolean
  /** Total cursor-addressable rows (includes the `..` row). */
  getEffectiveTotalCount: () => number
  getBriefListRef: () => ListViewAPI | undefined
  getFullListRef: () => ListViewAPI | undefined
}

export interface CursorNavKeys {
  /**
   * Land the cursor on `newIndex`: fill the selection on Shift, commit the index,
   * and scroll it into view. `overflow` (intended jump clamped at a boundary)
   * decides whether the landing item is included in the range fill.
   */
  applyNavigation: (newIndex: number, listRef: ScrollTarget | undefined, shiftKey?: boolean, overflow?: boolean) => void
  /** Handle a keydown in Brief mode. Returns true if the key moved the cursor. */
  handleBriefModeKeys: (e: KeyboardEvent) => boolean
  /** Handle a keydown in Full mode. Returns true if the key moved the cursor. */
  handleFullModeKeys: (e: KeyboardEvent) => boolean
}

export function createCursorNavKeys(deps: CursorNavKeysDeps): CursorNavKeys {
  function applyNavigation(
    newIndex: number,
    listRef: ScrollTarget | undefined,
    shiftKey = false,
    overflow = false,
  ): void {
    if (shiftKey) {
      deps.extendSelection(deps.getCursorIndex(), newIndex, overflow, deps.getHasParent())
    }
    deps.applyCursor(newIndex)
    listRef?.scrollToIndex(newIndex)
    // fetchEntryUnderCursor is handled by the $effect tracking cursorIndex
  }

  /**
   * `⌘←` / `⌘→` belong to "Copy path between panes" (document-level dispatch).
   * Bail so the local pane handlers don't also move the cursor when those
   * shortcuts fire. Other modifier + arrow combos keep their existing behavior.
   */
  function isShortcutModifierArrow(e: KeyboardEvent): boolean {
    if (!e.metaKey) return false
    return e.key === 'ArrowLeft' || e.key === 'ArrowRight'
  }

  function handleBriefModeKeys(e: KeyboardEvent): boolean {
    if (isShortcutModifierArrow(e)) return false
    const briefListRef = deps.getBriefListRef()
    const result = briefListRef?.handleKeyNavigation?.(e.key, e)
    if (result !== undefined) {
      e.preventDefault()
      applyNavigation(result.newIndex, briefListRef, e.shiftKey, result.overflow)
      return true
    }
    return false
  }

  function handleFullModeKeys(e: KeyboardEvent): boolean {
    if (isShortcutModifierArrow(e)) return false
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
      applyNavigation(shortcutResult.newIndex, fullListRef, e.shiftKey, shortcutResult.overflow)
      return true
    }

    // Handle arrow navigation. Overflow = the step was clamped at a boundary.
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      const newIndex = Math.min(cursorIndex + 1, effectiveTotalCount - 1)
      applyNavigation(newIndex, fullListRef, e.shiftKey, newIndex === cursorIndex)
      return true
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault()
      const newIndex = Math.max(cursorIndex - 1, 0)
      applyNavigation(newIndex, fullListRef, e.shiftKey, newIndex === cursorIndex)
      return true
    }
    // Left/Right arrows jump to first/last (same as Brief mode at boundaries).
    // These always overflow: intended distance = infinity.
    if (e.key === 'ArrowLeft') {
      e.preventDefault()
      applyNavigation(0, fullListRef, e.shiftKey, true)
      return true
    }
    if (e.key === 'ArrowRight') {
      e.preventDefault()
      applyNavigation(effectiveTotalCount - 1, fullListRef, e.shiftKey, true)
      return true
    }
    return false
  }

  return { applyNavigation, handleBriefModeKeys, handleFullModeKeys }
}
