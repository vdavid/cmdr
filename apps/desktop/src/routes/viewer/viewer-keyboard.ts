import { eventMatchesCommand } from '#lib/shortcuts/index.js'
import { holdsPrimaryModifier } from '#lib/shortcuts/primary-modifier.js'
import { isMacOS } from '#lib/platform/index.js'
import { EOF_ROW, type RowOffset, type SelectAllArgs, type Selection } from './selection.svelte'
import { moveFocus, type CaretMotion, type MotionDirection } from './viewer-caret-motion'

/** Scroll actions the unmodified navigation keys drive. */
interface ScrollActions {
  scrollByRows: (n: number) => void
  scrollByPages: (n: number) => void
  scrollToStart: () => void
  scrollToEnd: () => void
  /** Horizontal scroll by `n` columns. A no-op under word wrap (nothing overflows). */
  scrollByColumns: (n: number) => void
}

/** Everything the keyboard asks of the scroll composable. */
interface NavigationActions extends ScrollActions {
  /** Scrolls a line just into view vertically, leaving an already-visible one alone. */
  ensureRowVisible: (line: number) => void
  /** Scrolls the character at `point` into view horizontally. */
  ensureColumnVisible: (point: RowOffset) => void
}

/** Maps Arrow / Page / Home / End keys to viewer scroll actions. Returns true if handled. */
export function handleNavigationKey(key: string, actions: ScrollActions): boolean {
  switch (key) {
    case 'ArrowUp':
      actions.scrollByRows(-1)
      return true
    case 'ArrowDown':
      actions.scrollByRows(1)
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
 * header names this window's case: its fixed chords match locally but split on "carries
 * ⌘/⌃/⌥" up front.
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
      return rowEdgeMotion(e, -1)
    case 'End':
      return rowEdgeMotion(e, 1)
    default:
      return null
  }
}

/**
 * Shift+Arrow steps a character; ⌥ (macOS) or Ctrl (Windows, Linux) promotes it to a word.
 * macOS also takes ⌃⇧Arrow as a word, though the system usually eats it first, which costs
 * nothing to support. Off macOS Alt+Shift+Arrow is no motion: Alt+← / → is Back / Forward there.
 */
function horizontalMotion(e: KeyboardEvent, direction: MotionDirection): CaretMotion | null {
  if (!e.shiftKey) return null
  if (e.metaKey) return null
  if (!isMacOS() && e.altKey) return null
  const byWord = e.altKey || e.ctrlKey
  return { kind: byWord ? 'word' : 'char', direction }
}

/** Shift+Up/Down steps a logical line; on macOS ⌘ promotes it to the file edge. */
function verticalMotion(e: KeyboardEvent, direction: MotionDirection): CaretMotion | null {
  if (!e.shiftKey) return null
  // Off macOS ⌘ is Super, the desktop's key; the file edge is Ctrl+Shift+Home / End there.
  if (e.metaKey) return isMacOS() ? { kind: 'docEdge', direction } : null
  if (e.altKey || e.ctrlKey) return null
  return { kind: 'row', direction }
}

/**
 * Shift+Home/End extends to the LINE edge, while bare Home/End scroll to the file edge.
 * That difference is on purpose: unmodified Home/End are scroll-view navigation (what
 * macOS does in a document view), and a selection gesture works on the line in every
 * editor; ⌘+Shift+Up/Down (macOS) or Ctrl+Shift+Home/End (Windows, Linux) then promotes it
 * to the whole file. Don't harmonize them.
 */
function rowEdgeMotion(e: KeyboardEvent, direction: MotionDirection): CaretMotion | null {
  if (!e.shiftKey) return null
  if (isDocEdgeChord(e)) return { kind: 'docEdge', direction }
  if (e.metaKey || e.altKey || e.ctrlKey) return null
  return { kind: 'rowEdge', direction }
}

/** Ctrl (and only Ctrl, Shift aside) on Home / End off macOS: the document's edge, as in every editor there. */
function isDocEdgeChord(e: KeyboardEvent): boolean {
  return !isMacOS() && e.ctrlKey && !e.metaKey && !e.altKey
}

