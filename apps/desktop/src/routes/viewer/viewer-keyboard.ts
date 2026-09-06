import { EOF_LINE, type LineOffset, type SelectAllArgs, type Selection } from './selection.svelte'
import { moveFocus, type CaretMotion, type MotionDirection } from './viewer-caret-motion'

/** Scroll actions the unmodified navigation keys drive. */
interface ScrollActions {
  scrollByLines: (n: number) => void
  scrollByPages: (n: number) => void
  scrollToStart: () => void
  scrollToEnd: () => void
  /** Horizontal scroll by `n` columns. A no-op under word wrap (nothing overflows). */
  scrollByColumns: (n: number) => void
}

/** Everything the keyboard asks of the scroll composable. */
interface NavigationActions extends ScrollActions {
  /** Scrolls a line just into view vertically, leaving an already-visible one alone. */
  ensureLineVisible: (line: number) => void
  /** Scrolls the character at `point` into view horizontally. */
  ensureColumnVisible: (point: LineOffset) => void
}

/** Maps Arrow / Page / Home / End keys to viewer scroll actions. Returns true if handled. */
export function handleNavigationKey(key: string, actions: ScrollActions): boolean {
  switch (key) {
    case 'ArrowUp':
      actions.scrollByLines(-1)
      return true
    case 'ArrowDown':
      actions.scrollByLines(1)
      return true
    case 'ArrowLeft':
      actions.scrollByColumns(-1)
      return true
    case 'ArrowRight':
      actions.scrollByColumns(1)
      return true
    case 'PageUp':
      actions.scrollByPages(-1)
      return true
    case 'PageDown':
      actions.scrollByPages(1)
      return true
    case 'Home':
      actions.scrollToStart()
      return true
    case 'End':
      actions.scrollToEnd()
      return true
    default:
      return false
  }
}

/**
 * The motion a pressed chord asks for, or `null` when it isn't an extend chord.
 *
 * The guard-then-branch shape is deliberate: switch on the key FIRST, then read the
 * modifiers in a separate statement, the way the Shift+Enter branch below already does.
 * ❌ Never fold the two together as `e.shiftKey && e.key === 'ArrowLeft'`. That pairs a
 * required modifier read with a literal key test while leaving ⌘/⌃/⌥ unconstrained, which
 * is the modifier-superset bug `cmdr/no-raw-key-match` (an ERROR here) exists for; its
 * header names this window's case, "in a window with no command registry (the viewer),
 * match locally but split on 'carries ⌘/⌃/⌥' up front".
 */
function extendMotionFor(e: KeyboardEvent): CaretMotion | null {
  switch (e.key) {
    case 'ArrowLeft':
      return horizontalMotion(e, -1)
    case 'ArrowRight':
      return horizontalMotion(e, 1)
    case 'ArrowUp':
      return verticalMotion(e, -1)
    case 'ArrowDown':
      return verticalMotion(e, 1)
    case 'Home':
      return lineEdgeMotion(e, -1)
    case 'End':
      return lineEdgeMotion(e, 1)
    default:
      return null
  }
}

/**
 * Shift+Arrow steps a character; ⌥ (macOS) or ⌃ (Linux, Windows) promotes it to a word.
 * macOS usually eats ⌃⇧Arrow at the system level, which costs nothing to support.
 */
function horizontalMotion(e: KeyboardEvent, direction: MotionDirection): CaretMotion | null {
  if (!e.shiftKey) return null
  if (e.metaKey) return null
  const byWord = e.altKey || e.ctrlKey
  return { kind: byWord ? 'word' : 'char', direction }
}

/** Shift+Up/Down steps a logical line; ⌘ promotes it to the file edge. */
function verticalMotion(e: KeyboardEvent, direction: MotionDirection): CaretMotion | null {
  if (!e.shiftKey) return null
  if (e.metaKey) return { kind: 'docEdge', direction }
  if (e.altKey || e.ctrlKey) return null
  return { kind: 'line', direction }
}

