/**
 * The rail's in-chat find: ⌘F while the rail has focus opens a find bar over the open
 * conversation (`AskCmdrFind.svelte` draws it with the house `FindBar`).
 *
 * - **Keys**: the fixed `askCmdr.find` / `askCmdr.findNext` / `askCmdr.findPrevious`
 *   commands, heard on the rail's own element, so they fire only while focus is in the rail
 *   (the rail-focused region, `explorerState.getRailFocused()`) and are CLAIMED there: the
 *   document dispatcher never sees ⌘F or ⌘G, so Search files and Go to path keep those keys
 *   everywhere else. ⌘G / ⇧⌘G step from anywhere in the rail while the bar is up; Enter /
 *   ⇧Enter and Escape only from the find field.
 * - **What it searches**: the rendered text of the thread's `data-find-region`s (the user's
 *   bubbles, the assistant's prose, tool-line and group labels), through `findTextRanges`.
 *   Never message data turned into HTML, so the XSS boundary (`renderAssistantMarkdown`)
 *   stays the only HTML door. Only LOADED messages: an older page is one "Load earlier"
 *   away, and the bar says "Searching loaded messages" while one exists.
 * - **Streaming**: ranges die when their nodes re-render, so a `MutationObserver` on the
 *   thread re-runs the find on every content change (once per frame), keeping the current
 *   match's index and never scrolling: only the person's own step or typing moves the view.
 */

import { tick } from 'svelte'
import { claimKey } from '#lib/shortcuts/claim-key.js'
import { eventMatchesCommand } from '#lib/shortcuts/index.js'
import { captureFocusOrigin, restoreFocus, type FocusOrigin } from '#lib/ui/focus-restore.js'
import { stepMatchIndex, type FindDirection } from '#lib/ui/find-bar.js'
import {
  clearFindMatches,
  findTextRanges,
  firstRangeFrom,
  paintFindMatches,
  scrollRangeIntoView,
} from '#lib/ui/find-in-dom.js'

/** Typing settles this long before the thread is searched, so a burst of keys searches once. */
export const FIND_TYPING_DEBOUNCE_MS = 120

interface RailFindDeps {
  /** The rail's root, focus's fallback when the element it came from is gone. */
  getRail: () => HTMLElement | null
  /** The scrolling thread: what's searched, and what scrolls to a match. */
  getThread: () => HTMLElement | null
}