/** Ctrl+Home / Ctrl+End off macOS scroll to the file's edge, as bare Home / End do. Returns true if handled. */
function handleDocEdgeScroll(e: KeyboardEvent, actions: ScrollActions): boolean {
  if (e.shiftKey || !isDocEdgeChord(e)) return false
  switch (e.key) {
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
 * Word wrap's key (`viewer.toggleWordWrap`, `W` by default). Returns true if handled. The
 * key comes from the shortcuts registry, so the user can rebind it or turn it off (WCAG
 * 2.1.4); the whole-combo match means a default `W` never fires on `⌘W`.
 */
export function handleToggleKey(e: KeyboardEvent, toggleWordWrap: () => void): boolean {
  if (!eventMatchesCommand(e, 'viewer.toggleWordWrap')) return false
  toggleWordWrap()
  return true
}

/**
 * Tail mode's key (`viewer.toggleTail`, `F` by default). Returns true if handled. Like word
 * wrap's, it comes from the registry; the whole-combo match keeps `F` off `⌘F` (search).
 */
export function handleTailToggleKey(e: KeyboardEvent, toggleTailMode: () => void): boolean {
  if (!eventMatchesCommand(e, 'viewer.toggleTail')) return false
  toggleTailMode()
  return true
}

interface SearchToggleActions {
  toggleUseRegex: () => void
  toggleCaseSensitive: () => void
}

/**
 * The search bar's regex and case toggles (`viewer.toggleSearchRegex`, `viewer.toggleSearchCase`:
 * ⌘⌥R / ⌘⌥C on macOS, Alt+R / Alt+C on Windows and Linux). Returns true if handled; the caller
 * is responsible for `preventDefault`. Both keys hold a modifier, so they work while the search
 * input has focus without typing into it.
 */
export function handleSearchToggleKey(e: KeyboardEvent, actions: SearchToggleActions): boolean {
  if (eventMatchesCommand(e, 'viewer.toggleSearchRegex')) {
    actions.toggleUseRegex()
    return true
  }
  if (eventMatchesCommand(e, 'viewer.toggleSearchCase')) {
    actions.toggleCaseSensitive()
    return true
  }
  return false
}

/** What "is the search input the thing being typed into?" needs to know. */
export interface SearchFocusState {
  searchVisible: boolean
  searchInputRef: HTMLInputElement | null | undefined
}

/**
 * Whether the search bar is up AND its input holds keyboard focus. The viewer's single
 * notion of "the user is typing in the search box"; ⌘C / ⌘A and the Edit menu's Copy /
 * Select all all branch on it, and they have to agree.
 */
export function isSearchInputFocused(search: SearchFocusState): boolean {
  return search.searchVisible && document.activeElement === search.searchInputRef
}

/**
 * Whether `input` has a range of its own text selected. Only then does Copy mean "copy the
 * query"; with a bare caret there's nothing in the input to copy, so the gesture belongs to
 * the file selection instead.
 */
export function inputHasSelection(input: HTMLInputElement | null | undefined): boolean {
  if (!input) return false
  const { selectionStart, selectionEnd } = input
  return selectionStart !== null && selectionEnd !== null && selectionStart !== selectionEnd
}

interface KeyboardDeps {
  /** Total line count, or `null` in ByteSeek-no-index mode before an index exists. */
  getTotalRows: () => number | null
  /** Total byte count of the file (drives the ByteSeek-no-index ⌘A fallback). */
  getTotalBytes: () => number
  /**
   * The text the view SHOWS for a line, or `undefined` when no row is drawn for it yet
   * (scroll to it and the next press lands). ❌ Never the raw line cache: a rendered row
   * the cache missed draws empty, and a model that calls it non-existent goes dead on a
   * file ending in a newline. Wire it to `createViewerScroll.renderedRowText`.
   */
  getRowText: (line: number) => string | undefined
  /**
   * The last line currently rendered, or `null` when nothing is. Only ever read to
   * resolve an end-of-file sentinel focus onto a real line (see `resolveFrom`).
   */
  getLastRenderedRow: () => number | null
  selection: {
    /** The current selection, so an extend chord has an anchor to keep and a focus to move. */
    readonly selection: Selection | null
    /** Selects the whole file given its total line count and the last line's length. */
    selectAll: (args: SelectAllArgs) => void
    /** Selects the whole file when its line count isn't known yet (ByteSeek, no index). */
    selectToEof: () => void
    /** Moves the focus, keeping the anchor. The whole of keyboard extension. */
    setFocus: (point: RowOffset) => void
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
  cancelCopyConfirm: () => void
  dismissCopyRefuse: () => void
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
   * ByteSeek-no-index mode parks the focus on `EOF_ROW`. The sentinel names a line that
   * can never be cached, so the refusal has to happen here rather than in the pure
   * module, which knows nothing about what's on screen and could only hand the sentinel
   * back as its own `targetRow` — slamming the view to the bottom on every press with no
   * way to shrink the selection. Reaching the sentinel always scrolled to the bottom, so
   * the last rendered line is the practical end of the file. Returns `null` (a no-op
   * press) when nothing is rendered.
   */
  function resolveFrom(focus: RowOffset): RowOffset | null {
    if (focus.row !== EOF_ROW) return focus
    const row = deps.getLastRenderedRow()
    if (row === null) return null
    return { row, offset: deps.getRowText(row)?.length ?? 0 }
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
      getRowText: deps.getRowText,
      getTotalRows: deps.getTotalRows,
      desiredColumn,
    })
    desiredColumn = result.desiredColumn

    if (result.focus !== null) deps.selection.setFocus(result.focus)

    // Unconditional, so the uncached-line case heals itself: with no offset to land on,
    // the selection stays put and this scroll is what pulls the line into the render
    // window and triggers its fetch, so the next press lands instead of the key being
    // dead forever. `targetRow` can be the end-of-file sentinel, which `ensureRowVisible`
    // reads as "the end of the file"; it is the one place that branch lives.
    deps.scroll.ensureRowVisible(result.targetRow)
    if (result.focus !== null && result.focus.row !== EOF_ROW) deps.scroll.ensureColumnVisible(result.focus)
    return true
  }

  function handleSelectAllShortcut(): void {
    // ⌘A is a fresh gesture, so a Shift+Up right after it aims from the new focus rather
    // than from whatever column an earlier run was heading for.
    resetDesiredColumn()
    const totalRows = deps.getTotalRows()
    const lastRowText = totalRows !== null && totalRows > 0 ? deps.getRowText(totalRows - 1) : undefined
    if (totalRows !== null && lastRowText !== undefined) {
      deps.selection.selectAll({ totalRows, lastRowLength: lastRowText.length })
      return
    }
    // Two ways to not know where the file ends: no line count at all (ByteSeek before
    // its index lands), or a last line nobody has fetched, which is the ordinary state
    // of a long file the user hasn't scrolled to the end of. Both take the same road:
    // `EOF_ROW`, which `toRangeEnds` turns into `RangeEnd::Eof` so the backend resolves
    // the real end. ❌ Never substitute a length of 0 for an absent last line: the
    // selection then stops at the start of that line and ⌘C quietly drops it.
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
   * Routes Escape to the right cancel surface in priority order: in-flight copy read,
   * then any open copy dialog, then the search bar logic. (The right-click menu is
   * native and swallows its own Escape, so the webview never sees it.)
   *
   * Returns `true` if Escape was consumed here.
   */
  function tryConsumeEscapeForCopy(): boolean {
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
   * Handles the primary-modifier shortcuts inside the viewer: search (`viewer.search`, from the
   * registry), then copy and select all (⌘ on macOS, Ctrl elsewhere; never the other one, so ⌃C
   * doesn't copy on macOS nor Super+C on Linux). Returns `true` if the key was consumed; the
   * caller falls through to other handlers when it returns `false`. Defers to the browser's
   * native ⌘A / ⌘C when the search input is focused.
   */
  function handleModifierShortcut(e: KeyboardEvent, searchInputFocused: boolean): boolean {
    if (eventMatchesCommand(e, 'viewer.search')) {
      e.preventDefault()
      deps.search.openSearch()
      return true
    }
    // Exactly ⌘/⌃ + letter: ⌘⌥C is the search toggle and ⌘⇧A means nothing here.
    if (e.altKey || e.shiftKey || !holdsPrimaryModifier(e)) return false
    if (searchInputFocused) {
      // ⌘C when the input holds a bare caret; ⌘A and a ⌘C over selected query text go to the
      // input's native handler.
      if (e.key === 'c' && !inputHasSelection(deps.search.searchInputRef)) {
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
    return false
  }

  /**
   * The viewer's registry keys (tail mode, word wrap). Asked before the modifier split,
   * because a rebinding may carry a modifier (`⌥W`), and a whole-combo match can't confuse
   * a default `F` with `⌘F`.
   */
  function handleRegistryKey(e: KeyboardEvent): boolean {
    return handleTailToggleKey(e, deps.toggleTailMode) || handleToggleKey(e, deps.toggleWordWrap)
  }

  /**
   * Keys carrying ⌘ / ⌃ / ⌥. Nothing here falls through to the bare-key handlers,
   * which is the point: matching only part of a combo is how `⌥⌘C` would land on
   * "copy" instead of the search chord it actually is.
   */
  function handleModifiedKey(e: KeyboardEvent, searchInputFocused: boolean): void {
    // The search toggles (regex, case) work whenever the search bar is visible, even if the
    // input has focus. Checked before the generic modifier-shortcut handler so the
    // alt-bearing chord wins.
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

    // Ctrl+Home / Ctrl+End off macOS: the file's edge. Not in the search input, whose own
    // caret keys they are.
    if (!searchInputFocused && handleDocEdgeScroll(e, deps.scroll)) {
      e.preventDefault()
      return
    }

    handleModifierShortcut(e, searchInputFocused)
  }

  function handleKeyDown(e: KeyboardEvent): void {
    const { search } = deps
    const searchInputFocused = isSearchInputFocused(search)

    // Not while the search input has focus: there a letter is a letter.
    if (!searchInputFocused && handleRegistryKey(e)) {
      e.preventDefault()
      return
    }

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
    // would steal the search input's own Shift+Arrow; BEFORE the navigation keys, or
    // Shift+Up would keep scrolling instead of extending.
    if (tryExtendSelection(e)) {
      e.preventDefault()
      return
    }

    // Only unmodified keys reach here (⌘/⌃/⌥ returned above), which matters because
    // `handleNavigationKey` sees only `e.key`.
    if (handleNavigationKey(e.key, deps.scroll)) e.preventDefault()
  }

  return { handleKeyDown, handleSelectAllShortcut, resetDesiredColumn }
}