/**
 * Shift+Home/End extends to the LINE edge, while bare Home/End scroll to the file edge.
 * That difference is on purpose: unmodified Home/End are scroll-view navigation (what
 * macOS does in a document view), and a selection gesture works on the line in every
 * editor; ⌘ then promotes it back to the whole file. Don't harmonize them.
 */
function lineEdgeMotion(e: KeyboardEvent, direction: MotionDirection): CaretMotion | null {
  if (!e.shiftKey) return null
  if (e.metaKey || e.altKey || e.ctrlKey) return null
  return { kind: 'lineEdge', direction }
}

/** Handles single-letter toggles (word wrap on `W`). Returns true if handled. */
export function handleToggleKey(e: KeyboardEvent, toggleWordWrap: () => void): boolean {
  if (e.key.toLowerCase() === 'w' && !e.metaKey && !e.ctrlKey && !e.altKey) {
    toggleWordWrap()
    return true
  }
  return false
}

/**
 * Handles the tail-mode toggle on the unmodified `F` key. Returns true if
 * handled. Gated on no modifier so it can't collide with `⌘F` (open search)
 * or other chords.
 */
export function handleTailToggleKey(e: KeyboardEvent, toggleTailMode: () => void): boolean {
  if (e.key.toLowerCase() === 'f' && !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey) {
    toggleTailMode()
    return true
  }
  return false
}

interface SearchToggleActions {
  toggleUseRegex: () => void
  toggleCaseSensitive: () => void
}

/** Handles the search-mode chords:
 *  - Cmd+Alt+R (or Ctrl+Alt+R on non-mac): toggle regex
 *  - Cmd+Alt+C (or Ctrl+Alt+C on non-mac): toggle case-sensitivity
 *
 *  Returns true if handled. Caller is responsible for `preventDefault`.
 *
 *  The chord is gated on both meta/ctrl AND alt so it can't collide with the
 *  in-input shortcuts (Cmd+A = select all, Cmd+C = copy). */
export function handleSearchToggleKey(e: KeyboardEvent, actions: SearchToggleActions): boolean {
  const modKey = e.metaKey || e.ctrlKey
  if (!modKey || !e.altKey) return false
  const key = e.key.toLowerCase()
  if (key === 'r') {
    actions.toggleUseRegex()
    return true
  }
  if (key === 'c') {
    actions.toggleCaseSensitive()
    return true
  }
  return false
}

interface KeyboardDeps {
  /** Total line count, or `null` in ByteSeek-no-index mode before an index exists. */
  getTotalLines: () => number | null
  /** Total byte count of the file (drives the ByteSeek-no-index ⌘A fallback). */
  getTotalBytes: () => number
  /** Reads the cached text of a line, or `undefined` if not cached. */
  getLineText: (line: number) => string | undefined
  /**
   * The last line currently rendered, or `null` when nothing is. Only ever read to
   * resolve an end-of-file sentinel focus onto a real line (see `resolveFrom`).
   */
  getLastRenderedLine: () => number | null
  selection: {
    /** The current selection, so an extend chord has an anchor to keep and a focus to move. */
    readonly selection: Selection | null
    /** Selects the whole file given its total line count and the last line's length. */
    selectAll: (args: SelectAllArgs) => void
    /** Selects the whole file when its line count isn't known yet (ByteSeek, no index). */
    selectToEof: () => void
    /** Moves the focus, keeping the anchor. The whole of keyboard extension. */
    setFocus: (point: LineOffset) => void
  }
  scroll: NavigationActions
  search: {
    searchVisible: boolean
    searchStatus: string
    searchInputRef: HTMLInputElement | null | undefined
    openSearch: () => void
    closeSearch: () => void
    stopSearch: () => void
    findNext: () => void
    findPrev: () => void
    toggleUseRegex: () => void
    toggleCaseSensitive: () => void
  }
  copy: {
    busy: boolean
    cancelInFlight: () => Promise<void>
  }
  /** Whether a copy confirm dialog (10 to 100 MiB band) is currently showing. */
  isCopyConfirmOpen: () => boolean
  /** Whether the > 100 MiB copy refuse dialog is currently showing. */
  isCopyRefuseOpen: () => boolean
  /** Whether the in-app context menu is currently open. */
  isContextMenuOpen: () => boolean
  cancelCopyConfirm: () => void
  dismissCopyRefuse: () => void
  closeContextMenu: () => void
  /** Debug-logs the Escape press (search-visible / window-ready snapshot). */
  logEscape: () => void
  /** Runs the copy gesture (⌘C / context-menu Copy). */
  runCopy: () => void
  /** Toggles tail mode (unmodified `F`). */
  toggleTailMode: () => void
  /** Toggles word wrap (unmodified `W`). */
  toggleWordWrap: () => void
  /** Closes the viewer window (Escape with no other surface to consume it). */
  closeWindow: () => void
}