export function createRailFind(deps: RailFindDeps) {
  let open = $state(false)
  let query = $state('')
  let total = $state(0)
  let current = $state(-1)
  let inputElement = $state<HTMLInputElement | undefined>(undefined)
  let barElement: HTMLElement | null = null

  let ranges: Range[] = []
  let origin: FocusOrigin | null = null
  let typingTimer: ReturnType<typeof setTimeout> | null = null
  let frame: number | null = null

  function paint(): void {
    const thread = deps.getThread()
    if (thread) paintFindMatches(thread, ranges, current)
  }

  function scrollToCurrent(): void {
    const thread = deps.getThread()
    const range = ranges.at(current)
    if (thread && current >= 0 && range) scrollRangeIntoView(thread, range)
  }

  /**
   * Re-runs the find. `fromView` (a new query) picks the first match at or below the top of
   * the thread's view and scrolls to it; otherwise (the content changed) the current index
   * holds, clamped, and nothing scrolls.
   */
  function refresh(fromView: boolean): void {
    const thread = deps.getThread()
    ranges = thread && open ? findTextRanges(thread, query) : []
    total = ranges.length
    if (total === 0) current = -1
    else if (fromView || current < 0) current = firstRangeFrom(ranges, thread?.getBoundingClientRect().top ?? 0)
    else current = Math.min(current, total - 1)
    paint()
    if (fromView) scrollToCurrent()
  }

  function setQuery(next: string): void {
    query = next
    if (typingTimer !== null) clearTimeout(typingTimer)
    typingTimer = setTimeout(() => {
      typingTimer = null
      refresh(true)
    }, FIND_TYPING_DEBOUNCE_MS)
  }

  /** Opens the bar (or, when it's up, puts the cursor back in its field) with the query selected. */
  function openFind(): void {
    if (!open) {
      origin = captureFocusOrigin()
      open = true
    }
    void tick().then(() => {
      inputElement?.focus()
      inputElement?.select()
    })
  }

  /** Closes the bar, clears the highlights, and hands focus back to where it was in the rail. */
  function closeFind(): void {
    if (!open) return
    if (typingTimer !== null) clearTimeout(typingTimer)
    typingTimer = null
    restoreFocus(origin, { leaving: barElement })
    const active = document.activeElement
    if (active === null || active === document.body || barElement?.contains(active)) {
      deps.getRail()?.focus({ preventScroll: true })
    }
    origin = null
    open = false
    query = ''
    ranges = []
    total = 0
    current = -1
    clearFindMatches(deps.getThread())
  }

  function step(direction: FindDirection): void {
    // A step right after typing searches now, rather than stepping through stale ranges.
    if (typingTimer !== null) {
      clearTimeout(typingTimer)
      typingTimer = null
      refresh(true)
      return
    }
    current = stepMatchIndex(current, total, direction)
    paint()
    scrollToCurrent()
  }

  /**
   * Which way a key steps, if it's a step the find answers here: ⌘G / ⇧⌘G from anywhere in
   * the rail, Enter / ⇧Enter only from the field (elsewhere they're the composer's or a
   * button's).
   */
  function stepOf(event: KeyboardEvent, inField: boolean): FindDirection | null {
    if (!inField && !event.metaKey && !event.ctrlKey) return null
    if (eventMatchesCommand(event, 'askCmdr.findNext')) return 'next'
    if (eventMatchesCommand(event, 'askCmdr.findPrevious')) return 'previous'
    return null
  }

  function isBareEscape(event: KeyboardEvent): boolean {
    return event.key === 'Escape' && !event.metaKey && !event.ctrlKey && !event.altKey && !event.shiftKey
  }

  /**
   * The rail's keydown: every key the find answers, claimed so the document dispatcher
   * (Search files on ⌘F, Go to path on ⌘G) never runs it too.
   */
  function handleKeydown(event: KeyboardEvent): void {
    if (event.isComposing) return
    if (eventMatchesCommand(event, 'askCmdr.find')) {
      claimKey(event)
      openFind()
      return
    }
    if (!open) return
    const inField = inputElement !== undefined && event.target === inputElement
    if (inField && isBareEscape(event)) {
      claimKey(event)
      closeFind()
      return
    }
    const direction = stepOf(event, inField)
    if (direction === null) return
    claimKey(event)
    step(direction)
  }

  /** Follows the thread's content while the bar is up. Returns the stop. */
  function observe(thread: HTMLElement): () => void {
    const observer = new MutationObserver(() => {
      if (frame !== null) return
      frame = requestAnimationFrame(() => {
        frame = null
        if (open && query.trim() !== '') refresh(false)
      })
    })
    observer.observe(thread, { childList: true, subtree: true, characterData: true })
    return () => {
      observer.disconnect()
      if (frame !== null) cancelAnimationFrame(frame)
      frame = null
    }
  }

  function destroy(): void {
    if (typingTimer !== null) clearTimeout(typingTimer)
    typingTimer = null
    clearFindMatches(deps.getThread())
  }

  return {
    get open() {
      return open
    },
    get query() {
      return query
    },
    set query(next: string) {
      setQuery(next)
    },
    get total() {
      return total
    },
    /** 1-based, 0 when there's no current match. */
    get position() {
      return current + 1
    },
    get inputElement() {
      return inputElement
    },
    set inputElement(next: HTMLInputElement | undefined) {
      inputElement = next
    },
    setBarElement(next: HTMLElement | null) {
      barElement = next
    },
    openFind,
    closeFind,
    next: () => {
      step('next')
    },
    previous: () => {
      step('previous')
    },
    handleKeydown,
    observe,
    destroy,
  }
}