/**
 * Keyboard wiring for the viewer. Consolidates the page's keydown routing into one
 * place: the meta-shortcut handler, the Escape priority ladder, ⌘A select-all, and the
 * bare-key navigation/toggle dispatch. The page keeps a thin shim that binds
 * `handleKeyDown` to `<svelte:window on:keydown>` and passes the result of
 * `handleSelectAllShortcut` to the context menu's "Select all" action.
 *
 * Reads all reactive page state through getters/callbacks (the callback-based deps
 * pattern), so this stays a plain `.ts` module with no `$state` of its own.
 */
export function createViewerKeyboard(deps: KeyboardDeps) {
  /**
   * The column a run of vertical extend presses is aiming for, so walking down through a
   * short line and back returns to the original column. `moveFocus` clears it after any
   * non-`line` motion, so nothing here has to reset it; a pointer gesture does, through
   * `resetDesiredColumn`.
   */
  let desiredColumn: number | null = null

  /** Clears the vertical run's aim. The page calls this from its pointer-drag deps. */
  function resetDesiredColumn(): void {
    desiredColumn = null
  }

  /**
   * Where `moveFocus` starts from, with the end-of-file sentinel resolved away.
   *
   * ❌ `moveFocus` THROWS on a sentinel `from`, and this is reachable today: ⌘A in
   * ByteSeek-no-index mode parks the focus on `EOF_LINE`. The sentinel names a line that
   * can never be cached, so the refusal has to happen here rather than in the pure
   * module, which knows nothing about what's on screen and could only hand the sentinel
   * back as its own `targetLine` — slamming the view to the bottom on every press with no
   * way to shrink the selection. Reaching the sentinel always scrolled to the bottom, so
   * the last rendered line is the practical end of the file. Returns `null` (a no-op
   * press) when nothing is rendered.
   */
  function resolveFrom(focus: LineOffset): LineOffset | null {
    if (focus.line !== EOF_LINE) return focus
    const line = deps.getLastRenderedLine()
    if (line === null) return null
    return { line, offset: deps.getLineText(line)?.length ?? 0 }
  }

  /**
   * Scrolls to a motion's `targetLine`. `docEdge` down with no line count yet reports the
   * sentinel, which names no scrollable row: it means "the end of the file". ❌ Never
   * pass it to line arithmetic.
   */
  function scrollToTarget(targetLine: number): void {
    if (targetLine === EOF_LINE) deps.scroll.scrollToEnd()
    else deps.scroll.ensureLineVisible(targetLine)
  }

  /**
   * Runs an extend chord if the press is one. Returns `true` when the key was consumed,
   * which includes the deliberate no-ops: letting an unhandled Shift+Down fall through
   * would scroll the view out from under a selection the user is building.
   *
   * With no selection at all this does nothing: a plain click already leaves a collapsed
   * selection at the click point, so click-then-Shift+Arrow is the discoverable path,
   * while seeding an anchor off-screen would start a selection the user can't see.
   */
  function tryExtendSelection(e: KeyboardEvent): boolean {
    const motion = extendMotionFor(e)
    if (motion === null) return false

    const current = deps.selection.selection
    if (current === null) return true
    const from = resolveFrom(current.focus)
    if (from === null) return true

    const result = moveFocus({
      from,
      motion,
      getLineText: deps.getLineText,
      getTotalLines: deps.getTotalLines,
      desiredColumn,
    })
    desiredColumn = result.desiredColumn

    if (result.focus !== null) deps.selection.setFocus(result.focus)

    // Unconditional, so the uncached-line case heals itself: with no offset to land on,
    // the selection stays put and this scroll is what pulls the line into the render
    // window and triggers its fetch, so the next press lands instead of the key being
    // dead forever.
    scrollToTarget(result.targetLine)
    if (result.focus !== null && result.focus.line !== EOF_LINE) deps.scroll.ensureColumnVisible(result.focus)
    return true
  }

  function handleSelectAllShortcut(): void {
    // ⌘A is a fresh gesture, so a Shift+Up right after it aims from the new focus rather
    // than from whatever column an earlier run was heading for.
    resetDesiredColumn()
    const totalLines = deps.getTotalLines()
    if (totalLines !== null && totalLines > 0) {
      const lastLineText = deps.getLineText(totalLines - 1) ?? ''
      deps.selection.selectAll({ totalLines, lastLineLength: lastLineText.length })
      return
    }
    // ByteSeek-no-index ⌘A: we don't know `totalLines`, so select to `EOF_LINE`, which
    // `toRangeEnds` translates to `RangeEnd::Eof` at the IPC boundary.
    if (deps.getTotalBytes() > 0) {
      deps.selection.selectToEof()
    }
  }

  function handleEscapeKey(): void {
    const { search } = deps
    deps.logEscape()
    if (!search.searchVisible) {
      deps.closeWindow()
      return
    }
    if (search.searchStatus === 'running') {
      search.stopSearch()
    } else {
      search.closeSearch()
    }
  }

  /**
   * Routes Escape to the right cancel surface in priority order: open context menu
   * (the menu owns its own Escape too, but we short-circuit here so the page's
   * `closeWindow()` path doesn't fire after the menu closes itself), then in-flight
   * copy read, then any open copy dialog, then the search bar logic.
   *
   * Returns `true` if Escape was consumed here.
   */
  function tryConsumeEscapeForCopy(): boolean {
    if (deps.isContextMenuOpen()) {
      deps.closeContextMenu()
      return true
    }
    if (deps.copy.busy) {
      void deps.copy.cancelInFlight()
      return true
    }
    if (deps.isCopyConfirmOpen()) {
      deps.cancelCopyConfirm()
      return true
    }
    if (deps.isCopyRefuseOpen()) {
      deps.dismissCopyRefuse()
      return true
    }
    return false
  }

  /**
   * Whether the focused search input has a range of its own text selected. Only then
   * does ⌘C mean "copy the query"; with a bare caret there's nothing in the input to
   * copy, so the gesture belongs to the file selection instead.
   */
  function searchInputHasSelection(): boolean {
    const input = deps.search.searchInputRef
    if (!input) return false
    const { selectionStart, selectionEnd } = input
    return selectionStart !== null && selectionEnd !== null && selectionStart !== selectionEnd
  }

  /**
   * Handles ⌘/Ctrl-prefixed shortcuts inside the viewer. Returns `true` if the key
   * was consumed; the caller falls through to other handlers when it returns `false`.
   * Defers to the browser's native ⌘A / ⌘C when the search input is focused.
   */
  function handleModifierShortcut(e: KeyboardEvent, searchInputFocused: boolean): boolean {
    if (searchInputFocused) {
      // ⌘F, plus ⌘C when the input holds a bare caret; ⌘A and a ⌘C over selected query
      // text go to the input's native handler.
      if (e.key === 'f') {
        e.preventDefault()
        deps.search.openSearch()
        return true
      }
      if (e.key === 'c' && !searchInputHasSelection()) {
        e.preventDefault()
        deps.runCopy()
        return true
      }
      return false
    }
    if (e.key === 'a') {
      e.preventDefault()
      handleSelectAllShortcut()
      return true
    }
    if (e.key === 'c') {
      e.preventDefault()
      deps.runCopy()
      return true
    }
    if (e.key === 'f') {
      e.preventDefault()
      deps.search.openSearch()
      return true
    }
    return false
  }

  /**
   * Routes unmodified single-letter and navigation keys to their handler.
   * Split out from `handleKeyDown` to keep the latter's cyclomatic
   * complexity below the project lint threshold.
   */
  function handleBareKey(e: KeyboardEvent): boolean {
    // Callers reach here only for unmodified keys (`handleKeyDown` bails on
    // ⌘/⌃/⌥ first), which matters because `handleNavigationKey` sees only `e.key`.
    return (
      handleTailToggleKey(e, deps.toggleTailMode) ||
      handleToggleKey(e, deps.toggleWordWrap) ||
      handleNavigationKey(e.key, deps.scroll)
    )
  }

  /**
   * Keys carrying ⌘ / ⌃ / ⌥. Nothing here falls through to the bare-key handlers,
   * which is the point: matching only part of a combo is how `⌥⌘C` would land on
   * "copy" instead of the search chord it actually is.
   */
  function handleModifiedKey(e: KeyboardEvent, searchInputFocused: boolean): void {
    // Search-mode chords (⌘⌥R for regex, ⌘⌥C for case) work whenever the search
    // bar is visible, even if the input has focus. Checked before the generic
    // modifier-shortcut handler so the alt-bearing chord wins.
    if (
      deps.search.searchVisible &&
      handleSearchToggleKey(e, {
        toggleUseRegex: deps.search.toggleUseRegex,
        toggleCaseSensitive: deps.search.toggleCaseSensitive,
      })
    ) {
      e.preventDefault()
      return
    }

    // ⌥⇧/⌃⇧+Arrow (word) and ⌘⇧+Up/Down (file edge). Gated on focus by hand, because
    // unlike the unmodified path this handler runs even while the search input has it,
    // and without the gate it would steal the input's own ⌥⇧Arrow.
    if (!searchInputFocused && tryExtendSelection(e)) {
      e.preventDefault()
      return
    }

    // Exactly ⌘/⌃ + letter: ⌘⌥C is the chord above and ⌘⇧A means nothing here.
    if (!e.altKey && !e.shiftKey) handleModifierShortcut(e, searchInputFocused)
  }

  function handleKeyDown(e: KeyboardEvent): void {
    const { search } = deps
    const searchInputFocused = search.searchVisible && document.activeElement === search.searchInputRef

    if (e.metaKey || e.ctrlKey || e.altKey) {
      handleModifiedKey(e, searchInputFocused)
      return
    }

    // Everything below is an unmodified key. Shift is a modifier we DO read here: it
    // picks findPrev on Enter and extends the selection on the arrows and Home/End.
    if (e.key === 'Escape') {
      e.preventDefault()
      if (tryConsumeEscapeForCopy()) return
      handleEscapeKey()
      return
    }

    if (e.key === 'Enter' && search.searchVisible) {
      e.preventDefault()
      if (e.shiftKey) search.findPrev()
      else search.findNext()
      return
    }

    if (searchInputFocused) return

    // Plain Shift+Arrow and Shift+Home/End land here. AFTER the focus guard, or this
    // would steal the search input's own Shift+Arrow; BEFORE `handleBareKey`, or
    // Shift+Up would keep scrolling instead of extending.
    if (tryExtendSelection(e)) {
      e.preventDefault()
      return
    }

    if (handleBareKey(e)) e.preventDefault()
  }

  return { handleKeyDown, handleSelectAllShortcut, resetDesiredColumn }
}
